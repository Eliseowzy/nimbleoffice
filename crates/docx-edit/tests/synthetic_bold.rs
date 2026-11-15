//! Word widens each glyph of a bold run whose face has no bold weight, as it emboldens it.

#[path = "support/word_lines.rs"]
mod word_lines;

use serde_json::Value;

#[test]
fn a_synthesized_bold_run_is_as_wide_as_words() {
    let word: Value =
        serde_json::from_str(include_str!("fixtures/synthetic-bold/word.json")).unwrap();
    let bytes = include_bytes!("fixtures/synthetic-bold/synthetic-bold.docx");
    let pages = word_lines::layout(bytes);
    let lines: Vec<_> = pages[0]
        .lines
        .iter()
        .filter(|line| line.text.chars().all(|c| c.is_ascii_uppercase()))
        .collect();
    let (bold, regular) = (lines[0], lines[1]);
    let grown = bold.right - regular.right;
    let word_lines = word["synthetic-bold"][0]["lines"].as_array().unwrap();
    let expected = word_lines[1]["x1"].as_f64().unwrap() - word_lines[2]["x1"].as_f64().unwrap();
    assert!((grown - expected).abs() < 0.5, "{grown} vs {expected}");
}
