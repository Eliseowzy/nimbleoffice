//! Server-side rasterization of a display list.

use std::sync::{Mutex, PoisonError};

use docx_layout::display_list::DisplayList;
use docx_raster::{
    ImageCache, ImageMap, ImageScope, RenderResources, RenderedPage, scoped_image_key,
};
use serde_json::Number;

use crate::{Document, Error, Result};

const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGES: usize = 256;
pub const MAX_PIXMAP_DIM: u32 = docx_raster::MAX_PAGE_DIM;
pub const MAX_PIXMAP_PIXELS: u64 = docx_raster::MAX_PAGE_PIXELS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngTarget {
    pub page: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    pub scale: f32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

/// A layout of the document as saved, with what an export needs beside the display list.
pub(crate) struct LaidOut {
    pub display_list: DisplayList,
    /// Body headings in document order: paragraph id and zero-based outline level.
    #[cfg_attr(not(feature = "pdf"), allow(dead_code))]
    pub headings: Vec<(String, u8)>,
    /// Body bookmarks by name: the paragraph each starts in.
    #[cfg_attr(not(feature = "pdf"), allow(dead_code))]
    pub bookmarks: std::collections::HashMap<String, String>,
}

/// Caller-supplied bytes for relationship ids the display list did not already
/// carry as `data:` URLs, keyed by owning part, plus the decoded images a page
/// leaves for the next — behind a `Mutex` so the document stays `Send + Sync`.
#[derive(Default)]
pub(crate) struct ImageRegistry {
    pub(crate) entries: ImageMap,
    pub(crate) cache: Mutex<ImageCache>,
}

impl ImageRegistry {
    fn register(&mut self, scope: ImageScope<'_>, rel_id: &str, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(Error::ResourceLimit(format!(
                "image exceeds {MAX_IMAGE_BYTES} bytes"
            )));
        }
        if rel_id.is_empty() {
            return Err(Error::Image("image relationship id is empty".to_owned()));
        }
        let key = scoped_image_key(scope, rel_id);
        if self.entries.len() >= MAX_IMAGES && !self.entries.contains_key(&key) {
            return Err(Error::ResourceLimit(format!(
                "more than {MAX_IMAGES} images"
            )));
        }
        self.entries.insert(key, bytes.to_vec());
        Ok(())
    }
}

fn validate_render_size(width: u32, height: u32) -> Result<()> {
    if width > MAX_PIXMAP_DIM || height > MAX_PIXMAP_DIM {
        return Err(Error::RenderTooLarge {
            width,
            height,
            max: MAX_PIXMAP_DIM,
        });
    }
    if u64::from(width) * u64::from(height) > MAX_PIXMAP_PIXELS {
        return Err(Error::RenderAreaTooLarge {
            width,
            height,
            max_pixels: MAX_PIXMAP_PIXELS,
        });
    }
    Ok(())
}

/// The pixel extent the raster backend would allocate, or `None` when the
/// backend rejects the number before allocating anything.
fn page_pixels(number: &Number, scale: f32) -> Option<u32> {
    let value = number.as_f64()? * f64::from(scale);
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    Some(value.ceil().min(f64::from(u32::MAX)) as u32)
}

impl Document {
    pub fn render_png(&self, target: PngTarget, options: &RenderOptions) -> Result<RenderedPage> {
        if !options.scale.is_finite() || options.scale <= 0.0 {
            return Err(Error::Render("scale must be finite and positive".into()));
        }
        let laid_out = self.lay_out(false)?;
        self.render_display_list_scaled(&laid_out.display_list, target.page, options.scale)
    }

