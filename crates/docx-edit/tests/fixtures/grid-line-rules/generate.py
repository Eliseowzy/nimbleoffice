from pathlib import Path
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
RULES = [
    ("A08", 'w:line="192" w:lineRule="auto"'),
    ("A15", 'w:line="360" w:lineRule="auto"'),
    ("A20", 'w:line="480" w:lineRule="auto"'),
    ("L00", 'w:line="0" w:lineRule="atLeast"'),
    ("L20", 'w:line="400" w:lineRule="atLeast"'),
    ("L30", 'w:line="600" w:lineRule="atLeast"'),
]


def paragraph(tag, spacing):
    runs = '<w:r><w:br/></w:r>'.join(f'<w:r><w:t>{tag} LINE {n}</w:t></w:r>' for n in range(3))
    return f'<w:p><w:pPr><w:spacing w:before="0" w:after="0" {spacing}/></w:pPr>{runs}</w:p>'


def document(name, size):
    styles = f'<w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="{size * 2}"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:widowControl w:val="0"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>'
    body = "".join(paragraph(tag, spacing) for tag, spacing in RULES)
    section = '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/><w:docGrid w:type="lines" w:linePitch="312"/></w:sectPr>'
    parts = {
        "[Content_Types].xml": '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>',
        "_rels/.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>',
        "word/_rels/document.xml.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="{REL}/styles" Target="styles.xml"/><Relationship Id="settings" Type="{REL}/settings" Target="settings.xml"/></Relationships>',
        "word/document.xml": f'<w:document {W}><w:body>{body}{section}</w:body></w:document>',
        "word/styles.xml": styles,
        "word/settings.xml": f'<w:settings {W}><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>',
    }
    with zipfile.ZipFile(Path(__file__).parent / f"{name}.docx", "w", zipfile.ZIP_DEFLATED) as archive:
        for key, value in parts.items():
            archive.writestr(zipfile.ZipInfo(key, date_time=(1980, 1, 1, 0, 0, 0)), value, zipfile.ZIP_DEFLATED)


def main():
    document("one-row", 12)
    document("two-rows", 14)


if __name__ == "__main__":
    main()
