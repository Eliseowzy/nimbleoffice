//! A floating table taller than the page splits in place; the paragraph after it
//! wraps beside the last fragment and then continues at full width, as Word does
//! (fixtures/floating-table-split/word.json, Word 16.114 PDF export).
//!
//! float3600's table is 180pt wide (wrap column at x0=261.5) and the whole
//! paragraph fits beside the fragment. float7000's table is 350pt wide (wrap
//! column at x0=431.5); the paragraph's last line is wider than the 92pt column,
//! so it leaves the wrap zone.
//!
//! A line that does not fit the wrap column moves to the next page at full width.

use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const FONT: &[u8] = include_bytes!("../../ooxml-text/tests/fonts/LiberationSans-Regular.ttf");
const PT_PER_PX: f64 = 0.75;
const LEFT_MARGIN_PT: f64 = 72.0;

struct Line {
    page: usize,
    y: f64,
    x0: f64,
    text: String,
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/floating-table-split")
            .join(format!("{name}.docx")),
    )
    .unwrap()
}

fn lines(bytes: &[u8]) -> Vec<Line> {
    docx_layout::with_private_measure_fonts(|| {
        let font = docx_layout::register_measure_font(FONT).unwrap();
        let engine = EngineSession::new(78903);
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
        // Text primitives are per character: group them into lines by page and
        // baseline, then take the leftmost x as the line's x0.
        let mut chars: Vec<(usize, f64, f64, char)> = Vec::new();
        for (page, page_value) in display["pages"].as_array().unwrap().iter().enumerate() {
            for primitive in page_value["primitives"].as_array().unwrap() {
                if primitive["kind"].as_str() != Some("text") {
                    continue;
                }
                let (Some(x), Some(y), Some(text)) = (
                    primitive["x"].as_f64(),
                    primitive["baselineY"].as_f64(),
                    primitive["text"].as_str(),
                ) else {
                    continue;
                };
                for c in text.chars() {
                    chars.push((page, y, x, c));
                }
            }
        }
        let mut out: Vec<Line> = Vec::new();
        chars.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
        });
        for (page, y, x, c) in chars {
            match out.last_mut() {
                Some(line) if line.page == page && (line.y - y).abs() < 0.01 => {
                    line.text.push(c);
                }
                _ => out.push(Line {
                    page,
                    y,
                    x0: x * PT_PER_PX,
                    text: c.to_string(),
                }),
            }
        }
        out
    })
}

fn body_lines(name: &str) -> Vec<Line> {
    // Only the anchor paragraph's lines: those that contain body words, not the
    // fixture labels ('floatNNNN ...') that belong to the table cells and heading.
    lines(&fixture(name))
        .into_iter()
        .filter(|line| {
            let text = line.text.replace(' ', "");
            let words = [
                "After", "wrap", "final", "rows", "continue", "page", "around", "table",
            ];
            words.iter().any(|w| text.contains(w))
                && !text.contains("float3600")
                && !text.contains("float7000")
        })
        .collect()
}

fn assert_body_lines_match_word(name: &str, expected: &[Value]) {
    let actual = body_lines(name);
    assert_eq!(
        actual.len(),
        expected.len(),
        "{name}: line count; actual {:?}",
        actual
            .iter()
            .map(|line| (
                line.page,
                line.x0,
                line.y * PT_PER_PX,
                line.text.replace(' ', "")
            ))
            .collect::<Vec<_>>()
    );
    for (index, (line, word)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            line.page as u64,
            word["page"].as_u64().unwrap(),
            "{name}[{index}]: page"
        );
        assert_eq!(
            line.text.replace(' ', ""),
            word["text"].as_str().unwrap(),
            "{name}[{index}]: text"
        );
        assert!(
            (line.x0 - word["x0"].as_f64().unwrap()).abs() < 2.0,
            "{name}[{index}]: x {}",
            line.x0
        );
        assert!(
            (line.y * PT_PER_PX - word["y"].as_f64().unwrap()).abs() < 0.5,
            "{name}[{index}]: y {}",
            line.y * PT_PER_PX
        );
    }
}

