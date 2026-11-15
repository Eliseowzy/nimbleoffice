//! Exports a .docx as a PDF, and optionally its pages as PNG for comparison.
//!
//!   cargo run -p nimbleoffice-docx --features pdf --example export_pdf -- IN.docx OUT.pdf [--markup] [--pages FROM-TO] [--png DIR]
use nimbleoffice_docx::{Document, PdfExportOptions, PngTarget, RenderOptions};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        eprintln!("usage: export_pdf IN.docx OUT.pdf [--markup] [--pages FROM-TO] [--png DIR]");
        std::process::exit(2);
    };
    let flag = |name: &str| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|at| args.get(at + 1))
    };
    let mut options = PdfExportOptions {
        markup: args.iter().any(|arg| arg == "--markup"),
        ..PdfExportOptions::default()
    };
    if let Some(pages) = flag("--pages") {
        let (from, to) = pages.split_once('-').expect("--pages FROM-TO");
        options.pages = Some(from.parse::<usize>().unwrap() - 1..=to.parse::<usize>().unwrap() - 1);
    }
    let bytes = std::fs::read(input).expect("read input");
    let document = Document::open(&bytes).expect("open");
    let started = std::time::Instant::now();
    let export = document.render_pdf(&options).expect("export");
    eprintln!(
        "{} of {} pages in {:?}; {} bytes; raster patches {}; skipped {}; outlined fonts {:?}; effects {:?}; rasterized {:?}",
        export.pages,
        export.total_pages,
        started.elapsed(),
        export.bytes.len(),
        export.raster_patches,
        export.skipped_primitives,
        export.outlined_fonts,
        export.skipped_effects,
        export.rasterized
    );
    std::fs::write(output, &export.bytes).expect("write output");
    if let Some(dir) = flag("--png") {
        std::fs::create_dir_all(dir).unwrap();
        for page in 0..export.total_pages {
            let png = document
                .render_png(PngTarget { page }, &RenderOptions { scale: 1.0 })
                .expect("png");
            std::fs::write(format!("{dir}/page-{}.png", page + 1), png.bytes).unwrap();
        }
    }
}
