//! Vector PDF backend for DOCX display lists.
//!
//! A page is drawn from the same display list the raster backend draws, in the same order,
//! with the same geometry: glyph runs become PDF text in the very fonts the layout measured
//! (subset and embedded, so the text selects, searches and copies), images keep their encoded
//! bytes, and rules, borders, decorations and shapes are the fills and strokes
//! [`docx_raster::record_primitive`] records from the raster painters. What only a pixmap can
//! draw (a shadow, a gradient, text without a resolved font, an image format PDF cannot hold)
//! is drawn by the raster backend at [`PdfOptions::raster_scale`] and placed as an image, so
//! nothing the screen shows goes missing from the PDF.
//!
//! A font whose license forbids embedding (OS/2 `fsType` restricted, or bitmap-only) is drawn
//! as glyph outlines: it looks the same and prints the same, but does not select as text.
//! [`PdfOutput::outlined_fonts`] names those fonts so a host can say so.

#[cfg(target_arch = "wasm32")]
compile_error!("nimbleoffice-docx-pdf is server-side only");

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::{Range, RangeInclusive};

use docx_layout::display_list::{
    DecoKind, DisplayList, DisplayPage, DocAttrs, GlyphRunPrimitive, Primitive,
};
use docx_raster::{
    GlyphCache, ImageCache, ImageScope, NoteKind, RecordedGlyph, RenderResources, VectorOp,
    glyph_run_has_drawn_shadow, glyph_run_transform, image_placement, primitive_clip,
    primitive_paint_opacity, rasterize_primitives, record_page_borders, record_primitive,
    record_text,
};
use krilla::Data;
use krilla::Document;
use krilla::action::LinkAction;
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::color::rgb;
use krilla::destination::{Destination, XyzDestination};
use krilla::geom::{Path, PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, Stroke};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};
use ooxml_text::{FontId, FontStore};
use serde_json::Number;

/// PDF points per display-list pixel (CSS px, 96 per inch).
const PT_PER_PX: f32 = 0.75;

