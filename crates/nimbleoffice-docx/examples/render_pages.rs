//! Renders every page of a .docx to PNG: `render_pages IN.docx OUT_PREFIX [SCALE]`.
use nimbleoffice_docx::{Document, PngTarget, RenderOptions};

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("IN.docx");
    let prefix = args.next().expect("OUT_PREFIX");
    let scale: f32 = args.next().map_or(1.0, |s| s.parse().expect("scale"));
    let doc = Document::open(&std::fs::read(&input).expect("read")).expect("open");
    for page in 0.. {
        match doc.render_png(PngTarget { page }, &RenderOptions { scale }) {
            Ok(png) => {
                std::fs::write(format!("{prefix}-{}.png", page + 1), png.bytes).expect("write")
            }
            Err(_) => break,
        }
    }
}