fn last_fragment_page(name: &str) -> usize {
    // The table's own cell text ('floatNNNN r...') marks the pages it occupies.
    lines(&fixture(name))
        .into_iter()
        .filter(|line| {
            line.text.replace(' ', "").contains(name) && line.text.replace(' ', "").contains("r")
        })
        .map(|line| line.page)
        .max()
        .unwrap_or(0)
}

#[test]
fn text_keeps_wrapping_beside_the_last_fragment_of_a_split_float() {
    for (name, column_x0) in [("float3600", 261.0), ("float7000", 431.0)] {
        let wrapped = body_lines(name)
            .into_iter()
            .filter(|line| (line.x0 - column_x0).abs() < 2.0)
            .count();
        assert!(
            wrapped >= 1,
            "{name}: no lines wrap beside the last fragment"
        );
    }
}

#[test]
fn no_text_stays_squeezed_on_pages_after_the_split_float_ends() {
    for (name, column_x0) in [("float3600", 261.0), ("float7000", 431.0)] {
        let after = last_fragment_page(name);
        let squeezed = body_lines(name)
            .into_iter()
            .filter(|line| line.page > after && (line.x0 - column_x0).abs() < 2.0)
            .count();
        assert_eq!(
            squeezed, 0,
            "{name}: {squeezed} line(s) still wrap at x0 {column_x0}pt on pages after the table's last fragment (page {after})"
        );
    }
}

#[test]
fn a_line_too_wide_for_the_wrap_column_leaves_it() {
    // float7000's wrap column is 92pt wide; the paragraph's last line is wider,
    // so Word and the engine both lay it out at the full column width.
    let last = body_lines("float7000")
        .pop()
        .expect("float7000: no body lines");
    assert!(
        (last.x0 - LEFT_MARGIN_PT).abs() < 2.0,
        "float7000: the paragraph's last line stays squeezed at x0 {:.1}pt",
        last.x0
    );
}

#[test]
fn a_line_too_wide_for_the_wrap_column_moves_to_the_next_page_like_word() {
    let word: Value =
        serde_json::from_str(include_str!("fixtures/floating-table-split/word.json")).unwrap();
    let expected = word["lines"]["float7000"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|line| {
            let text = line["text"].as_str().unwrap();
            let text = text
                .split_once("After")
                .map(|(_, suffix)| format!("After{suffix}"))
                .or_else(|| {
                    ["wrap", "final", "floating", "continue"]
                        .iter()
                        .any(|prefix| text.starts_with(prefix))
                        .then(|| text.to_owned())
                })?;
            Some((line["page"].as_u64().unwrap(), text))
        })
        .collect::<Vec<_>>();
    let actual = body_lines("float7000")
        .into_iter()
        .map(|line| (line.page as u64, line.text.replace(' ', "")))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn split_float_page_boundaries_match_word() {
    let word: Value = serde_json::from_str(include_str!(
        "fixtures/floating-table-split/word-page-matrix.json"
    ))
    .unwrap();
    for (name, expected) in word["cases"].as_object().unwrap() {
        assert_body_lines_match_word(name, expected.as_array().unwrap());
    }
}

#[test]
fn widow_controls_on_split_float_match_word() {
    let word: Value = serde_json::from_str(include_str!(
        "fixtures/floating-table-split/word-controls.json"
    ))
    .unwrap();
    for name in ["control-widow0", "control-widow1"] {
        assert_body_lines_match_word(name, word["cases"][name].as_array().unwrap());
    }
}

#[test]
fn keep_lines_after_split_float_moves_entire_paragraph_like_word() {
    let word: Value = serde_json::from_str(include_str!(
        "fixtures/floating-table-split/word-controls.json"
    ))
    .unwrap();
    assert_body_lines_match_word(
        "control-keep",
        word["cases"]["control-keep"].as_array().unwrap(),
    );
}
