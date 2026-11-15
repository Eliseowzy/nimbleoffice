//! Superscript and subscript size and baseline offset against Word 16.114's
//! PDF export of `fixtures/vertical-align/script.docx` (`word.json`).

use serde_json::Value;

#[test]
fn script_size_and_offset_follow_word() {
    let word: Value =
        serde_json::from_str(include_str!("fixtures/vertical-align/word.json")).unwrap();
    for entry in word["sizes"].as_array().unwrap() {
        let size = entry["size"].as_f64().unwrap();
        let script = ooxml_text::script_size_pt(size);
        let expected = entry["scriptSize"].as_f64().unwrap();
        assert!(
            (script - expected).abs() <= 0.13,
            "{size}pt size {script} != {expected}"
        );
        let raise = ooxml_text::superscript_raise(size);
        let expected = entry["raise"].as_f64().unwrap();
        assert!(
            (raise - expected).abs() <= 0.6,
            "{size}pt raise {raise} != {expected}"
        );
        if let Some(expected) = entry["drop"].as_f64() {
            let drop = ooxml_text::subscript_drop(size);
            assert!(
                (drop - expected).abs() <= 0.5,
                "{size}pt drop {drop} != {expected}"
            );
        }
    }
}
