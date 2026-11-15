//! A line an inline image grows, snapped to the document grid, centres its content in
//! the rows as a text line does.

#[path = "support/word_lines.rs"]
mod word_lines;

use serde_json::Value;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/grid-image-line/{name}.docx",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn word() -> Value {
    serde_json::from_str(include_str!("fixtures/grid-image-line/word.json")).unwrap()
}

#[test]
fn text_beside_an_image_sits_where_word_puts_it() {
    let word = word();
    for name in ["text-tall", "text-short"] {
        let page = &word_lines::layout(&fixture(name))[0];
        for text in ["DOT", "AFTER"] {
            let offset = page.baseline(text) - page.baseline("TOP");
            let expected = word_lines::word_baseline(&word, name, 0, text)
                - word_lines::word_baseline(&word, name, 0, "TOP");
            assert!(
                (offset - expected).abs() < 0.5,
                "{name} {text}: {offset} vs {expected}"
            );
        }
    }
}

#[test]
fn an_image_alone_is_centred_in_its_rows() {
    let word = word();
    for name in ["alone-tall", "alone-short"] {
        let page = &word_lines::layout(&fixture(name))[0];
        let (top, _) = page.images[0];
        let expected = word[name][0]["images"][0][0].as_f64().unwrap();
        assert!((top - expected).abs() < 0.5, "{name}: {top} vs {expected}");
    }
}
