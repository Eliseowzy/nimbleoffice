#![cfg(feature = "pdf")]

use nimbleoffice_docx::{Document, PdfExportOptions};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#;

fn docx(body: &str) -> Vec<u8> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let parts = vec![
        (
            "[Content_Types].xml".to_owned(),
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec(),
        ),
        (
            "_rels/.rels".to_owned(),
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec(),
        ),
        ("word/document.xml".to_owned(), document.into_bytes()),
    ];
    ooxml_opc::rezip_parts(&parts).unwrap()
}

fn paragraph(id: &str, inner: &str) -> String {
    format!(r#"<w:p w14:paraId="{id}">{inner}</w:p>"#)
}

fn run(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

fn page_break() -> String {
    r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#.to_owned()
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

#[test]
fn writes_one_pdf_page_per_laid_out_page() {
    let body = [
        paragraph("00000001", &run("First page")),
        page_break(),
        paragraph("00000002", &run("Second page")),
    ]
    .concat();
    let document = Document::open(&docx(&body)).unwrap();
    let export = document.render_pdf(&PdfExportOptions::default()).unwrap();
    assert!(export.bytes.starts_with(b"%PDF-"));
    assert_eq!(export.total_pages, 2);
    assert_eq!(export.pages, 2);
    assert_eq!(export.raster_patches, 0, "plain text is drawn as text");
    assert_eq!(export.skipped_primitives, 0);
}

#[test]
fn exports_a_page_range() {
    let body = [
        paragraph("00000001", &run("One")),
        page_break(),
        paragraph("00000002", &run("Two")),
        page_break(),
        paragraph("00000003", &run("Three")),
    ]
    .concat();
    let document = Document::open(&docx(&body)).unwrap();
    let options = PdfExportOptions {
        pages: Some(1..=2),
        ..PdfExportOptions::default()
    };
    let export = document.render_pdf(&options).unwrap();
    assert_eq!((export.pages, export.total_pages), (2, 3));
    let outside = PdfExportOptions {
        pages: Some(5..=6),
        ..PdfExportOptions::default()
    };
    assert!(document.render_pdf(&outside).is_err());
}

#[test]
fn embeds_the_fonts_as_text() {
    let document = Document::open(&docx(&paragraph("00000001", &run("Selectable")))).unwrap();
    let export = document.render_pdf(&PdfExportOptions::default()).unwrap();
    assert!(
        count(&export.bytes, b"/ToUnicode") > 0,
        "text maps back to characters"
    );
    assert!(
        count(&export.bytes, b"/FontFile2") + count(&export.bytes, b"/FontFile3") > 0,
        "the font is embedded"
    );
    assert!(export.outlined_fonts.is_empty());
}

#[test]
fn headings_become_bookmarks_and_anchors_links() {
    let body = [
        paragraph(
            "00000001",
            r#"<w:hyperlink w:anchor="target"><w:r><w:t>Go to the target</w:t></w:r></w:hyperlink>"#,
        ),
        page_break(),
        paragraph(
            "00000002",
            &format!(
                r#"<w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:bookmarkStart w:id="1" w:name="target"/>{}<w:bookmarkEnd w:id="1"/>"#,
                run("Chapter one")
            ),
        ),
        paragraph(
            "00000003",
            &format!(r#"<w:pPr><w:outlineLvl w:val="1"/></w:pPr>{}"#, run("Section")),
        ),
        paragraph(
            "00000004",
            r#"<w:hyperlink w:anchor="nowhere"><w:r><w:t>broken</w:t></w:r></w:hyperlink>"#,
        ),
    ]
    .concat();
    let document = Document::open(&docx(&body)).unwrap();
    let export = document.render_pdf(&PdfExportOptions::default()).unwrap();
    let pdf = &export.bytes;
    assert!(count(pdf, b"/Outlines") > 0, "the document has bookmarks");
    assert!(
        count(pdf, b"(Chapter one)") == 1,
        "the heading is a bookmark"
    );
    assert!(
        count(pdf, b"(Section)") == 1,
        "the subheading is a bookmark"
    );
    assert_eq!(
        count(pdf, b"/Subtype/Link") + count(pdf, b"/Subtype /Link"),
        1,
        "only the link with a target"
    );
}

#[test]
fn accepts_tracked_changes_unless_markup() {
    let body = paragraph(
        "00000001",
        &format!(
            r#"{}<w:ins w:id="1" w:author="A" w:date="2024-01-01T00:00:00Z"><w:r><w:t>Inserted</w:t></w:r></w:ins><w:del w:id="2" w:author="A" w:date="2024-01-01T00:00:00Z"><w:r><w:delText>Deleted</w:delText></w:r></w:del>"#,
            run("Kept ")
        ),
    );
    let document = Document::open(&docx(&body)).unwrap();
    let final_view = document.render_pdf(&PdfExportOptions::default()).unwrap();
    let markup = document
        .render_pdf(&PdfExportOptions {
            markup: true,
            ..PdfExportOptions::default()
        })
        .unwrap();
    // The markup view draws the deleted word and the revision marks too.
    assert!(markup.bytes.len() > final_view.bytes.len());
    // Exporting leaves the document's own revisions alone.
    let again = document.render_pdf(&PdfExportOptions::default()).unwrap();
    assert_eq!(again.bytes.len(), final_view.bytes.len());
}
