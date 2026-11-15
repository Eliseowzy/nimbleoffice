//! The display list as drawing operations, for a vector backend (PDF) to draw the geometry
//! this raster backend draws. The painters for rules, borders, decorations and shapes issue
//! their fills and strokes to a [`Canvas`]: the raster backend's paints a pixmap, a
//! [`Recorder`] keeps them as [`VectorOp`]s. What only a pixmap can draw (text, images,
//! shadows, gradients) a vector backend takes as a [`RasterPatch`] from
//! [`rasterize_primitives`], or draws natively from [`image_placement`] and the glyph runs.

use std::borrow::Cow;
use std::collections::BTreeMap;

use base64::Engine as _;
use docx_layout::display_list::{
    DisplayPage, GlyphRunPrimitive, ImagePrimitive, PageBorderZOrder, Primitive,
};
use serde_json::Value;
use tiny_skia::{
    Color, FillRule, IntRect, Mask, Paint, Path, Pixmap, PixmapPaint, Rect, Shader, Stroke,
    Transform,
};

use crate::{
    FRect, GlyphCache, ImageCache, ImageScope, MAX_DATA_URL_BYTES, PageBudget, RenderResources,
    Renderer, build_shape_path, clip_rect, image_frame, image_source, image_transform, number_f32,
    paint_decoration, paint_image_border, paint_image_revision, paint_line_primitive,
    paint_page_border, paint_rect, painted_rect, primitive_opacity, scaled_page_dimension,
    shape_fill, shape_shadow, shape_stroke, shape_transform, validate_page_surface,
    validate_visual_attrs,
};

