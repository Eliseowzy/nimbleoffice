//! Text-box text lays out as Word's does: through its paragraph and character styles,
//! inset by half the box's outline, and with a MACROBUTTON's display text shown.

#[path = "support/word_lines.rs"]
mod word_lines;

use serde_json::Value;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/textbox-text/{name}.docx",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn word() -> Value {
    serde_json::from_str(include_str!("fixtures/textbox-text/word.json")).unwrap()
}

/// The lines break where Word's do; Noto Sans SC stands in for SimSun, so baselines differ.
fn assert_lines_match(name: &str) {
    let page = &word_lines::layout(&fixture(name))[0];
    let texts: Vec<_> = page.lines.iter().map(|line| line.text.clone()).collect();
    assert_eq!(texts, word_lines::word_texts(&word(), name, 0), "{name}");
}

#[test]
fn text_box_paragraphs_take_their_styles() {
    assert_lines_match("styles");
}

#[test]
fn text_box_text_clears_half_the_outline() {
    assert_lines_match("outline");
}

#[test]
fn a_macro_button_shows_its_display_text() {
    assert_lines_match("macro");
}
