//! The float flow places each paragraph where the paginator will, spacing included, so a
//! page-anchored image's wrap zone meets the lines it covers (fixtures/image-wrap-flow).

use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const FONT: &[u8] = include_bytes!("../../ooxml-text/tests/fonts/LiberationSans-Regular.ttf");
const PT_PER_PX: f64 = 0.75;

fn display(name: &str) -> Value {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/image-wrap-flow")
            .join(name),
    )
    .unwrap();
    docx_layout::with_private_measure_fonts(|| {
        let font = docx_layout::register_measure_font(FONT).unwrap();
        let engine = EngineSession::new(78913);
        seed_from_docx(engine.doc(), &bytes).unwrap();
        let package = docx_parse::parse_docx_s9_wire(&bytes, Default::default())
            .unwrap()
            .document
            .package;
        let chain = json!([font]);
        let request = json!({
            "bodyStory": "body", "renderEnv": {},
            "regions": {"sections": [{"properties": package.document.final_section_properties}],
                        "settings": package.settings},
            "measurement": {"fontChains": {"calibri|0|0": chain, "arial|0|0": chain, "times new roman|0|0": chain},
                            "defaults": {"fontFamily": "Calibri", "fontSize": 11}, "authoritativeShaping": true}
        });
        let layout = engine
            .layout_document_with_regions_json(&request.to_string())
            .unwrap();
        serde_json::from_str(&engine.build_display_list_json(&layout).unwrap()).unwrap()
    })
}

/// (baseline pt, right edge pt) of the first text line on page one, glyphs joined by baseline.
fn first_line(display: &Value) -> (f64, f64) {
    let mut glyphs: Vec<(f64, f64)> = Vec::new();
    fn walk(p: &Value, out: &mut Vec<(f64, f64)>) {
        if p["kind"] == "text" && p["text"].as_str().is_some_and(|t| !t.trim().is_empty()) {
            out.push((
                p["baselineY"].as_f64().unwrap(),
                p["x"].as_f64().unwrap() + p["width"].as_f64().unwrap_or(0.0),
            ));
        }
        for c in p["children"].as_array().into_iter().flatten() {
            walk(c, out);
        }
    }
    for p in display["pages"][0]["primitives"].as_array().unwrap() {
        walk(p, &mut glyphs);
    }
    let top = glyphs.iter().map(|g| g.0).fold(f64::INFINITY, f64::min);
    let right = glyphs
        .iter()
        .filter(|g| (g.0 - top).abs() < 0.01)
        .map(|g| g.1)
        .fold(0.0, f64::max);
    (top * PT_PER_PX, right * PT_PER_PX)
}

#[test]
fn first_line_beside_a_page_anchored_image_wraps_like_word() {
    let (baseline, right) = first_line(&display("page-anchored-square.docx"));
    assert!(
        (baseline - 254.16).abs() < 1.5,
        "first line at {baseline:.2}pt, Word 254.16pt"
    );
    // Word ends the line at 376.2pt, before the image (385.1pt less distL 9pt).
    assert!(
        right <= 376.5,
        "first line runs to {right:.2}pt, past the image's wrap edge"
    );
}
