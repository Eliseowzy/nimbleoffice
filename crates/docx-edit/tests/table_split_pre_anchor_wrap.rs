//! Text laid out before a legacy (compatibilityMode < 15) page-anchored float
//! on the same page wraps beside it, and the float's first fragment shares the
//! page's body budget with that text (fixtures/table-split-bands, Word 16.114
//! PDF exports of the continuation and rows tables after N short paragraphs).

use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const FONT: &[u8] = include_bytes!("../../ooxml-text/tests/fonts/LiberationSans-Regular.ttf");
const PT_PER_PX: f64 = 0.75;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/table-split-bands")
            .join(format!("{name}.docx")),
    )
    .unwrap()
}

fn display(bytes: &[u8]) -> Value {
    docx_layout::with_private_measure_fonts(|| {
        let font = docx_layout::register_measure_font(FONT).unwrap();
        let engine = EngineSession::new(78911);
        seed_from_docx(engine.doc(), bytes).unwrap();
        let package = docx_parse::parse_docx_s9_wire(bytes, Default::default())
            .unwrap()
            .document
            .package;
        let chain = json!([font]);
        let request = json!({
            "bodyStory": "body", "renderEnv": {},
            "regions": {
                "sections": [{"properties": package.document.final_section_properties}],
                "settings": package.settings
            },
            "measurement": {
                "fontChains": {
                    "arial|0|0": chain, "calibri|0|0": chain, "calibri light|0|0": chain,
                    "cambria|0|0": chain, "times new roman|0|0": chain
                },
                "defaults": {"fontFamily": "Arial", "fontSize": 11},
                "authoritativeShaping": true
            }
        });
        let layout = engine
            .layout_document_with_regions_json(&request.to_string())
            .unwrap();
        serde_json::from_str(&engine.build_display_list_json(&layout).unwrap()).unwrap()
    })
}

fn walk<'a>(primitive: &'a Value, out: &mut Vec<&'a Value>) {
    out.push(primitive);
    for child in primitive["children"].as_array().into_iter().flatten() {
        walk(child, out);
    }
}

fn primitives(page: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    for primitive in page["primitives"].as_array().into_iter().flatten() {
        walk(primitive, &mut out);
    }
    out
}

/// (page, baseline pt, x pt) of the painted line starting with `text`.
fn line_at(display: &Value, text: &str) -> (usize, f64, f64) {
    for (index, page) in display["pages"].as_array().unwrap().iter().enumerate() {
        let mut chars: Vec<(f64, f64, &str)> = primitives(page)
            .into_iter()
            .filter_map(|p| {
                Some((
                    p["baselineY"].as_f64()?,
                    p["x"].as_f64()?,
                    p["text"].as_str()?,
                ))
            })
            .collect();
        chars.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut i = 0;
        while i < chars.len() {
            let (y, x, _) = chars[i];
            let mut joined = String::new();
            let mut j = i;
            while j < chars.len() && (chars[j].0 - y).abs() < 0.01 && chars[j].1 >= x {
                joined.push_str(chars[j].2);
                j += 1;
            }
            if joined.starts_with(text) {
                return (index, y * PT_PER_PX, x * PT_PER_PX);
            }
            // Next glyph run on this baseline (another column) or the next line.
            i += chars[i..j]
                .windows(2)
                .position(|w| w[1].1 - w[0].1 > 40.0)
                .map_or(j - i, |gap| gap + 1);
        }
    }
    panic!("no painted line {text:?}")
}

fn assert_line(display: &Value, text: &str, page: usize, y: f64, x: Option<f64>) {
    let (p, ly, lx) = line_at(display, text);
    assert_eq!(
        p,
        page,
        "{text:?} on page {}, Word page {}",
        p + 1,
        page + 1
    );
    assert!((ly - y).abs() < 0.5, "{text:?} at {ly:.2}pt, Word {y:.2}pt");
    if let Some(x) = x {
        // The engine places the float a few points right of Word (cell
        // margin shift, a separate gap); the wrap side and column must match.
        assert!(
            (lx - x).abs() < 5.0,
            "{text:?} at x {lx:.2}pt, Word {x:.2}pt"
        );
    }
}

#[test]
fn text_before_a_legacy_page_float_wraps_beside_it() {
    let display = display(&fixture("continuation-6-before"));
    assert_line(&display, "pre 0", 0, 67.92, Some(57.6));
    // The float's top edge (85pt) reaches into pre1's line: it wraps too.
    assert_line(&display, "pre 1", 0, 82.56, Some(469.0));
    assert_line(&display, "pre 5", 0, 140.64, Some(469.0));
    // Body budget: top + body - the text before = 754.6 - 87.3pt.
    assert_line(&display, "A line 35", 0, 646.32, None);
    assert_line(&display, "A line 36", 1, 74.88, None);
}

#[test]
fn text_past_a_shortened_first_fragment_runs_full_width() {
    let display = display(&fixture("continuation-25-before"));
    assert_line(&display, "pre 22", 0, 387.84, Some(469.0));
    assert_line(&display, "pre 23", 0, 402.48, Some(57.6));
    assert_line(&display, "A line 18", 0, 374.4, None);
    assert_line(&display, "A line 19", 1, 74.88, None);
}

#[test]
fn a_legacy_page_float_first_fragment_splits_rows_mid_content() {
    let display = display(&fixture("rows-6-before"));
    assert_line(&display, "Row 16 first", 0, 649.92, None);
    assert_line(&display, "Row 16 second", 1, 74.88, None);
    assert_line(&display, "Row 17 first", 1, 95.52, None);
}
