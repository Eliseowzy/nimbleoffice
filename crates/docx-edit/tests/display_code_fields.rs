use docx_edit::bridge::{RenderEnv, yrs_doc_to_layout_blocks};
use docx_edit::{EditingDoc, seed_from_docx};
use docx_layout::types::{LayoutBlock, Run};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn document(body: &str) -> Vec<u8> {
    let parts = [
        ("[Content_Types].xml", r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_owned()),
        ("_rels/.rels", r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_owned()),
        ("word/document.xml", format!(r#"<w:document xmlns:w="{W}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"#)),
    ];
    ooxml_opc::rezip_parts(
        &parts
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.into_bytes()))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn code_run(text: &str, rpr: &str) -> String {
    format!(
        r#"<w:r><w:rPr>{rpr}</w:rPr><w:instrText xml:space="preserve">{text}</w:instrText></w:r>"#
    )
}

fn macro_field(name_only: bool, display: &[(&str, &str)]) -> String {
    let mut out = String::from(r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#);
    out.push_str(&code_run(" MACROBUTTON  AcceptAllChangesShown ", ""));
    if !name_only {
        for (text, rpr) in display {
            out.push_str(&code_run(text, rpr));
        }
    }
    out.push_str(r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#);
    out
}

fn first_paragraph_runs(body: &str) -> Vec<Run> {
    let doc = EditingDoc::new(1);
    seed_from_docx(&doc, &document(body)).unwrap();
    let blocks = yrs_doc_to_layout_blocks(&doc, "body", &RenderEnv::default()).unwrap();
    match blocks.into_iter().next().unwrap() {
        LayoutBlock::Paragraph(paragraph) => paragraph.runs,
        _ => panic!("not a paragraph"),
    }
}

#[test]
fn display_text_lowers_as_a_plain_text_run() {
    let body = format!(
        "<w:p>{}</w:p>",
        macro_field(false, &[("［点此输入］", "<w:b/>")])
    );
    let runs = first_paragraph_runs(&body);
    assert!(matches!(
        &runs[..],
        [Run::Text(text)] if text.text == "［点此输入］" && text.fmt.bold == Some(true)
    ));
}

#[test]
fn macro_name_without_display_text_takes_no_run() {
    let body = format!(
        "<w:p>{}{}</w:p>",
        macro_field(true, &[]),
        macro_field(false, &[("题目", "")])
    );
    let runs = first_paragraph_runs(&body);
    assert!(matches!(&runs[..], [Run::Text(text)] if text.text == "题目"));
}

#[test]
fn display_text_inside_a_hyperlink_with_a_bookmark_in_the_field_is_kept() {
    let body = format!(
        r#"<w:p><w:hyperlink w:anchor="_Toc1"><w:r><w:t>1.1</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r>{}{}{}{}<w:bookmarkStart w:id="1" w:name="_Toc2"/><w:r><w:fldChar w:fldCharType="end"/></w:r><w:bookmarkEnd w:id="1"/><w:r><w:tab/></w:r></w:hyperlink></w:p>"#,
        code_run(" MACROBUTTON  AcceptAllChangesShown [", ""),
        code_run("单击输入二级标题", ""),
        code_run("] ", ""),
        ""
    );
    let runs = first_paragraph_runs(&body);
    let text: String = runs
        .iter()
        .filter_map(|run| match run {
            Run::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "1.1[单击输入二级标题]");
}
