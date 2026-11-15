//! A table row cut by a page break redraws its boundary band at both cut edges
//! (fixtures/table-split-bands, Word 16.114 PDF exports): the cut fragment's
//! lines end above the band, the resumed fragment's lines start below it, and
//! a legacy (compat < 15) page-anchored float fills its first page to
//! min(footer line, top + body height).

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

/// (page, baseline pt, text) of every painted text line starting with
/// `prefix`; glyphs on one baseline join into a line until a column gap.
fn baselines(display: &Value, prefix: &str) -> Vec<(usize, f64, String)> {
    let mut out: Vec<(usize, f64, String)> = Vec::new();
    for (index, page) in display["pages"].as_array().unwrap().iter().enumerate() {
        let mut chars: Vec<(f64, f64, f64, &str)> = primitives(page)
            .into_iter()
            .filter_map(|p| {
                Some((
                    p["baselineY"].as_f64()?,
                    p["x"].as_f64()?,
                    p["width"].as_f64().unwrap_or(0.0),
                    p["text"].as_str()?,
                ))
            })
            .collect();
        chars.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut line: Option<(f64, f64, String)> = None;
        for (y, x, width, text) in chars {
            match &mut line {
                Some((ly, end, joined)) if (*ly - y).abs() < 0.01 && x - *end < 20.0 => {
                    joined.push_str(text);
                    *end = x + width;
                }
                _ => {
                    if let Some((ly, _, joined)) = line.take() {
                        out.push((index, ly * PT_PER_PX, joined));
                    }
                    line = Some((y, x + width, text.to_string()));
                }
            }
        }
        if let Some((ly, _, joined)) = line {
            out.push((index, ly * PT_PER_PX, joined));
        }
    }
    out.retain(|(_, _, text)| text.starts_with(prefix));
    out
}

fn find(lines: &[(usize, f64, String)], text: &str) -> (usize, f64) {
    lines
        .iter()
        .find(|(_, _, t)| t.starts_with(text))
        .map(|(page, y, _)| (*page, *y))
        .unwrap_or_else(|| panic!("no painted line {text:?}"))
}

#[test]
fn legacy_page_float_continuation_matches_word() {
    let lines = baselines(&display(&fixture("continuation-compat14")), "A line");
    let (page, y) = find(&lines, "A line 41");
    assert_eq!(page, 0, "line 41 stays on page one (Word: 742.32pt)");
    assert!(
        (y - 742.32).abs() < 0.5,
        "line 41 at {y:.2}pt, Word 742.32pt"
    );
    let (page, y) = find(&lines, "A line 42");
    assert_eq!(page, 1, "line 42 opens page two");
    // The resumed row redraws its sz36 band below the top margin: 57.6 + 4.5 + 12.8.
    assert!((y - 74.88).abs() < 0.5, "line 42 at {y:.2}pt, Word 74.88pt");
}

/// Word 2013+ moves a page-anchored float that must split, positioned above
/// body text already on the page, to the next page at the same position; with
/// no text over its position it stays on page one.
#[test]
fn word2013_page_float_above_text_moves_to_next_page() {
    let lines = baselines(&display(&fixture("continuation-compat15")), "A line");
    let (page, y) = find(&lines, "A line 01");
    assert_eq!(page, 1, "the float opens page two (Word)");
    assert!(
        (y - 102.24).abs() < 0.5,
        "line 01 at {y:.2}pt, Word 102.24pt"
    );
    let (page, _) = find(&lines, "A line 39");
    assert_eq!(page, 1, "line 39 ends page two (Word 710.40pt)");
    let (page, y) = find(&lines, "A line 40");
    assert_eq!(page, 2, "line 40 opens page three");
    assert!((y - 74.88).abs() < 0.5, "line 40 at {y:.2}pt, Word 74.88pt");

    let lines = baselines(&display(&fixture("continuation-compat15-clear")), "A line");
    let (page, y) = find(&lines, "A line 01");
    assert_eq!(
        page, 0,
        "with only a short line above it the float stays on page one"
    );
    assert!(
        (y - 102.24).abs() < 0.5,
        "line 01 at {y:.2}pt, Word 102.24pt"
    );
}

#[test]
fn legacy_page_float_whole_rows_match_word() {
    let lines = baselines(&display(&fixture("rows-compat14")), "Row");
    let (page, y) = find(&lines, "Row 18 second");
    assert_eq!(page, 0, "row 18 stays on page one");
    assert!(
        (y - 738.72).abs() < 0.5,
        "row 18 second line at {y:.2}pt, Word 738.72pt"
    );
    let (page, y) = find(&lines, "Row 19 first");
    assert_eq!(page, 1, "row 19 opens page two");
    assert!((y - 74.88).abs() < 0.5, "row 19 at {y:.2}pt, Word 74.88pt");
}

/// No horizontal table border or cut line crosses a painted text line: a page
/// break redraws the cut row's band outside its lines, and a boundary band sits
/// between two rows' lines rather than over either.
#[test]
fn split_table_bands_never_cross_text() {
    for name in [
        "continuation-clipping",
        "page-bottom-boundary",
        "continuation-compat14",
        "rows-compat14",
    ] {
        let display = display(&fixture(name));
        for (index, page) in display["pages"].as_array().unwrap().iter().enumerate() {
            let all = primitives(page);
            let bands: Vec<(f64, f64)> = all
                .iter()
                .filter(|p| p["kind"] == "line")
                .filter(|p| matches!(p["role"].as_str(), Some("table-border" | "table-cut")))
                .filter(|p| (p["y1"].as_f64().unwrap() - p["y2"].as_f64().unwrap()).abs() < 1e-6)
                .map(|p| {
                    let y = p["y1"].as_f64().unwrap();
                    let half = p["strokeWidth"].as_f64().unwrap_or(1.0) / 2.0;
                    (y - half, y + half)
                })
                .collect();
            assert!(
                !bands.is_empty(),
                "{name}: page {index} paints no table border"
            );
            for text in all
                .iter()
                .filter(|p| p["kind"] == "text" && p.get("table").is_some())
            {
                let label = text["text"].as_str().unwrap_or_default();
                if label.trim().is_empty() {
                    continue;
                }
                let baseline = text["baselineY"].as_f64().unwrap();
                // 11pt Arial ink: ascent ~0.72em above, descent ~0.21em below.
                let size = 11.0 / PT_PER_PX;
                let (top, bottom) = (baseline - 0.72 * size, baseline + 0.21 * size);
                for (band_top, band_bottom) in &bands {
                    assert!(
                        *band_bottom <= top + 1e-6 || *band_top >= bottom - 1e-6,
                        "{name}: page {index} band {:.2}-{:.2}pt crosses {label:?} ({:.2}-{:.2}pt)",
                        band_top * PT_PER_PX,
                        band_bottom * PT_PER_PX,
                        top * PT_PER_PX,
                        bottom * PT_PER_PX,
                    );
                }
            }
        }
    }
}