/// Where the painters send their fills and strokes.
pub trait Canvas {
    fn fill_rect(&mut self, rect: Rect, paint: &Paint<'_>, transform: Transform);
    fn fill_path(&mut self, path: &Path, paint: &Paint<'_>, rule: FillRule, transform: Transform);
    fn stroke_path(
        &mut self,
        path: &Path,
        paint: &Paint<'_>,
        stroke: &Stroke,
        transform: Transform,
    );
}

/// A pixmap under an optional clip mask: the raster backend's canvas.
pub(crate) struct Masked<'a, 'm> {
    pixmap: &'a mut Pixmap,
    mask: Option<&'m Mask>,
}

impl<'a, 'm> Masked<'a, 'm> {
    pub(crate) fn new(pixmap: &'a mut Pixmap, mask: Option<&'m Mask>) -> Self {
        Self { pixmap, mask }
    }
}

impl Canvas for Masked<'_, '_> {
    fn fill_rect(&mut self, rect: Rect, paint: &Paint<'_>, transform: Transform) {
        self.pixmap.fill_rect(rect, paint, transform, self.mask);
    }

    fn fill_path(&mut self, path: &Path, paint: &Paint<'_>, rule: FillRule, transform: Transform) {
        self.pixmap
            .fill_path(path, paint, rule, transform, self.mask);
    }

    fn stroke_path(
        &mut self,
        path: &Path,
        paint: &Paint<'_>,
        stroke: &Stroke,
        transform: Transform,
    ) {
        self.pixmap
            .stroke_path(path, paint, stroke, transform, self.mask);
    }
}

/// One fill or stroke in page pixels (CSS px, 96 per inch), its color carrying the
/// primitive's opacity. A stroke is never dashed: its path holds the dashes.
#[derive(Clone, Debug)]
pub enum VectorOp {
    Fill {
        path: Path,
        color: Color,
        rule: FillRule,
        transform: Transform,
    },
    Stroke {
        path: Path,
        color: Color,
        stroke: Stroke,
        transform: Transform,
    },
}

/// A canvas keeping what is drawn on it. A paint other than a solid color (a gradient)
/// leaves it [`Recorder::unsupported`].
#[derive(Default)]
pub struct Recorder {
    pub ops: Vec<VectorOp>,
    pub unsupported: bool,
}

impl Recorder {
    fn color(&mut self, paint: &Paint<'_>) -> Option<Color> {
        match paint.shader {
            Shader::SolidColor(color) => Some(color),
            _ => {
                self.unsupported = true;
                None
            }
        }
    }
}

impl Canvas for Recorder {
    fn fill_rect(&mut self, rect: Rect, paint: &Paint<'_>, transform: Transform) {
        self.fill_path(
            &tiny_skia::PathBuilder::from_rect(rect),
            paint,
            FillRule::Winding,
            transform,
        );
    }

    fn fill_path(&mut self, path: &Path, paint: &Paint<'_>, rule: FillRule, transform: Transform) {
        if let Some(color) = self.color(paint) {
            self.ops.push(VectorOp::Fill {
                path: path.clone(),
                color,
                rule,
                transform,
            });
        }
    }

    fn stroke_path(
        &mut self,
        path: &Path,
        paint: &Paint<'_>,
        stroke: &Stroke,
        transform: Transform,
    ) {
        let Some(color) = self.color(paint) else {
            return;
        };
        // A dash pattern is opaque once in a `Stroke`: record the dashes as the path.
        let mut stroke = stroke.clone();
        let path = match stroke.dash.take() {
            Some(dash) => match path.dash(&dash, 1.0) {
                Some(dashed) => dashed,
                None => return,
            },
            None => path.clone(),
        };
        self.ops.push(VectorOp::Stroke {
            path,
            color,
            stroke,
            transform,
        });
    }
}

/// A budget no single primitive's vector form comes near; it bounds a hostile dash or
/// wave length the way a page render does.
fn vector_budget() -> PageBudget {
    PageBudget::new(crate::MAX_PAGE_DIM, crate::MAX_PAGE_DIM)
}

/// The fills and strokes `primitive` draws, without its clip (see [`primitive_clip`]), or
/// `None` when only a pixmap draws it as the raster backend does: text, glyph runs and
/// images, a shape with a shadow or a gradient. A picture-filled shape draws nothing.
pub fn record_primitive(primitive: &Primitive) -> Result<Option<Vec<VectorOp>>, String> {
    validate_visual_attrs(primitive, crate::primitive_attrs(primitive))?;
    let opacity = primitive_opacity(primitive)?;
    let mut recorder = Recorder::default();
    let mut budget = vector_budget();
    let identity = Transform::identity();
    match primitive {
        Primitive::Rect(rect) => paint_rect(&mut recorder, rect, identity, opacity)?,
        Primitive::Line(line) => {
            paint_line_primitive(&mut recorder, line, identity, opacity, &mut budget)?
        }
        Primitive::Decoration(decoration) => {
            paint_decoration(&mut recorder, decoration, identity, opacity, &mut budget)?
        }
        Primitive::Shape(shape) => {
            if shape
                .attrs
                .fill_paint
                .as_ref()
                .and_then(|paint| paint.get("kind"))
                .and_then(Value::as_str)
                == Some("picture")
            {
                return Ok(Some(Vec::new()));
            }
            if shape_shadow(&shape.attrs.effects)?.0.is_some() {
                return Ok(None);
            }
            let path = build_shape_path(&shape.geometry_path)?;
            let transform = shape_transform(shape)?;
            if let Some(paint) = shape_fill(shape, opacity)? {
                recorder.fill_path(&path, &paint, FillRule::Winding, transform);
            }
            if let Some(stroked) = shape_stroke(shape, opacity)? {
                let (line, fills, open) = stroked.pieces(&path);
                recorder.stroke_path(&line, &stroked.paint, &stroked.stroke, transform);
                for fill in &fills {
                    recorder.fill_path(fill, &stroked.paint, FillRule::Winding, transform);
                }
                let open_stroke = stroked.open_end_stroke();
                for open in &open {
                    recorder.stroke_path(open, &stroked.paint, &open_stroke, transform);
                }
            }
        }
        Primitive::Text(_) | Primitive::GlyphRun(_) | Primitive::Image(_) => return Ok(None),
    }
    Ok((!recorder.unsupported).then_some(recorder.ops))
}

/// The page borders drawn beneath everything (`back`) or above it.
pub fn record_page_borders(page: &DisplayPage, back: bool) -> Result<Vec<VectorOp>, String> {
    let mut recorder = Recorder::default();
    let mut budget = vector_budget();
    for border in page
        .page_borders
        .iter()
        .filter(|border| (border.z_order == Some(PageBorderZOrder::Back)) == back)
    {
        paint_page_border(&mut recorder, border, &mut budget, Transform::identity())?;
    }
    Ok(recorder.ops)
}

/// The rectangle a primitive's clip group clips it to, in page pixels.
pub fn primitive_clip(primitive: &Primitive) -> Result<Option<Rect>, String> {
    Ok(clip_rect(crate::primitive_attrs(primitive))?
        .and_then(|clip| Rect::from_xywh(clip.x, clip.y, clip.w, clip.h)))
}

/// What a vector backend needs to draw an image the way [`crate::render_page`] does: the
/// encoded bytes, the frame they fill, the frame's rotation and flips, the crop as
/// fractions of the source, and the opacity. The border and tracked-change marks the
/// raster backend draws over it are in [`ImagePlacement::overlay`].
pub struct ImagePlacement<'a> {
    pub bytes: Cow<'a, [u8]>,
    pub frame: Rect,
    pub transform: Transform,
    /// Left, top, right, bottom, each a fraction of the source cut away.
    pub crop: [f32; 4],
    pub opacity: f32,
    pub overlay: Vec<VectorOp>,
}

/// The image `primitive` draws, or `None` when it is not an image or resolves to no bytes
/// (the raster backend skips those) or to an empty frame.
pub fn image_placement<'a>(
    primitive: &'a Primitive,
    scope: ImageScope<'_>,
    resources: &RenderResources<'a>,
) -> Result<Option<ImagePlacement<'a>>, String> {
    let Primitive::Image(image) = primitive else {
        return Ok(None);
    };
    validate_visual_attrs(primitive, &image.attrs)?;
    let opacity = primitive_opacity(primitive)?;
    let Some(source) = image_source(&image.rel_id, scope, resources) else {
        return Ok(None);
    };
    let bytes = match source {
        crate::ImageSource::Registered(bytes) => Cow::Borrowed(bytes),
        crate::ImageSource::Data(payload) => {
            if payload.len() as u64 > MAX_DATA_URL_BYTES.saturating_mul(4) / 3 + 4 {
                return Ok(None);
            }
            match base64::engine::general_purpose::STANDARD.decode(payload) {
                Ok(bytes) => Cow::Owned(bytes),
                Err(_) => return Ok(None),
            }
        }
    };
    let frame = image_frame(image)?;
    if frame.w == 0.0 || frame.h == 0.0 {
        return Ok(None);
    }
    let transform = image_transform(image, frame)?;
    let crop = image_crop(image)?;
    let overlay = image_overlay(image, frame, transform, opacity)?;
    Ok(Some(ImagePlacement {
        bytes,
        frame: Rect::from_xywh(frame.x, frame.y, frame.w, frame.h)
            .ok_or_else(|| "invalid image frame".to_string())?,
        transform,
        crop,
        opacity,
        overlay,
    }))
}

