//! A header part shared by sections of different widths is painted with the measure of each
//! page's own section (fixtures/header-section-variant).

use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const FONT: &[u8] = include_bytes!("../../ooxml-text/tests/fonts/LiberationSans-Regular.ttf");

fn display(name: &str) -> Value {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/header-section-variant")
            .join(name),
    )
    .unwrap();
    docx_layout::with_private_measure_fonts(|| {
        let font = docx_layout::register_measure_font(FONT).unwrap();
        let engine = EngineSession::new(78917);
        seed_from_docx(engine.doc(), &bytes).unwrap();
        let package = docx_parse::parse_docx_s9_wire(&bytes, Default::default())
            .unwrap()
            .document
            .package;
        let mut sections: Vec<Value> = package
            .document
            .sections
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|section| json!({"sectionId": section.id, "properties": section.properties}))
            .collect();
        sections.push(json!({"properties": package.document.final_section_properties}));
        let chain = json!([font]);
        let request = json!({
            "bodyStory": "body", "renderEnv": {},
            "regions": {"sections": sections, "settings": package.settings},
            "measurement": {"fontChains": {"calibri|0|0": chain, "carlito|0|0": chain, "arial|0|0": chain,
                                           "times new roman|0|0": chain},
                            "defaults": {"fontFamily": "Calibri", "fontSize": 11}, "authoritativeShaping": true}
        });
        let layout = engine
            .layout_document_with_regions_json(&request.to_string())
            .unwrap();
        serde_json::from_str(&engine.build_display_list_json(&layout).unwrap()).unwrap()
    })
}

fn right_edge(primitives: &Value) -> f64 {
    let mut right: f64 = 0.0;
    fn walk(p: &Value, right: &mut f64) {
        if p["kind"] == "text" && p["text"].as_str().is_some_and(|t| !t.trim().is_empty()) {
            *right = right.max(p["x"].as_f64().unwrap() + p["width"].as_f64().unwrap_or(0.0));
        }
        for c in p["children"].as_array().into_iter().flatten() {
            walk(c, right);
        }
    }
    for p in primitives.as_array().into_iter().flatten() {
        walk(p, &mut right);
    }
    right
}

#[test]
fn a_portrait_page_paints_its_own_sections_header_measure() {
    let display = display("portrait-then-landscape.docx");
    let page = &display["pages"][0];
    let width = page["width"].as_f64().unwrap();
    let header = right_edge(&page["header"]["primitives"]);
    assert!(header > 0.0, "page one paints a header");
    assert!(
        header <= width,
        "page one's header runs to {header:.1}px on a {width:.1}px page"
    );
}
