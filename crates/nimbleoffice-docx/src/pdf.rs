//! PDF export: the layout the raster backend draws, as a vector PDF.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::sync::PoisonError;

use docx_raster::RenderResources;

use crate::{Document, Error, Result};

/// How [`Document::render_pdf`] exports.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfExportOptions {
    /// Page ordinals (from 0) to export; every page when `None`.
    pub pages: Option<RangeInclusive<usize>>,
    /// Draw tracked changes and their marks ("All Markup"); otherwise they are accepted
    /// first, as Word prints the document with "No Markup".
    pub markup: bool,
    pub title: Option<String>,
    pub author: Option<String>,
    /// Device pixels per page pixel for what is drawn as an image (shadows, gradients).
    pub raster_scale: f32,
}

impl Default for PdfExportOptions {
    fn default() -> Self {
        Self {
            pages: None,
            markup: false,
            title: None,
            author: None,
            raster_scale: 3.0,
        }
    }
}

/// A written PDF and what it had to draw differently from the screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfExport {
    pub bytes: Vec<u8>,
    /// Pages written.
    pub pages: usize,
    /// Pages the document lays out to.
    pub total_pages: usize,
    /// Families drawn as outlines, unselectable, because their license forbids embedding.
    pub outlined_fonts: Vec<String>,
    /// Places drawn as images rather than vectors.
    pub raster_patches: usize,
    /// Primitives neither backend could draw, left out.
    pub skipped_primitives: usize,
    pub skipped_effects: BTreeMap<String, usize>,
    /// Primitives drawn as images, by why (see `docx_pdf::PdfOutput::rasterized`).
    pub rasterized: BTreeMap<String, usize>,
}

impl Document {
    /// Exports the document as a PDF: text in its own subset fonts, with links, and the
    /// headings as bookmarks.
    pub fn render_pdf(&self, options: &PdfExportOptions) -> Result<PdfExport> {
        let laid_out = self.lay_out(!options.markup)?;
        let total_pages = laid_out.display_list.pages.len();
        if let Some(range) = &options.pages
            && (range.is_empty() || *range.start() >= total_pages)
        {
            return Err(Error::Render(format!(
                "pages {}-{} are outside the document's {total_pages}",
                range.start() + 1,
                range.end() + 1
            )));
        }
        #[cfg(feature = "fonts")]
        for page in 0..total_pages {
            if options
                .pages
                .as_ref()
                .is_none_or(|range| range.contains(&page))
            {
                self.prepare_display_fonts(&laid_out.display_list, page)?;
            }
        }
        let pdf_options = docx_pdf::PdfOptions {
            pages: options.pages.clone(),
            headings: laid_out
                .headings
                .into_iter()
                .map(|(para_id, level)| docx_pdf::Heading {
                    para_id,
                    level,
                    title: String::new(),
                })
                .collect(),
            bookmarks: laid_out.bookmarks,
            title: options.title.clone(),
            author: options.author.clone(),
            raster_scale: options.raster_scale,
        };
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
        let output = docx_pdf::render_pdf(
            &laid_out.display_list,
            &resources,
            glyphs,
            &mut images,
            &pdf_options,
        )
        .map_err(Error::Render)?;
        Ok(PdfExport {
            bytes: output.bytes,
            pages: output.pages,
            total_pages,
            outlined_fonts: output.outlined_fonts,
            raster_patches: output.raster_patches,
            skipped_primitives: output.skipped_primitives,
            skipped_effects: output.skipped_effects,
            rasterized: output.rasterized,
        })
    }
}
