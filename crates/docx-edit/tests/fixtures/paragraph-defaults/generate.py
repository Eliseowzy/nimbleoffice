from pathlib import Path
import zipfile


W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
CT = "application/vnd.openxmlformats-officedocument.wordprocessingml"
SECTION = '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720"/></w:sectPr>'


def styles(paragraph_default):
    return f'<w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="24"/></w:rPr></w:rPrDefault>{paragraph_default}</w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>'


def document(name, paragraph_default):
    body = "".join(f"<w:p><w:r><w:t>LINE {i}</w:t></w:r></w:p>" for i in range(1, 4))
    body += '<w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:t>DIRECT</w:t></w:r></w:p><w:p><w:r><w:t>AFTER</w:t></w:r></w:p>'
    parts = {
        "[Content_Types].xml": f'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="{CT}.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="{CT}.styles+xml"/></Types>',
        "_rels/.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>',
        "word/_rels/document.xml.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="{REL}/styles" Target="styles.xml"/></Relationships>',
        "word/document.xml": f"<w:document {W}><w:body>{body}{SECTION}</w:body></w:document>",
        "word/styles.xml": styles(paragraph_default),
    }
    destination = Path(__file__).parent / f"{name}.docx"
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
        for key, value in parts.items():
            archive.writestr(zipfile.ZipInfo(key, date_time=(1980, 1, 1, 0, 0, 0)), value)


def main():
    document("absent", "")
    document("empty", "<w:pPrDefault/>")


if __name__ == "__main__":
    main()
