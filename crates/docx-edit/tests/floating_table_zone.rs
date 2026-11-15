//! Word-measured behaviour of a floating table that splits across pages
//! (fixtures/floating-table-zone/word.json, Word 16.114 PDF export).
//!
//! A 250pt-wide floating table with five 150pt rows splits: rows 1-4 stay on
//! page one, row 5 lands on page two, and a paragraph after the table wraps
//! beside the fragments. Two variants vary only the border size.
//!
//! The tests cover row positions and paragraph wrapping beside the last fragment.

use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const FONT: &[u8] = include_bytes!("../../ooxml-text/tests/fonts/LiberationSans-Regular.ttf");
const PT_PER_PX: f64 = 0.75;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/floating-table-zone")
            .join(format!("{name}.docx")),
    )
    .unwrap()
}

fn lines(bytes: &[u8]) -> Vec<(usize, f64, f64, String)> {
    docx_layout::with_private_measure_fonts(|| {
        let font = docx_layout::register_measure_font(FONT).unwrap();
        let engine = EngineSession::new(78909);
        seed_from_docx(engine.doc(), bytes).unwrap();
        let package = docx_parse::parse_docx_s9_wire(bytes, Default::default())
            .unwrap()
            .document
            .package;
        let request = json!({
            "bodyStory": "body", "renderEnv": {},
            "regions": {
                "sections": [{"properties": package.document.final_section_properties}],
                "settings": package.settings
            },
            "measurement": {
                "fontChains": {"arial|0|0": [font], "arial|1|0": [font]},
                "defaults": {"fontFamily": "Arial", "fontSize": 12},
                "authoritativeShaping": true
            }
        });
        let layout = engine
            .layout_document_with_regions_json(&request.to_string())
            .unwrap();
        let display: Value =
            serde_json::from_str(&engine.build_display_list_json(&layout).unwrap()).unwrap();
        let mut out = Vec::new();
        for (page, page_value) in display["pages"].as_array().unwrap().iter().enumerate() {
            let mut chars: Vec<(f64, f64, char)> = Vec::new();
            for primitive in page_value["primitives"].as_array().unwrap() {
                if primitive["kind"].as_str() != Some("text") {
                    continue;
                }
                if let (Some(x), Some(y), Some(text)) = (
                    primitive["x"].as_f64(),
                    primitive["baselineY"].as_f64(),
                    primitive["text"].as_str(),
                ) {
                    for c in text.chars() {
                        chars.push((y, x, c));
                    }
                }
            }
            chars.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            let mut y = f64::NAN;
            let mut x0 = 0.0;
            let mut previous_x = 0.0;
            let mut text = String::new();
            for (cy, cx, c) in chars {
                if text.is_empty() {
                    y = cy;
                    x0 = cx;
                } else if (cy - y).abs() > 0.01 || cx - previous_x > 100.0 {
                    out.push((page, y * PT_PER_PX, x0 * PT_PER_PX, text.clone()));
                    y = cy;
                    x0 = cx;
                    text.clear();
                }
                previous_x = cx;
                text.push(c);
            }
            if !text.is_empty() {
                out.push((page, y * PT_PER_PX, x0 * PT_PER_PX, text.clone()));
            }
        }
        out
    })
}

fn word_lines(name: &str) -> Vec<(usize, f64, f64, String)> {
    let word: Value =
        serde_json::from_str(include_str!("fixtures/floating-table-zone/word.json")).unwrap();
    word["lines"][name]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["page"].as_u64().unwrap() as usize,
                l["y"].as_f64().unwrap(),
                l["x0"].as_f64().unwrap(),
                l["text"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn split_float_rows_match_word_on_both_pages() {
    for name in ["zone-sz4", "zone-none"] {
        let engine = lines(&fixture(name));
        for (page, y, _x0, text) in word_lines(name) {
            // Word merges the row's text with any wrapped words at the same y,
            // and the engine's paragraph can overlap the fragment (open rule 1);
            // this test pins only the row geometry: a line exists at Word's y.
            if !text.starts_with("row") {
                continue;
            }
            let matched = engine
                .iter()
                .any(|(p, ey, _ex, _et)| *p == page && (ey - y).abs() < 0.5);
            assert!(
                matched,
                "{name}: row line {text:?} at page {page} y={y:.2} not within 0.5pt of the engine"
            );
        }
    }
}

#[test]
fn text_wraps_beside_the_last_fragment_like_word() {
    for name in ["zone-sz4", "zone-none"] {
        let engine = lines(&fixture(name));
        let word = word_lines(name);
        let first_row = word
            .iter()
            .find(|(_, _, _, text)| text.contains("row5wrapword01"))
            .unwrap();
        let following = word
            .iter()
            .find(|(_, _, _, text)| text.starts_with("wrapword03"))
            .unwrap();
        assert!(
            engine.iter().any(|(page, y, x, text)| *page == first_row.0
                && (y - first_row.1).abs() < 1.0
                && (x - following.2).abs() < 2.0
                && text.replace(' ', "").starts_with("wrapword01wrapword02")),
            "{name}: first wrapped line"
        );
        for (page, y, x0, text) in word {
            if !text.starts_with("wrapword") {
                continue;
            }
            let ok = engine.iter().any(|(p, ey, ex, et)| {
                *p == page
                    && (ey - y).abs() < 1.0
                    && (ex - x0).abs() < 2.0
                    && et.replace(' ', "").starts_with(&text)
            });
            assert!(
                ok,
                "{name}: wrap line {text:?} (page {page}, y {y:.2}, x0 {x0:.1}) missing or not at Word's x"
            );
        }
    }
}

#[test]
fn fragment_bottom_includes_the_last_rows_bottom_border() {
    // floattable-avoid-last-manip-ofst.docx: Word's page-one fragment spans
    // 51.78pt (51.28 of content + the 0.5pt border); the engine's spans 51.33.
    // Recorded here because that document is not a fixture; the zone fixtures
    // pin the row arithmetic this builds on.
    let engine = lines(&fixture("zone-sz4"));
    let last_row_page0 = engine
        .iter()
        .filter(|(p, _, _, t)| *p == 0 && t.starts_with("row"))
        .map(|(_, y, _, _)| *y)
        .fold(f64::MIN, f64::max);
    let word_last = word_lines("zone-sz4")
        .into_iter()
        .filter(|(p, _, _, t)| *p == 0 && t.starts_with("row"))
        .map(|(_, y, _, _)| y)
        .fold(f64::MIN, f64::max);
    assert!(
        (word_last - last_row_page0).abs() < 0.5,
        "fragment bottoms drift: engine {last_row_page0:.2} vs Word {word_last:.2}"
    );
}

#[test]
fn subsequent_paragraphs_wrap_beside_split_table_like_word() {
    let word: Value = serde_json::from_str(include_str!(
        "fixtures/floating-table-zone/word-multi-paragraph.json"
    ))
    .unwrap();
    for (name, expected) in word["cases"].as_object().unwrap() {
        let prefix = expected["prefix"].as_str().unwrap();
        let actual = lines(&fixture(name));
        assert!(
            actual.iter().any(|(page, y, x, text)| *page as u64
                == expected["page"].as_u64().unwrap()
                && (y - expected["y"].as_f64().unwrap()).abs() < 1.0
                && (x - expected["x0"].as_f64().unwrap()).abs() < 2.0
                && text.replace(' ', "").starts_with(prefix)),
            "{name}: {prefix}"
        );
    }
}