/// A heading to list in the PDF's bookmarks (its outline).
#[derive(Clone, Debug)]
pub struct Heading {
    /// The paragraph's `w14:paraId`, as the display list stamps it.
    pub para_id: String,
    /// Zero-based outline level, 0 for Heading 1.
    pub level: u8,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct PdfOptions {
    /// Page ordinals (from 0) to export; every page when `None`.
    pub pages: Option<RangeInclusive<usize>>,
    /// Headings in document order, for the bookmarks.
    pub headings: Vec<Heading>,
    /// Bookmark names to the paragraph each starts in, for links to a place in the document.
    pub bookmarks: HashMap<String, String>,
    pub title: Option<String>,
    pub author: Option<String>,
    /// Device pixels per page pixel for what is drawn as an image.
    pub raster_scale: f32,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            pages: None,
            headings: Vec::new(),
            bookmarks: HashMap::new(),
            title: None,
            author: None,
            raster_scale: 3.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PdfOutput {
    pub bytes: Vec<u8>,
    /// Pages written.
    pub pages: usize,
    /// Families drawn as outlines because their license forbids embedding.
    pub outlined_fonts: Vec<String>,
    /// Places drawn as images rather than vectors.
    pub raster_patches: usize,
    /// Primitives left out because neither backend could draw them.
    pub skipped_primitives: usize,
    /// Effects the raster backend leaves undrawn, by kind (as `RenderedPage` reports them).
    pub skipped_effects: BTreeMap<String, usize>,
    /// Primitives drawn as images, by why: `text` (no resolved font), `shadow`, `image`
    /// (a format PDF does not hold), `shape` (a gradient or shadow), `glyphRun` (font unusable).
    pub rasterized: BTreeMap<String, usize>,
}

/// Writes `display_list`'s pages as a PDF.
pub fn render_pdf(
    display_list: &DisplayList,
    resources: &RenderResources<'_>,
    glyphs: &mut GlyphCache,
    images: &mut ImageCache,
    options: &PdfOptions,
) -> Result<PdfOutput, String> {
    if !options.raster_scale.is_finite() || options.raster_scale <= 0.0 {
        return Err("raster scale must be finite and positive".into());
    }
    let selected: Vec<usize> = (0..display_list.pages.len())
        .filter(|ordinal| {
            options
                .pages
                .as_ref()
                .is_none_or(|range| range.contains(ordinal))
        })
        .collect();
    if selected.is_empty() {
        return Err("no page to export".into());
    }
    let exported: HashMap<usize, usize> = selected
        .iter()
        .enumerate()
        .map(|(index, ordinal)| (*ordinal, index))
        .collect();
    let places = paragraph_places(display_list, &exported);

    let mut document = Document::new();
    let mut fonts = FontTable::new(resources.fonts);
    let mut stats = Stats::default();
    for &ordinal in &selected {
        let page = &display_list.pages[ordinal];
        let width = px(&page.width)?;
        let height = px(&page.height)?;
        let settings = PageSettings::from_wh(width * PT_PER_PX, height * PT_PER_PX)
            .ok_or_else(|| format!("page {ordinal} has no area"))?;
        let mut pdf_page = document.start_page_with(settings);
        let links = {
            let mut surface = pdf_page.surface();
            surface.push_transform(&Transform::from_scale(PT_PER_PX, PT_PER_PX));
            let mut painter = Painter {
                surface: &mut surface,
                page,
                resources,
                glyphs: &mut *glyphs,
                images: &mut *images,
                fonts: &mut fonts,
                stats: &mut stats,
                scale: options.raster_scale,
                pending: Vec::new(),
                links: Vec::new(),
            };
            painter.paint_page()?;
            let links = std::mem::take(&mut painter.links);
            surface.pop();
            surface.finish();
            links
        };
        for link in merge_links(links) {
            let Some(target) = link_target(&link.target, options, &places) else {
                continue;
            };
            let rect = Rect::from_xywh(
                link.rect.0 * PT_PER_PX,
                link.rect.1 * PT_PER_PX,
                link.rect.2 * PT_PER_PX,
                link.rect.3 * PT_PER_PX,
            );
            if let Some(rect) = rect {
                pdf_page.add_annotation(Annotation::new_link(
                    LinkAnnotation::new(rect, target),
                    None,
                ));
            }
        }
        pdf_page.finish();
    }
    let titles = heading_texts(display_list, &options.headings);
    if let Some(outline) = outline(&options.headings, &titles, &places) {
        document.set_outline(outline);
    }
    let mut metadata = Metadata::new().producer("NimbleOffice".into());
    if let Some(title) = options.title.clone().filter(|title| !title.is_empty()) {
        metadata = metadata.title(title);
    }
    if let Some(author) = options.author.clone().filter(|author| !author.is_empty()) {
        metadata = metadata.authors(vec![author]);
    }
    document.set_metadata(metadata);
    let bytes = document
        .finish()
        .map_err(|error| format!("PDF serialization failed: {error:?}"))?;
    Ok(PdfOutput {
        bytes,
        pages: selected.len(),
        outlined_fonts: fonts.outlined.into_iter().collect(),
        raster_patches: stats.patches,
        skipped_primitives: stats.skipped,
        skipped_effects: stats.effects,
        rasterized: stats.rasterized,
    })
}

#[derive(Default)]
struct Stats {
    rasterized: BTreeMap<String, usize>,
    patches: usize,
    skipped: usize,
    effects: BTreeMap<String, usize>,
}

/// Where a paragraph first paints in the export, by its `w14:paraId` and by the engine's
/// block key (`body:p12`, what the engine names a paragraph without one): exported page
/// index and its top-left, in page pixels.
type Places = HashMap<String, (usize, f32, f32)>;

fn paragraph_places(list: &DisplayList, exported: &HashMap<usize, usize>) -> Places {
    let mut places = Places::new();
    for (ordinal, page) in list.pages.iter().enumerate() {
        let Some(&index) = exported.get(&ordinal) else {
            continue;
        };
        for primitive in &page.primitives {
            let attrs = attrs(primitive);
            for key in [attrs.para_id.as_ref(), attrs.block_key.as_ref()]
                .into_iter()
                .flatten()
            {
                if places.contains_key(key) {
                    continue;
                }
                if let Some((x, y)) = primitive_top_left(primitive) {
                    places.insert(key.clone(), (index, x, y));
                }
            }
        }
    }
    places
}

/// The painted text of each heading without a title, in paint order: its list number too.
fn heading_texts<'h>(list: &DisplayList, headings: &'h [Heading]) -> HashMap<&'h str, String> {
    let mut texts: HashMap<&str, String> = headings
        .iter()
        .filter(|heading| heading.title.is_empty())
        .map(|heading| (heading.para_id.as_str(), String::new()))
        .collect();
    if texts.is_empty() {
        return texts;
    }
    for page in &list.pages {
        for primitive in &page.primitives {
            let text = match primitive {
                Primitive::GlyphRun(run) if !run.hidden => &run.text,
                Primitive::Text(run) if !run.hidden => &run.text,
                _ => continue,
            };
            let attrs = attrs(primitive);
            let key = [attrs.para_id.as_deref(), attrs.block_key.as_deref()]
                .into_iter()
                .flatten()
                .find(|key| texts.contains_key(key));
            if let Some(out) = key.and_then(|key| texts.get_mut(key)) {
                if attrs.list_marker == Some(true) {
                    out.push_str(text);
                    out.push(' ');
                } else {
                    out.push_str(text);
                }
            }
        }
    }
    texts
}

fn primitive_top_left(primitive: &Primitive) -> Option<(f32, f32)> {
    let number = |value: &Number| value.as_f64().map(|value| value as f32);
    match primitive {
        Primitive::GlyphRun(run) => {
            let first = run.glyphs.first()?;
            Some((first.x as f32, (first.y - run.size) as f32))
        }
        Primitive::Text(run) => Some((number(&run.x)?, number(&run.baseline_y)? - 16.0)),
        Primitive::Rect(rect) => Some((number(&rect.x)?, number(&rect.y)?)),
        Primitive::Image(image) => Some((number(&image.x)?, number(&image.y)?)),
        Primitive::Shape(shape) => Some((number(&shape.x)?, number(&shape.y)?)),
        Primitive::Decoration(_) | Primitive::Line(_) => None,
    }
}

fn attrs(primitive: &Primitive) -> &DocAttrs {
    match primitive {
        Primitive::Text(value) => &value.attrs,
        Primitive::GlyphRun(value) => &value.attrs,
        Primitive::Rect(value) => &value.attrs,
        Primitive::Line(value) => &value.attrs,
        Primitive::Image(value) => &value.attrs,
        Primitive::Shape(value) => &value.attrs,
        Primitive::Decoration(value) => &value.attrs,
    }
}