    /// Lays the document out as saved, its tracked changes accepted first when
    /// `accept_revisions` (what Word prints as "No Markup").
    pub(crate) fn lay_out(&self, accept_revisions: bool) -> Result<LaidOut> {
        let bytes = self.save()?;
        let package = docx_parse::parse_docx_s9_wire(
            &bytes,
            docx_parse::S9ParseOptions {
                detect_variables: false,
                ..Default::default()
            },
        )?
        .document
        .package;
        let engine = docx_edit::EngineSession::new(1);
        docx_edit::seed_from_docx(engine.doc(), &bytes).map_err(Error::Render)?;
        if accept_revisions {
            let context = docx_edit::EditCtx::local(String::new(), String::new());
            let mut accepted = std::collections::BTreeSet::new();
            for revision in engine
                .doc()
                .list_revisions()
                .map_err(|error| Error::Render(error.to_string()))?
            {
                let id = revision.change.revision_id.clone();
                if accepted.insert(id.clone()) {
                    engine
                        .doc()
                        .accept_change(&context, &docx_edit::ChangeTarget::Revision(id))
                        .map_err(|error| Error::Render(error.to_string()))?;
                }
            }
        }
        let package =
            serde_json::to_value(package).map_err(|error| Error::Render(error.to_string()))?;
        let mut request = docx_edit::layout_request::region_layout_request(&package, 24.0);
        request["measurement"] = serde_json::json!({
            "defaults": {"fontFamily": "Calibri", "fontSize": 11}
        });
        let mut fonts = self.fonts.lock().unwrap_or_else(PoisonError::into_inner);
        #[cfg(feature = "fonts")]
        {
            let requirements = engine
                .layout_font_requirements_json(&request.to_string())
                .map_err(Error::Render)?;
            let requirements: Vec<serde_json::Value> = serde_json::from_str(&requirements)
                .map_err(|error| Error::Render(error.to_string()))?;
            let requirements: Vec<_> = requirements
                .iter()
                .map(|value| docx_layout::measure_blocks::FontRequirement {
                    key: value["key"].as_str().unwrap_or_default().to_owned(),
                    family: value["family"].as_str().unwrap_or("Calibri").to_owned(),
                    bold: value["bold"].as_bool().unwrap_or(false),
                    italic: value["italic"].as_bool().unwrap_or(false),
                    scripts: value["scripts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|script| script.as_str().map(str::to_owned))
                        .collect(),
                    characters: Default::default(),
                })
                .collect();
            fonts.resolve(&self.font_manager, &requirements)?;
        }
        let chains: std::collections::BTreeMap<_, Vec<_>> = fonts
            .chains
            .iter()
            .map(|(key, ids)| (key.clone(), ids.iter().map(|id| id.to_u32()).collect()))
            .collect();
        request["measurement"]["fontChains"] = serde_json::json!(chains);
        let display_list = docx_layout::with_measure_font_store(&mut fonts.store, || {
            let output = engine
                .layout_document_with_regions_json(&request.to_string())
                .map_err(Error::Render)?;
            let mut output: serde_json::Value =
                serde_json::from_str(&output).map_err(|error| Error::Render(error.to_string()))?;
            output["fontChains"] = serde_json::json!(chains);
            docx_layout::build_display_list_value(&output.to_string()).map_err(Error::Render)
        })?;
        drop(fonts);
        let headings = engine
            .doc()
            .paragraph_headings("body")
            .map(|headings| {
                headings
                    .into_iter()
                    .map(|(para_id, heading)| (para_id.to_string(), heading.outline_level))
                    .collect()
            })
            .unwrap_or_default();
        let mut bookmarks = std::collections::HashMap::new();
        for paragraph in engine.doc().paragraphs("body").unwrap_or_default() {
            let Some(yrs::Any::Array(entries)) = paragraph.properties.get("bookmarks") else {
                continue;
            };
            for entry in entries.iter() {
                let yrs::Any::Map(entry) = entry else {
                    continue;
                };
                if let (Some(yrs::Any::String(kind)), Some(yrs::Any::String(name))) =
                    (entry.get("kind"), entry.get("name"))
                    && kind.as_ref() == "start"
                {
                    bookmarks
                        .entry(name.to_string())
                        .or_insert_with(|| paragraph.para_id.to_string());
                }
            }
        }
        Ok(LaidOut {
            display_list,
            headings,
            bookmarks,
        })
    }

    /// Supplies bytes for one relationship id the display list left
    /// unresolved, scoped to the part that owns the id. Images are capped at
    /// 256 and each at 32 MiB.
    pub fn register_image(
        &mut self,
        scope: ImageScope<'_>,
        rel_id: &str,
        bytes: &[u8],
    ) -> Result<()> {
        self.images.register(scope, rel_id, bytes)
    }

    /// Rasterizes one display-list page to deterministic PNG bytes. A page past
    /// [`MAX_PIXMAP_DIM`] or [`MAX_PIXMAP_PIXELS`] is refused before any surface
    /// is allocated; an image the backend cannot draw is skipped and counted.
    /// Glyph outlines and decoded images are cached on the document, so a page
    /// reuses what earlier pages extracted and decoded; an export past either
    /// cache's cap re-does what it evicted.
    pub fn render_display_list_png(
        &self,
        display_list: &DisplayList,
        page_ordinal: usize,
    ) -> Result<RenderedPage> {
        self.render_display_list_scaled(display_list, page_ordinal, 1.0)
    }

    fn render_display_list_scaled(
        &self,
        display_list: &DisplayList,
        page_ordinal: usize,
        scale: f32,
    ) -> Result<RenderedPage> {
        if let Some(page) = display_list.pages.get(page_ordinal)
            && let (Some(width), Some(height)) = (
                page_pixels(&page.width, scale),
                page_pixels(&page.height, scale),
            )
        {
            validate_render_size(width, height)?;
        }
        #[cfg(feature = "fonts")]
        self.prepare_display_fonts(display_list, page_ordinal)?;
        let mut fonts = self.fonts.lock().unwrap_or_else(PoisonError::into_inner);
        let mut images = self
            .images
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let crate::font::FontRegistry {
            store,
            chains,
            glyphs,
            ..
        } = &mut *fonts;
        let resources = RenderResources::new(store, chains, &self.images.entries);
        docx_raster::render_page_cached_scaled(
            display_list,
            page_ordinal,
            &resources,
            glyphs,
            &mut images,
            scale,
        )
        .map_err(Error::Render)
    }
}