fn image_crop(image: &ImagePrimitive) -> Result<[f32; 4], String> {
    let Some(crop) = &image.crop else {
        return Ok([0.0; 4]);
    };
    Ok([
        number_f32(&crop.left)?,
        number_f32(&crop.top)?,
        number_f32(&crop.right)?,
        number_f32(&crop.bottom)?,
    ])
}

fn image_overlay(
    image: &ImagePrimitive,
    frame: FRect,
    transform: Transform,
    opacity: f32,
) -> Result<Vec<VectorOp>, String> {
    let mut recorder = Recorder::default();
    let mut budget = vector_budget();
    paint_image_border(&mut recorder, image, frame, transform, opacity, &mut budget)?;
    paint_image_revision(&mut recorder, image, frame, transform, opacity, &mut budget)?;
    Ok(recorder.ops)
}

/// Pixels covering what some primitives of one page paint, transparent elsewhere.
pub struct RasterPatch {
    /// The patch's place on the page, in page pixels.
    pub rect: Rect,
    /// Premultiplied RGBA at the scale it was drawn at.
    pub pixmap: Pixmap,
}

/// Draws `primitives` of `page`, in order, the way [`crate::render_page`] draws them,
/// at `scale` device pixels per page pixel onto a transparent surface, and returns the
/// part they painted (`None` when they painted nothing) with the effects left undrawn.
pub fn rasterize_primitives<'k>(
    page: &DisplayPage,
    primitives: &[(&'k Primitive, ImageScope<'k>)],
    resources: &RenderResources<'_>,
    glyphs: &'k mut GlyphCache,
    images: &'k mut ImageCache,
    scale: f32,
) -> Result<(Option<RasterPatch>, BTreeMap<String, usize>), String> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err("scale must be finite and positive".to_owned());
    }
    let width = scaled_page_dimension(&page.width, "page width", scale)?;
    let height = scaled_page_dimension(&page.height, "page height", scale)?;
    validate_page_surface(width, height)?;
    let mut pixmap = Pixmap::new(width, height).ok_or_else(|| "invalid pixmap size".to_string())?;
    let mut renderer = Renderer::new(width, height, glyphs, images);
    renderer.scale = scale;
    for (primitive, scope) in primitives {
        renderer.paint_primitive(&mut pixmap, primitive, resources, *scope)?;
    }
    let skipped = std::mem::take(&mut renderer.scratch.skipped);
    let Some(painted) = painted_rect(&pixmap, &mut Vec::new()) else {
        return Ok((None, skipped));
    };
    let patch = crop(&pixmap, painted)?;
    let rect = Rect::from_xywh(
        painted.x() as f32 / scale,
        painted.y() as f32 / scale,
        painted.width() as f32 / scale,
        painted.height() as f32 / scale,
    )
    .ok_or_else(|| "invalid raster patch".to_string())?;
    Ok((
        Some(RasterPatch {
            rect,
            pixmap: patch,
        }),
        skipped,
    ))
}

