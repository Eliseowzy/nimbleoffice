//! A run's `w:hint` reaches layout, so a character Word sets in the East Asian face, such
//! as a private-use symbol under `w:hint="eastAsia"`, measures with that face.

use docx_edit::{EngineSession, bridge::RenderEnv, seed_from_docx};
use serde_json::Value;

#[test]
fn the_east_asian_hint_reaches_the_lowered_run() {
    let body = r#"<w:p><w:r><w:rPr><w:rFonts w:cs="宋体" w:hint="eastAsia"/></w:rPr><w:t>&#xE004;</w:t></w:r></w:p>"#;
    let parts = [
        ("[Content_Types].xml", r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_owned()),
        ("_rels/.rels", r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_owned()),
        ("word/document.xml", format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#)),
    ];
    let bytes = ooxml_opc::rezip_parts(
        &parts
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.into_bytes()))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let engine = EngineSession::new(76601);
    seed_from_docx(engine.doc(), &bytes).unwrap();
    let lowered: Value = serde_json::from_str(
        &engine
            .lower_story_json("body", &RenderEnv::default())
            .unwrap(),
    )
    .unwrap();
    let run = &lowered.to_string();
    assert!(run.contains(r#""hint":"eastAsia""#), "{run}");
}
