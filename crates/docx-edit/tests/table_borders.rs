use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const FONT: &[u8] = include_bytes!("../../ooxml-text/tests/fonts/LiberationSans-Regular.ttf");
const PT_PER_PX: f64 = 0.75;

fn baselines(primitive: &Value, out: &mut Vec<f64>) {
    let y = match primitive["kind"].as_str() {
        Some("text") => primitive["baselineY"].as_f64(),
        Some("glyphRun") => primitive["glyphs"][0]["y"].as_f64(),
        _ => None,
    };
    if let Some(y) = y {
        let y = y * PT_PER_PX;
        if out.iter().all(|seen| (seen - y).abs() > 0.01) {
            out.push(y);
        }
        return;
    }
    for key in ["primitives", "children", "content"] {
        for child in primitive[key].as_array().into_iter().flatten() {
            baselines(child, out);
        }
    }
}

fn layout(bytes: &[u8]) -> Vec<f64> {
    docx_layout::with_private_measure_fonts(|| {
        let font = docx_layout::register_measure_font(FONT).unwrap();
        let engine = EngineSession::new(78902);
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
                "fontChains": {"arial|0|0": [font]},
                "defaults": {"fontFamily": "Arial", "fontSize": 12},
                "authoritativeShaping": true
            }
        });
        let layout = engine
            .layout_document_with_regions_json(&request.to_string())
            .unwrap();
        let display: Value =
            serde_json::from_str(&engine.build_display_list_json(&layout).unwrap()).unwrap();
        let mut ys = Vec::new();
        for primitive in display["pages"][0]["primitives"].as_array().unwrap() {
            baselines(primitive, &mut ys);
        }
        ys.sort_by(f64::total_cmp);
        ys
    })
}

#[test]
fn cell_text_baselines_match_word() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/table-borders/word.json")).unwrap();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/table-borders");
    let mut failures = Vec::new();
    for (name, expected) in oracle.as_object().unwrap() {
        let bytes = std::fs::read(format!("{dir}/{name}.docx")).unwrap();
        let actual = layout(&bytes);
        let expected: Vec<f64> = expected
            .as_array()
            .unwrap()
            .iter()
            .map(|y| y.as_f64().unwrap())
            .collect();
        if actual.len() != expected.len()
            || actual
                .iter()
                .zip(&expected)
                .any(|(a, w)| (a - w).abs() > 0.5)
        {
            failures.push(format!("{name}: engine {actual:?} word {expected:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