fn px(number: &Number) -> Result<f32, String> {
    number
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| "page size must be finite and positive".to_string())
}

/// Where a link goes: an address outside, or a bookmark inside.
#[derive(Clone, Debug, PartialEq)]
enum LinkGoal {
    Uri(String),
    Bookmark(String),
}

struct Link {
    /// x, y, width, height in page pixels.
    rect: (f32, f32, f32, f32),
    target: LinkGoal,
}

fn link_goal(attrs: &DocAttrs) -> Option<LinkGoal> {
    if let Some(location) = attrs
        .link_doc_location
        .as_deref()
        .filter(|name| !name.is_empty())
    {
        return Some(LinkGoal::Bookmark(location.to_owned()));
    }
    let href = attrs.href.as_deref()?.trim();
    if let Some(name) = href.strip_prefix('#') {
        return (!name.is_empty()).then(|| LinkGoal::Bookmark(name.to_owned()));
    }
    let scheme = href.split_once(':')?.0.to_ascii_lowercase();
    matches!(scheme.as_str(), "http" | "https" | "mailto" | "tel" | "ftp")
        .then(|| LinkGoal::Uri(href.to_owned()))
}

fn link_target(goal: &LinkGoal, options: &PdfOptions, places: &Places) -> Option<Target> {
    match goal {
        LinkGoal::Uri(uri) => Some(Target::Action(LinkAction::new(uri.clone()).into())),
        LinkGoal::Bookmark(name) => {
            let para_id = options.bookmarks.get(name)?;
            let (page, _, y) = places.get(para_id)?;
            Some(Target::Destination(Destination::Xyz(XyzDestination::new(
                *page,
                Point::from_xy(0.0, (y * PT_PER_PX).max(0.0)),
            ))))
        }
    }
}

/// Joins the pieces of one link that sit side by side on a line, as a run split by
/// formatting paints them.
fn merge_links(links: Vec<Link>) -> Vec<Link> {
    let mut merged: Vec<Link> = Vec::new();
    for link in links {
        if let Some(last) = merged.last_mut()
            && last.target == link.target
            && (last.rect.1 - link.rect.1).abs() < 1.0
            && (last.rect.3 - link.rect.3).abs() < 1.0
            && link.rect.0 <= last.rect.0 + last.rect.2 + 2.0
            && link.rect.0 + link.rect.2 >= last.rect.0 - 2.0
        {
            let left = last.rect.0.min(link.rect.0);
            let right = (last.rect.0 + last.rect.2).max(link.rect.0 + link.rect.2);
            last.rect.0 = left;
            last.rect.2 = right - left;
            continue;
        }
        merged.push(link);
    }
    merged
}

/// The bookmarks: headings nested by level, each pointing at where it paints.
fn outline(
    headings: &[Heading],
    texts: &HashMap<&str, String>,
    places: &Places,
) -> Option<Outline> {
    struct Node {
        level: u8,
        title: String,
        page: usize,
        y: f32,
        children: Vec<Node>,
    }
    fn into_outline(node: Node) -> OutlineNode {
        let mut outline = OutlineNode::new(
            node.title,
            XyzDestination::new(
                node.page,
                Point::from_xy(0.0, (node.y * PT_PER_PX).max(0.0)),
            ),
        );
        for child in node.children {
            outline.push_child(into_outline(child));
        }
        outline
    }
    let mut roots: Vec<Node> = Vec::new();
    for heading in headings {
        let Some(&(page, _, y)) = places.get(&heading.para_id) else {
            continue;
        };
        let title = if heading.title.is_empty() {
            texts
                .get(heading.para_id.as_str())
                .map_or("", String::as_str)
        } else {
            heading.title.as_str()
        };
        let title: String = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() {
            continue;
        }
        let node = Node {
            level: heading.level,
            title,
            page,
            y,
            children: Vec::new(),
        };
        // Descend along the last nodes while they are shallower than this heading.
        let mut siblings = &mut roots;
        while siblings.last().is_some_and(|last| last.level < node.level) {
            siblings = &mut siblings.last_mut().expect("checked").children;
        }
        siblings.push(node);
    }
    if roots.is_empty() {
        return None;
    }
    let mut outline = Outline::new();
    for root in roots {
        outline.push_child(into_outline(root));
    }
    Some(outline)
}

/// The fonts of one export, by the store id owning their bytes.
struct FontTable<'a> {
    store: &'a FontStore,
    fonts: HashMap<u32, Option<PdfFont>>,
    outlined: BTreeSet<String>,
}

#[derive(Clone)]
struct PdfFont {
    font: Font,
    /// Licensed for viewing only: drawn as outlines, never embedded.
    outlined: bool,
    glyphs: u16,
}

impl<'a> FontTable<'a> {
    fn new(store: &'a FontStore) -> Self {
        Self {
            store,
            fonts: HashMap::new(),
            outlined: BTreeSet::new(),
        }
    }

    fn get(&mut self, id: FontId) -> Option<PdfFont> {
        let owner = self.store.byte_owner(id).ok()?;
        if let Some(font) = self.fonts.get(&owner.to_u32()) {
            return font.clone();
        }
        let font = self.load(owner);
        self.fonts.insert(owner.to_u32(), font.clone());
        font
    }

