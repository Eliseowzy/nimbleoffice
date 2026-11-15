//! Without a `w:pPrDefault`, Word spaces paragraphs 8pt apart at 1.15 lines; an empty
//! one means no spacing.

#[path = "support/word_lines.rs"]
mod word_lines;

use serde_json::Value;

fn pitch(lines: impl Fn(&str) -> f64) -> f64 {
    lines("LINE2") - lines("LINE1")
}

#[test]
fn an_absent_paragraph_default_adds_words_spacing() {
    let word: Value =
        serde_json::from_str(include_str!("fixtures/paragraph-defaults/word.json")).unwrap();
    let engine = |name: &str| {
        let bytes = std::fs::read(format!(
            "{}/tests/fixtures/paragraph-defaults/{name}.docx",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        pitch(|text| word_lines::layout(&bytes)[0].baseline(text))
    };
    let expected = pitch(|text| word_lines::word_baseline(&word, "absent", 0, text))
        - pitch(|text| word_lines::word_baseline(&word, "empty", 0, text));
    let grown = engine("absent") - engine("empty");
    assert!((grown - expected).abs() < 0.5, "{grown} vs {expected}");
}
