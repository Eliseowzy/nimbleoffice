from pathlib import Path
import zipfile


W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
CT = "application/vnd.openxmlformats-officedocument.wordprocessingml"
STYLES = f'<w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="exact"/><w:widowControl w:val="0"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>'
FOOTER = f'<w:ftr {W}><w:p><w:r><w:t>FOOT</w:t></w:r></w:p></w:ftr>'


def paragraph(text):
    return f"<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"


def section(kind, grid=""):
    return f'<w:sectPr><w:footerReference w:type="default" r:id="ftr"/><w:type w:val="{kind}"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720"/>{grid}</w:sectPr>'


def document(name, body, kind, grid=""):
    parts = {
        "[Content_Types].xml": f'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="{CT}.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="{CT}.styles+xml"/><Override PartName="/word/footer1.xml" ContentType="{CT}.footer+xml"/></Types>',
        "_rels/.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>',
        "word/_rels/document.xml.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="{REL}/styles" Target="styles.xml"/><Relationship Id="ftr" Type="{REL}/footer" Target="footer1.xml"/></Relationships>',
        "word/document.xml": f"<w:document {W}><w:body>{body}{section(kind, grid)}</w:body></w:document>",
        "word/styles.xml": STYLES,
        "word/footer1.xml": FOOTER,
    }
    destination = Path(__file__).parent / f"{name}.docx"
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
        for key, value in parts.items():
            archive.writestr(zipfile.ZipInfo(key, date_time=(1980, 1, 1, 0, 0, 0)), value)


def main():
    full = "".join(paragraph(f"LINE {i}") for i in range(1, 55))
    for kind in ["nextPage", "oddPage"]:
        mark = f"<w:p><w:pPr>{section(kind)}</w:pPr></w:p>"
        document(f"empty-{kind}", full + mark + paragraph("NEXT"), kind)
        text_mark = f"<w:p><w:pPr>{section(kind)}</w:pPr><w:r><w:t>MARK</w:t></w:r></w:p>"
        document(f"text-{kind}", full + text_mark + paragraph("NEXT"), kind)
    grid = '<w:docGrid w:type="lines" w:linePitch="312"/>'
    auto = '<w:pPr><w:spacing w:line="240" w:lineRule="auto"/></w:pPr>'
    lines = "".join(f"<w:p>{auto}<w:r><w:t>ROW {i}</w:t></w:r></w:p>" for i in range(1, 41))
    hanging = f'<w:p>{auto}<w:r><w:rPr><w:sz w:val="28"/></w:rPr><w:t>HANGING</w:t></w:r></w:p>'
    mark = f"<w:p><w:pPr>{section('oddPage', grid)}</w:pPr></w:p>"
    document("hanging-oddPage", lines + hanging + mark + paragraph("NEXT"), "oddPage", grid)


if __name__ == "__main__":
    main()