    fn load(&mut self, owner: FontId) -> Option<PdfFont> {
        let bytes = self.store.font_bytes(owner).ok()?;
        let tables = TableDirectory::new(bytes)?;
        let glyphs = tables
            .table(bytes, b"maxp")
            .and_then(|maxp| be16(maxp, 4))
            .unwrap_or(u16::MAX);
        let outlined = !tables
            .table(bytes, b"OS/2")
            .and_then(|os2| be16(os2, 8))
            .is_none_or(embeddable);
        let font = Font::new(Data::from(bytes.to_vec()), 0)?;
        if outlined {
            let family = self
                .store
                .declared_families(owner)
                .ok()
                .and_then(|names| names.into_iter().next())
                .unwrap_or_else(|| "?".into());
            self.outlined.insert(family);
        }
        Some(PdfFont {
            font,
            outlined,
            glyphs,
        })
    }
}

/// OS/2 `fsType`: restricted-license embedding (2 in the low nibble) and bitmap-only
/// embedding (bit 9) both forbid embedding the outlines.
fn embeddable(fs_type: u16) -> bool {
    fs_type & 0x000f != 0x0002 && fs_type & 0x0200 == 0
}

struct TableDirectory {
    records: Vec<([u8; 4], usize, usize)>,
}

impl TableDirectory {
    fn new(bytes: &[u8]) -> Option<Self> {
        let count = usize::from(be16(bytes, 4)?);
        let mut records = Vec::with_capacity(count);
        for index in 0..count {
            let at = 12 + 16 * index;
            let tag: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
            let offset = be32(bytes, at + 8)? as usize;
            let length = be32(bytes, at + 12)? as usize;
            records.push((tag, offset, length));
        }
        Some(Self { records })
    }

    fn table<'b>(&self, bytes: &'b [u8], tag: &[u8; 4]) -> Option<&'b [u8]> {
        let (_, offset, length) = self.records.iter().find(|record| &record.0 == tag)?;
        bytes.get(*offset..offset.checked_add(*length)?)
    }
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Draws one page, keeping consecutive primitives only a pixmap draws for one raster patch.
struct Painter<'a, 'p, 'f, 'k> {
    surface: &'a mut Surface<'p>,
    page: &'k DisplayPage,
    resources: &'a RenderResources<'a>,
    glyphs: &'a mut GlyphCache,
    images: &'a mut ImageCache,
    fonts: &'a mut FontTable<'f>,
    stats: &'a mut Stats,
    scale: f32,
    pending: Vec<(&'k Primitive, ImageScope<'k>)>,
    links: Vec<Link>,
}