fn crop(pixmap: &Pixmap, rect: IntRect) -> Result<Pixmap, String> {
    let mut out =
        Pixmap::new(rect.width(), rect.height()).ok_or_else(|| "invalid patch".to_string())?;
    out.draw_pixmap(
        -rect.x(),
        -rect.y(),
        pixmap.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Ok(out)
}

/// The rotation and horizontal scale a glyph run is drawn under, about its own box: the
/// placed glyphs' positions are before it.
pub fn glyph_run_transform(run: &GlyphRunPrimitive) -> Result<Transform, String> {
    crate::primitive_visual_transform(
        crate::font::glyph_rect(run)?,
        run.rotation_deg.as_ref(),
        run.horizontal_scale.as_ref(),
    )
}

/// Whether the raster backend draws a shadow under this glyph run, which a vector backend
/// leaves to it.
pub fn glyph_run_has_drawn_shadow(run: &GlyphRunPrimitive) -> bool {
    matches!(
        crate::shadow::text_shadow(run.text_shadow.as_deref(), run.size as f32),
        crate::shadow::TextShadow::Drawn(_)
    )
}

/// The opacity the raster backend draws `primitive` at: its own, its flattened group's and
/// its clip group's, a hidden run's 0.4 included.
pub fn primitive_paint_opacity(primitive: &Primitive) -> Result<f32, String> {
    primitive_opacity(primitive)
}

/// One glyph the raster backend would paint for a text run, in page pixels: its pen origin
/// at `x`, its baseline at `y`, under `transform`.
#[derive(Clone, Debug)]
pub struct RecordedGlyph {
    pub font: ooxml_text::FontId,
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
    pub size: f32,
    /// Opacity included.
    pub color: Color,
    pub transform: Transform,
    /// The characters of the cluster this glyph starts; empty for a cluster's later glyphs.
    pub text: String,
}

/// The glyphs a text run (one whose font the layout did not resolve to a glyph run) paints,
/// shaped and placed as the raster backend does; `None` for a run with a drawn shadow,
/// which only a pixmap draws, and for anything that is not a text run. The run's
/// `paint_clip` is not applied.
pub fn record_text(
    primitive: &Primitive,
    resources: &RenderResources<'_>,
) -> Result<Option<Vec<RecordedGlyph>>, String> {
    let Primitive::Text(run) = primitive else {
        return Ok(None);
    };
    if run.text_shadow.is_some() {
        return Ok(None);
    }
    validate_visual_attrs(primitive, &run.attrs)?;
    let mut glyphs = Vec::new();
    let mut pixmap = Pixmap::new(1, 1).ok_or_else(|| "invalid pixmap".to_string())?;
    let mut cache = GlyphCache::default();
    let mut specs = crate::font::FontSpecCache::default();
    let mut budget = vector_budget();
    let mut skipped = BTreeMap::new();
    let mut context = crate::font::PaintContext {
        pixmap: &mut pixmap,
        resources,
        cache: &mut cache,
        specs: &mut specs,
        budget: &mut budget,
        base_transform: Transform::identity(),
        mask: None,
        retained_mask_bytes: 0,
        opacity: primitive_opacity(primitive)?,
        skipped: &mut skipped,
        sink: Some(&mut glyphs),
    };
    let unclipped = docx_layout::display_list::TextRunPrimitive {
        paint_clip: None,
        ..run.clone()
    };
    crate::font::paint_text(&mut context, &unclipped)?;
    Ok(Some(glyphs))
}
