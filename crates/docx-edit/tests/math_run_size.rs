//! An equation sets in its runs' own size, not the paragraph style's.

use docx_edit::{EngineSession, bridge::RenderEnv, seed_from_docx};
use serde_json::Value;

#[test]
fn an_equation_takes_its_run_size() {
    let body = r#"<w:p><m:oMath><m:r><w:rPr><w:rFonts w:ascii="Cambria Math" w:hAnsi="Cambria Math"/><w:sz w:val="48"/></w:rPr><m:t>R</m:t></m:r></m:oMath></w:p>"#;
    let parts = [
        ("[Content_Types].xml", r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_owned()),
        ("_rels/.rels", r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_owned()),
        ("word/document.xml", format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><w:body>{body}<w:sectPr/></w:body></w:document>"#)),
    ];
    let bytes = ooxml_opc::rezip_parts(
        &parts
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.into_bytes()))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let engine = EngineSession::new(76602);
    seed_from_docx(engine.doc(), &bytes).unwrap();
    let lowered: Value = serde_json::from_str(
        &engine
            .lower_story_json("body", &RenderEnv::default())
            .unwrap(),
    )
    .unwrap();
    let text = lowered.to_string();
    assert!(
        text.contains(r#""fontFamily":"Cambria Math","fontSize":24.0"#),
        "{text}"
    );
}