impl<'k> Painter<'_, '_, '_, 'k> {
    fn paint_page(&mut self) -> Result<(), String> {
        let page = self.page;
        if let Some(background) = &page.background {
            let color = ooxml_raster::parse_css_color(background)?;
            let rect = tiny_skia::Rect::from_xywh(0.0, 0.0, px(&page.width)?, px(&page.height)?)
                .ok_or_else(|| "invalid page background".to_string())?;
            self.draw_ops(&[VectorOp::Fill {
                path: tiny_skia::PathBuilder::from_rect(rect),
                color,
                rule: tiny_skia::FillRule::Winding,
                transform: tiny_skia::Transform::identity(),
            }]);
        }
        self.draw_ops(&record_page_borders(page, true)?);
        let (watermark, body) = page.primitives.split_at(
            page.watermark_primitive_count
                .unwrap_or(0)
                .min(page.primitives.len()),
        );
        for primitive in watermark {
            self.primitive(primitive, ImageScope::Body)?;
        }
        for region in [&page.header, &page.footer].into_iter().flatten() {
            let scope = ImageScope::HeaderFooter(&region.r_id);
            for primitive in &region.primitives {
                self.primitive(primitive, scope)?;
            }
        }
        for primitive in body {
            self.primitive(primitive, ImageScope::Body)?;
        }
        for area in &page.note_areas {
            let kind = match area.kind.as_deref() {
                Some("endnote") => NoteKind::Endnote,
                _ => NoteKind::Footnote,
            };
            let scope = ImageScope::Notes(kind);
            for primitive in area.separator_primitives.iter().chain(&area.primitives) {
                self.primitive(primitive, scope)?;
            }
        }
        self.flush()?;
        self.draw_ops(&record_page_borders(page, false)?);
        Ok(())
    }

    fn primitive(&mut self, primitive: &'k Primitive, scope: ImageScope<'k>) -> Result<(), String> {
        if !printed(primitive) {
            return Ok(());
        }
        if let Some(goal) = link_goal(attrs(primitive))
            && let Some(rect) = link_rect(primitive)
        {
            self.links.push(Link { rect, target: goal });
        }
        let clip = primitive_clip(primitive).ok().flatten();
        match primitive {
            Primitive::GlyphRun(run) if !glyph_run_has_drawn_shadow(run) => {
                if let Some(font) = self.fonts.get(FontId::from_u32(run.font_id)) {
                    self.flush()?;
                    return self.clipped(clip, |painter| painter.glyph_run(primitive, run, &font));
                }
            }
            Primitive::Text(run) => {
                if let Ok(Some(glyphs)) = record_text(primitive, self.resources)
                    && let Some(fonts) = glyphs
                        .iter()
                        .map(|glyph| self.fonts.get(glyph.font))
                        .collect::<Option<Vec<_>>>()
                {
                    self.flush()?;
                    let slot = text_paint_clip(run.paint_clip.as_ref());
                    return self.clipped(clip, |painter| {
                        painter.clipped(slot, |painter| {
                            painter.recorded_glyphs(&glyphs, &fonts);
                            Ok(())
                        })
                    });
                }
            }
            Primitive::Image(_) => {
                if let Ok(Some(placement)) = image_placement(primitive, scope, self.resources)
                    && let Some(image) = pdf_image(&placement.bytes)
                {
                    self.flush()?;
                    return self.clipped(clip, |painter| {
                        painter.image(&placement, image);
                        Ok(())
                    });
                }
            }
            _ => {
                if let Ok(Some(ops)) = record_primitive(primitive) {
                    self.flush()?;
                    return self.clipped(clip, |painter| {
                        painter.draw_ops(&ops);
                        Ok(())
                    });
                }
            }
        }
        let why = match primitive {
            Primitive::GlyphRun(run) if glyph_run_has_drawn_shadow(run) => "shadow",
            Primitive::GlyphRun(_) => "glyphRun",
            Primitive::Text(_) => "text",
            Primitive::Image(_) => "image",
            Primitive::Shape(_) => "shape",
            Primitive::Rect(_) => "rect",
            Primitive::Line(_) => "line",
            Primitive::Decoration(_) => "decoration",
        };
        *self.stats.rasterized.entry(why.to_owned()).or_default() += 1;
        self.pending.push((primitive, scope));
        Ok(())
    }

    fn clipped(
        &mut self,
        clip: Option<tiny_skia::Rect>,
        draw: impl FnOnce(&mut Self) -> Result<(), String>,
    ) -> Result<(), String> {
        let Some(path) =
            clip.and_then(|clip| convert_path(&tiny_skia::PathBuilder::from_rect(clip)))
        else {
            return draw(self);
        };
        self.surface.push_clip_path(&path, &FillRule::NonZero);
        let drawn = draw(self);
        self.surface.pop();
        drawn
    }

    /// Draws what is pending as one raster patch; a patch the raster backend refuses as a
    /// whole is drawn primitive by primitive, leaving out the ones it refuses.
    fn flush(&mut self) -> Result<(), String> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        if self.patch(&pending).is_err() {
            for item in &pending {
                if self.patch(std::slice::from_ref(item)).is_err() {
                    self.stats.skipped += 1;
                }
            }
        }
        Ok(())
    }

    fn patch(&mut self, primitives: &[(&'k Primitive, ImageScope<'k>)]) -> Result<(), String> {
        let (patch, skipped) = rasterize_primitives(
            self.page,
            primitives,
            self.resources,
            self.glyphs,
            self.images,
            self.scale,
        )?;
        for (kind, count) in skipped {
            *self.stats.effects.entry(kind).or_default() += count;
        }
        let Some(patch) = patch else {
            return Ok(());
        };
        let (width, height) = (patch.pixmap.width(), patch.pixmap.height());
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for pixel in patch.pixmap.pixels() {
            let color = pixel.demultiply();
            rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
        }
        let image = Image::from_rgba8(rgba, width, height);
        let Some(size) = Size::from_wh(patch.rect.width(), patch.rect.height()) else {
            return Ok(());
        };
        self.surface
            .push_transform(&Transform::from_translate(patch.rect.x(), patch.rect.y()));
        self.surface.draw_image(image, size);
        self.surface.pop();
        self.stats.patches += 1;
        Ok(())
    }

    fn glyph_run(
        &mut self,
        primitive: &Primitive,
        run: &GlyphRunPrimitive,
        font: &PdfFont,
    ) -> Result<(), String> {
        let size = run.size as f32;
        if run.glyphs.is_empty() || !size.is_finite() || size <= 0.0 {
            return Ok(());
        }
        let color = ooxml_raster::parse_css_color(&run.color)?;
        let opacity = color.alpha() * primitive_paint_opacity(primitive)?;
        let transform = glyph_run_transform(run)?;
        let paint_clip = text_paint_clip(run.paint_clip.as_ref());
        let ranges = cluster_ranges(run);
        let origin = &run.glyphs[0];
        let (start_x, baseline) = (origin.x as f32, origin.y as f32);
        let glyphs: Vec<KrillaGlyph> = run
            .glyphs
            .iter()
            .enumerate()
            .filter(|(_, glyph)| glyph.id < u32::from(font.glyphs))
            .map(|(index, glyph)| {
                let next = run
                    .glyphs
                    .get(index + 1)
                    .map_or(glyph.x + glyph.advance, |next| next.x);
                KrillaGlyph::new(
                    GlyphId::new(glyph.id),
                    (next - glyph.x) as f32 / size,
                    0.0,
                    (baseline - glyph.y as f32) / size,
                    0.0,
                    ranges[index].clone(),
                    None,
                )
            })
            .collect();
        if glyphs.is_empty() {
            return Ok(());
        }
        // A dropped glyph shifts the ones after it; place the run glyph by glyph then.
        let exact = glyphs.len() == run.glyphs.len();
        self.surface.push_transform(&convert_transform(transform));
        let clip =
            paint_clip.and_then(|rect| convert_path(&tiny_skia::PathBuilder::from_rect(rect)));
        if let Some(clip) = &clip {
            self.surface.push_clip_path(clip, &FillRule::NonZero);
        }
        self.surface.set_fill(Some(Fill {
            paint: to_rgb(color).into(),
            opacity: NormalizedF32::new(opacity.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE),
            rule: FillRule::NonZero,
        }));
        self.surface.set_stroke(None);
        if exact {
            self.surface.draw_glyphs(
                Point::from_xy(start_x, baseline),
                &glyphs,
                font.font.clone(),
                &run.text,
                size,
                font.outlined,
            );
        } else {
            for (index, glyph) in run.glyphs.iter().enumerate() {
                if glyph.id >= u32::from(font.glyphs) {
                    continue;
                }
                let single = [KrillaGlyph::new(
                    GlyphId::new(glyph.id),
                    glyph.advance as f32 / size,
                    0.0,
                    0.0,
                    0.0,
                    ranges[index].clone(),
                    None,
                )];
                self.surface.draw_glyphs(
                    Point::from_xy(glyph.x as f32, glyph.y as f32),
                    &single,
                    font.font.clone(),
                    &run.text,
                    size,
                    font.outlined,
                );
            }
        }
        if clip.is_some() {
            self.surface.pop();
        }
        self.surface.pop();
        Ok(())
    }

    /// Draws a text run's glyphs, one PDF text object per stretch in one font, size, color
    /// and transform.
    fn recorded_glyphs(&mut self, glyphs: &[RecordedGlyph], fonts: &[PdfFont]) {
        let mut start = 0;
        while start < glyphs.len() {
            let first = &glyphs[start];
            let font = &fonts[start];
            let mut end = start + 1;
            while end < glyphs.len() {
                let next = &glyphs[end];
                if next.font != first.font
                    || next.size != first.size
                    || next.color != first.color
                    || next.transform != first.transform
                {
                    break;
                }
                end += 1;
            }
            self.glyph_group(&glyphs[start..end], font);
            start = end;
        }
    }

    fn glyph_group(&mut self, glyphs: &[RecordedGlyph], font: &PdfFont) {
        let first = &glyphs[0];
        let size = first.size;
        if !size.is_finite() || size <= 0.0 {
            return;
        }
        let mut text = String::new();
        let mut placed = Vec::new();
        let kept: Vec<&RecordedGlyph> = glyphs
            .iter()
            .filter(|glyph| glyph.id < u32::from(font.glyphs))
            .collect();
        let Some(origin) = kept.first() else {
            return;
        };
        for (index, glyph) in kept.iter().enumerate() {
            let from = text.len();
            text.push_str(&glyph.text);
            let next = kept
                .get(index + 1)
                .map_or(glyph.x + glyph.advance, |next| next.x);
            placed.push(KrillaGlyph::new(
                GlyphId::new(glyph.id),
                (next - glyph.x) / size,
                0.0,
                (origin.y - glyph.y) / size,
                0.0,
                from..text.len(),
                None,
            ));
        }
        self.surface
            .push_transform(&convert_transform(first.transform));
        self.surface.set_stroke(None);
        self.surface.set_fill(Some(Fill {
            paint: to_rgb(first.color).into(),
            opacity: NormalizedF32::new(first.color.alpha()).unwrap_or(NormalizedF32::ONE),
            rule: FillRule::NonZero,
        }));
        self.surface.draw_glyphs(
            Point::from_xy(origin.x, origin.y),
            &placed,
            font.font.clone(),
            &text,
            size,
            font.outlined,
        );
        self.surface.pop();
    }

    fn image(&mut self, placement: &docx_raster::ImagePlacement<'_>, image: Image) {
        let (source_width, source_height) = image.size();
        let (source_width, source_height) = (source_width as f32, source_height as f32);
        let [left, top, right, bottom] = placement.crop;
        let frame = placement.frame;
        let requested_w = source_width * (1.0 - left - right);
        let requested_h = source_height * (1.0 - top - bottom);
        if requested_w <= 0.0 || requested_h <= 0.0 {
            return;
        }
        let Some(size) = Size::from_wh(source_width, source_height) else {
            return;
        };
        let source_to_frame = Transform::from_row(
            frame.width() / requested_w,
            0.0,
            0.0,
            frame.height() / requested_h,
            frame.x() - left * source_width * frame.width() / requested_w,
            frame.y() - top * source_height * frame.height() / requested_h,
        );
        self.surface
            .push_transform(&convert_transform(placement.transform));
        let cropped = placement.crop != [0.0; 4];
        let clip = cropped
            .then(|| convert_path(&tiny_skia::PathBuilder::from_rect(frame)))
            .flatten();
        if let Some(clip) = &clip {
            self.surface.push_clip_path(clip, &FillRule::NonZero);
        }
        let faded = placement.opacity < 1.0;
        if faded {
            self.surface.push_opacity(
                NormalizedF32::new(placement.opacity.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE),
            );
        }
        self.surface.push_transform(&source_to_frame);
        self.surface.draw_image(image, size);
        self.surface.pop();
        if faded {
            self.surface.pop();
        }
        if clip.is_some() {
            self.surface.pop();
        }
        self.surface.pop();
        self.draw_ops(&placement.overlay);
    }

    fn draw_ops(&mut self, ops: &[VectorOp]) {
        for op in ops {
            match op {
                VectorOp::Fill {
                    path,
                    color,
                    rule,
                    transform,
                } => {
                    let Some(path) = convert_path(path) else {
                        continue;
                    };
                    self.surface.push_transform(&convert_transform(*transform));
                    self.surface.set_stroke(None);
                    self.surface.set_fill(Some(Fill {
                        paint: to_rgb(*color).into(),
                        opacity: NormalizedF32::new(color.alpha()).unwrap_or(NormalizedF32::ONE),
                        rule: match rule {
                            tiny_skia::FillRule::Winding => FillRule::NonZero,
                            tiny_skia::FillRule::EvenOdd => FillRule::EvenOdd,
                        },
                    }));
                    self.surface.draw_path(&path);
                    self.surface.pop();
                }
                VectorOp::Stroke {
                    path,
                    color,
                    stroke,
                    transform,
                } => {
                    let Some(path) = convert_path(path) else {
                        continue;
                    };
                    self.surface.push_transform(&convert_transform(*transform));
                    self.surface.set_fill(None);
                    self.surface.set_stroke(Some(Stroke {
                        paint: to_rgb(*color).into(),
                        width: stroke.width,
                        miter_limit: stroke.miter_limit,
                        line_cap: match stroke.line_cap {
                            tiny_skia::LineCap::Butt => LineCap::Butt,
                            tiny_skia::LineCap::Round => LineCap::Round,
                            tiny_skia::LineCap::Square => LineCap::Square,
                        },
                        line_join: match stroke.line_join {
                            tiny_skia::LineJoin::Miter | tiny_skia::LineJoin::MiterClip => {
                                LineJoin::Miter
                            }
                            tiny_skia::LineJoin::Round => LineJoin::Round,
                            tiny_skia::LineJoin::Bevel => LineJoin::Bevel,
                        },
                        opacity: NormalizedF32::new(color.alpha()).unwrap_or(NormalizedF32::ONE),
                        dash: None,
                    }));
                    self.surface.draw_path(&path);
                    self.surface.pop();
                }
            }
        }
        self.surface.set_stroke(None);
    }
}

