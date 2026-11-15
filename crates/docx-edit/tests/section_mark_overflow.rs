//! An empty paragraph that only carries a page-type section break stays on the page its
//! text ended on, even when the text already hangs past the bottom margin.

#[path = "support/word_lines.rs"]
mod word_lines;

use serde_json::Value;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/section-mark-overflow/{name}.docx",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn word() -> Value {
    serde_json::from_str(include_str!("fixtures/section-mark-overflow/word.json")).unwrap()
}

#[test]
fn the_section_mark_follows_text_that_hangs_past_the_bottom() {
    let word = word();
    let pages = word_lines::layout(&fixture("hanging-oddPage"));
    assert_eq!(
        pages.len(),
        word["hanging-oddPage"].as_array().unwrap().len()
    );
    let hanging = pages[0].baseline("HANGING");
    assert!(
        (hanging - word_lines::word_baseline(&word, "hanging-oddPage", 0, "HANGING")).abs() < 0.5
    );
    assert!(pages[1].parity_filler);
    assert!(pages[1].lines.is_empty());
    pages[2].baseline("NEXT");
}

#[test]
fn page_counts_match_word() {
    let word = word();
    for name in [
        "empty-nextPage",
        "empty-oddPage",
        "text-nextPage",
        "text-oddPage",
    ] {
        let pages = word_lines::layout(&fixture(name));
        assert_eq!(pages.len(), word[name].as_array().unwrap().len(), "{name}");
    }
}
