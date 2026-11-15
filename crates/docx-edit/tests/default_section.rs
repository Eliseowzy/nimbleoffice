//! A package with no `w:sectPr` lays out on Word's default section (A4, 3.17cm sides, a
//! line grid), and a `w:sectPr` without margins keeps Word's 3.17cm sides. Baselines come
//! from Word 16.114's PDF export (`word.json`).

#[path = "support/word_lines.rs"]
mod word_lines;

use serde_json::Value;

fn agrees_with_word(name: &str) {
    let word: Value =
        serde_json::from_str(include_str!("fixtures/default-section/word.json")).unwrap();
    let bytes = std::fs::read(format!(
        "{}/tests/fixtures/default-section/{name}.docx",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let pages = word_lines::layout(&bytes);
    assert_eq!(pages.len(), word[name].as_array().unwrap().len(), "{name}");
    assert!(
        (pages[0].width - word[name][0]["width"].as_f64().unwrap()).abs() < 0.3,
        "{name} page width"
    );
    assert!(
        (pages[0].height - word[name][0]["height"].as_f64().unwrap()).abs() < 0.3,
        "{name} page height"
    );
    for line in word[name][0]["lines"].as_array().unwrap() {
        let text = line["text"].as_str().unwrap();
        let expected = line["y"].as_f64().unwrap();
        let actual = pages[0].baseline(text);
        assert!(
            (actual - expected).abs() < 0.6,
            "{name} {text}: {actual} != {expected}"
        );
    }
}

#[test]
fn no_sectpr_uses_word_defaults() {
    agrees_with_word("no-sectpr");
}

#[test]
fn empty_sectpr_keeps_letter_with_wide_sides() {
    agrees_with_word("empty-sectpr");
}

#[test]
fn sectpr_with_only_a_page_size_keeps_wide_sides() {
    agrees_with_word("pgsz-only");
}