/// A text run's horizontal paint slot, vertically unbounded unless it says otherwise.
fn text_paint_clip(clip: Option<&docx_layout::display_list::ClipRect>) -> Option<tiny_skia::Rect> {
    let clip = clip?;
    let x = clip.x.as_ref()?.as_f64()? as f32;
    let w = clip.w.as_ref()?.as_f64()? as f32;
    let y = clip
        .y
        .as_ref()
        .and_then(Number::as_f64)
        .map_or(-1e6, |y| y as f32);
    let h = clip
        .h
        .as_ref()
        .and_then(Number::as_f64)
        .map_or(2e6, |h| h as f32);
    tiny_skia::Rect::from_xywh(x, y, w.max(0.0), h.max(0.0))
}

/// Whether Word prints a primitive: proofing marks and comment highlights are screen-only,
/// and hidden text does not print.
fn printed(primitive: &Primitive) -> bool {
    match primitive {
        Primitive::Decoration(decoration) => {
            !matches!(decoration.deco, DecoKind::Spell | DecoKind::CommentRange)
        }
        Primitive::GlyphRun(run) => !run.hidden,
        Primitive::Text(run) => !run.hidden,
        _ => true,
    }
}

/// The box a link's primitive covers, in page pixels.
fn link_rect(primitive: &Primitive) -> Option<(f32, f32, f32, f32)> {
    let number = |value: &Number| value.as_f64().map(|value| value as f32);
    match primitive {
        Primitive::GlyphRun(run) => {
            let left = run
                .glyphs
                .iter()
                .map(|glyph| glyph.x)
                .fold(f64::MAX, f64::min);
            let right = run
                .glyphs
                .iter()
                .map(|glyph| glyph.x + glyph.advance)
                .fold(f64::MIN, f64::max);
            let baseline = run.glyphs.first()?.y;
            (right > left).then_some((
                left as f32,
                (baseline - run.size * 0.9) as f32,
                (right - left) as f32,
                (run.size * 1.15) as f32,
            ))
        }
        Primitive::Text(run) => {
            let size = 16.0;
            Some((
                number(&run.x)?,
                number(&run.baseline_y)? - size * 0.9,
                number(&run.width)?,
                size * 1.15,
            ))
        }
        Primitive::Image(image) => Some((
            number(&image.x)?,
            number(&image.y)?,
            number(&image.w)?,
            number(&image.h)?,
        )),
        _ => None,
    }
}

