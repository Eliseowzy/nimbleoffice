use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const ONE_ROW: &[u8] = include_bytes!("fixtures/grid-line-rules/one-row.docx");
const TWO_ROWS: &[u8] = include_bytes!("fixtures/grid-line-rules/two-rows.docx");

fn render(bytes: &[u8]) -> Value {
    let serif = docx_layout::register_measure_font(include_bytes!(
        "../../../packages/fonts/assets/LiberationSerif-Regular.ttf"
    ))
    .unwrap();
    let package = docx_parse::parse_docx_s9_wire(bytes, Default::default())
        .unwrap()
        .document
        .package;
    let engine = EngineSession::new(76402);
    seed_from_docx(engine.doc(), bytes).unwrap();
    serde_json::from_str(
        &engine
            .layout_document_with_regions_json(
                &json!({
                    "bodyStory": "body", "renderEnv": {},
                    "regions": {"sections": [{"properties": package.document.final_section_properties}], "settings": package.settings},
                    "measurement": {"fontChains": {"times new roman|0|0": [serif]}, "defaults": {"fontFamily": "Times New Roman", "fontSize": 12}}
                })
                .to_string(),
            )
            .unwrap(),
    )
    .unwrap()
}

fn heights(output: &Value) -> Vec<(String, f64)> {
    output["layout"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|page| page["fragments"].as_array().unwrap())
        .map(|fragment| {
            let tag: String = fragment["resolvedLines"][0]["segments"][0]["text"]
                .as_str()
                .unwrap()
                .chars()
                .take(3)
                .collect();
            (tag, fragment["height"].as_f64().unwrap() * 0.75)
        })
        .collect()
}

fn word_pitch(fixture: &str, tag: &str) -> f64 {
    let baseline: Value =
        serde_json::from_str(include_str!("fixtures/grid-line-rules/word.json")).unwrap();
    let ys: Vec<f64> = baseline[fixture][0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|line| line["text"].as_str().unwrap().starts_with(tag))
        .map(|line| line["y"].as_f64().unwrap())
        .collect();
    assert_eq!(ys.len(), 3, "{fixture}: {tag}");
    (ys[2] - ys[0]) / 2.0
}

#[test]
fn grid_line_heights_match_word() {
    for (fixture, bytes) in [("one-row", ONE_ROW), ("two-rows", TWO_ROWS)] {
        let measured = heights(&render(bytes));
        assert_eq!(measured.len(), 6, "{fixture}");
        for (tag, height) in measured {
            let expected = 3.0 * word_pitch(fixture, &tag);
            assert!(
                (height - expected).abs() < 1.0,
                "{fixture} {tag}: {height}pt != Word {expected}pt"
            );
        }
    }
}
