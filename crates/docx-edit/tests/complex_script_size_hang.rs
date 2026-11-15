//! A run's `szCs` sizes only complex-script characters, so it must not stop
//! the trailing leading of a line with none of them from hanging below the
//! page's bottom margin. A Chinese thesis cover pairs `sz=28` with `szCs=24`
//! on its date line and Word keeps that line on the cover.

use docx_edit::{EngineSession, seed_from_docx};
use serde_json::{Value, json};

const FONT: &[u8] = include_bytes!("../../ooxml-text/tests/fonts/LiberationSans-Regular.ttf");

fn document(last_run_props: &str) -> Vec<u8> {
    // FIRST is pinned at 144px; LAST is double-spaced 12pt (about 36.8px with
    // 18.4px of trailing leading). The 175px column holds LAST only if that
    // leading may hang.
    let body = format!(
        r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="2160" w:lineRule="exact"/></w:pPr><w:r><w:t>FIRST</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="auto"/></w:pPr><w:r><w:rPr>{last_run_props}</w:rPr><w:t>LAST 2018</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="12240" w:h="4065"/><w:pgMar w:top="720" w:right="1440" w:bottom="720" w:left="1440"/></w:sectPr>"#
    );
    let styles = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#;
    let parts = [
        ("[Content_Types].xml", r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_owned()),
        ("_rels/.rels", r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_owned()),
        ("word/_rels/document.xml.rels", r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_owned()),
        ("word/document.xml", format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#)),
        ("word/styles.xml", styles.to_owned()),
    ];
    ooxml_opc::rezip_parts(
        &parts
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.into_bytes()))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn pages(last_run_props: &str) -> usize {
    let bytes = document(last_run_props);
    let font = docx_layout::register_measure_font(FONT).unwrap();
    let package = docx_parse::parse_docx_s9_wire(&bytes, Default::default())
        .unwrap()
        .document
        .package;
    let engine = EngineSession::new(76500);
    seed_from_docx(engine.doc(), &bytes).unwrap();
    let output: Value = serde_json::from_str(&engine.layout_document_with_regions_json(&json!({
        "bodyStory": "body", "renderEnv": {},
        "regions": {"sections": [{"properties": package.document.final_section_properties}], "settings": package.settings},
        "measurement": {"fontChains": {"arial|0|0": [font]}, "defaults": {"fontFamily": "Arial", "fontSize": 12}}
    }).to_string()).unwrap()).unwrap();
    output["layout"]["pages"].as_array().unwrap().len()
}

#[test]
fn trailing_leading_hangs_below_the_margin() {
    assert_eq!(pages(""), 1);
}

#[test]
fn a_complex_script_size_does_not_stop_latin_text_hanging() {
    assert_eq!(pages(r#"<w:szCs w:val="20"/>"#), 1);
}