/// Each glyph's cluster as a byte range of the run's text: from its cluster to the next
/// cluster after it, empty where the text does not split there.
fn cluster_ranges(run: &GlyphRunPrimitive) -> Vec<Range<usize>> {
    let text = &run.text;
    let mut starts: Vec<usize> = run
        .glyphs
        .iter()
        .map(|glyph| glyph.cluster as usize)
        .filter(|start| *start <= text.len() && text.is_char_boundary(*start))
        .collect();
    starts.sort_unstable();
    starts.dedup();
    run.glyphs
        .iter()
        .map(|glyph| {
            let start = glyph.cluster as usize;
            if start > text.len() || !text.is_char_boundary(start) {
                return 0..0;
            }
            let end = starts
                .iter()
                .find(|next| **next > start)
                .copied()
                .unwrap_or(text.len());
            start..end
        })
        .collect()
}

/// The image formats a PDF holds as they are; others are drawn by the raster backend.
fn pdf_image(bytes: &[u8]) -> Option<Image> {
    let data = || Data::from(bytes.to_vec());
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Image::from_png(data(), true).ok()
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Image::from_jpeg(data(), true).ok()
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Image::from_gif(data(), true).ok()
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Image::from_webp(data(), true).ok()
    } else {
        None
    }
}

fn to_rgb(color: tiny_skia::Color) -> rgb::Color {
    let color = color.to_color_u8();
    rgb::Color::new(color.red(), color.green(), color.blue())
}

fn convert_transform(transform: tiny_skia::Transform) -> Transform {
    Transform::from_row(
        transform.sx,
        transform.ky,
        transform.kx,
        transform.sy,
        transform.tx,
        transform.ty,
    )
}

fn convert_path(path: &tiny_skia::Path) -> Option<Path> {
    let mut builder = PathBuilder::new();
    for segment in path.segments() {
        match segment {
            tiny_skia::PathSegment::MoveTo(point) => builder.move_to(point.x, point.y),
            tiny_skia::PathSegment::LineTo(point) => builder.line_to(point.x, point.y),
            tiny_skia::PathSegment::QuadTo(control, point) => {
                builder.quad_to(control.x, control.y, point.x, point.y)
            }
            tiny_skia::PathSegment::CubicTo(first, second, point) => {
                builder.cubic_to(first.x, first.y, second.x, second.y, point.x, point.y)
            }
            tiny_skia::PathSegment::Close => builder.close(),
        }
    }
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_and_bitmap_only_licenses_are_not_embedded() {
        assert!(embeddable(0x0000), "installable");
        assert!(embeddable(0x0004), "preview & print");
        assert!(embeddable(0x0008), "editable");
        assert!(!embeddable(0x0002), "restricted");
        assert!(!embeddable(0x0202), "restricted, bitmap only");
        assert!(!embeddable(0x0208), "bitmap only");
    }

    fn glyph(cluster: u32) -> docx_layout::display_list::PlacedGlyph {
        docx_layout::display_list::PlacedGlyph {
            id: 1,
            x: 0.0,
            y: 0.0,
            cluster,
            advance: 1.0,
            logical_order: None,
            bidi_level: None,
        }
    }

    fn run(text: &str, clusters: &[u32]) -> GlyphRunPrimitive {
        serde_json::from_value(serde_json::json!({
            "fontId": 0,
            "size": 16.0,
            "color": "#000000",
            "text": text,
            "glyphs": clusters.iter().map(|cluster| serde_json::to_value(glyph(*cluster)).unwrap()).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    #[test]
    fn clusters_cover_their_characters() {
        // "ffi" as one ligature glyph, then "x".
        assert_eq!(cluster_ranges(&run("ffix", &[0, 3])), vec![0..3, 3..4]);
        // A cluster of two glyphs (a base and its mark) maps both to the cluster's text.
        assert_eq!(
            cluster_ranges(&run("é!", &[0, 0, 2])),
            vec![0..2, 0..2, 2..3]
        );
        // Right to left: clusters descend along the glyphs.
        assert_eq!(
            cluster_ranges(&run("abc", &[2, 1, 0])),
            vec![2..3, 1..2, 0..1]
        );
        // Inside a character, or past the text: no text.
        assert_eq!(cluster_ranges(&run("中", &[1, 9])), vec![0..0, 0..0]);
    }

    #[test]
    fn link_pieces_on_one_line_join() {
        let link = |x: f32, y: f32, w: f32, goal: &str| Link {
            rect: (x, y, w, 10.0),
            target: LinkGoal::Uri(goal.into()),
        };
        let merged = merge_links(vec![
            link(0.0, 0.0, 10.0, "a"),
            link(10.0, 0.0, 5.0, "a"),
            link(0.0, 20.0, 5.0, "a"),
            link(5.0, 20.0, 5.0, "b"),
        ]);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].rect, (0.0, 0.0, 15.0, 10.0));
    }

    #[test]
    fn only_safe_schemes_and_bookmarks_link() {
        let goal = |href: &str| {
            link_goal(&DocAttrs {
                href: Some(href.into()),
                ..DocAttrs::default()
            })
        };
        assert_eq!(
            goal("https://example.com"),
            Some(LinkGoal::Uri("https://example.com".into()))
        );
        assert_eq!(
            goal("mailto:a@b.c"),
            Some(LinkGoal::Uri("mailto:a@b.c".into()))
        );
        assert_eq!(goal("#_Toc1"), Some(LinkGoal::Bookmark("_Toc1".into())));
        assert_eq!(goal("javascript:alert(1)"), None);
        assert_eq!(goal("file:///etc/passwd"), None);
        assert_eq!(goal("relative/path.docx"), None);
    }

    #[test]
    fn headings_nest_by_level() {
        let heading = |id: &str, level: u8| Heading {
            para_id: id.into(),
            level,
            title: id.into(),
        };
        let headings = [
            heading("a", 0),
            heading("b", 1),
            heading("c", 2),
            heading("d", 1),
            heading("e", 0),
            heading("f", 2),
        ];
        let places: Places = headings
            .iter()
            .enumerate()
            .map(|(index, heading)| (heading.para_id.clone(), (0, 0.0, index as f32)))
            .collect();
        assert!(outline(&headings, &HashMap::new(), &places).is_some());
        assert!(
            outline(&headings, &HashMap::new(), &Places::new()).is_none(),
            "unplaced headings make no outline"
        );
    }
}
