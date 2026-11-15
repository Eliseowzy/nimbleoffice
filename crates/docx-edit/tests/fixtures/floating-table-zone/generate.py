from pathlib import Path
from zipfile import ZIP_STORED, ZipFile, ZipInfo

"""Zone-bottom experiment: where does text resume below a split floating table?

Page A4 (595.3 x 841.9pt), 72pt margins. A 250pt-wide floating table anchored
to the page at tblpY=100 with five 150pt rows: 750pt total, so row 5 does not
fit the content bottom (769.9) and the table splits -- fragment one holds rows
1-4, ending around y=700, with ~70pt of room below. The paragraph after the
table wraps beside the fragment, then goes full width once past its bottom;
that first full-width line measures the zone bottom.

Variants vary only the table border size: sz4 (0.5pt) vs none.
Word's PDF export is measured with tools/word-parity/word_lines.py.
"""

ROOT = Path(__file__).resolve().parent
VARIANTS = ("sz4", "none")

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"""
PACKAGE_RELS = """<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rDoc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"""
DOCUMENT_RELS = """<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rSettings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"""
STYLES = """<?xml version="1.0" encoding="UTF-8"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial"/><w:sz w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"""
SETTINGS = """<?xml version="1.0" encoding="UTF-8"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"""

TEXT = " ".join(f"wrapword{i:02d}" for i in range(1, 41))


def borders_xml(variant: str) -> str:
    if variant == "none":
        return '<w:tblBorders><w:top w:val="none" w:sz="0" w:space="0" w:color="auto"/><w:left w:val="none" w:sz="0" w:space="0" w:color="auto"/><w:bottom w:val="none" w:sz="0" w:space="0" w:color="auto"/><w:right w:val="none" w:sz="0" w:space="0" w:color="auto"/><w:insideH w:val="none" w:sz="0" w:space="0" w:color="auto"/><w:insideV w:val="none" w:sz="0" w:space="0" w:color="auto"/></w:tblBorders>'
    return '<w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders>'


def document_xml(variant: str) -> str:
    rows = "".join(
        f'<w:tr><w:trPr><w:trHeight w:val="3000" w:hRule="atLeast"/></w:trPr>'
        f'<w:tc><w:tcPr><w:tcW w:w="2500" w:type="dxa"/></w:tcPr>'
        f'<w:p><w:r><w:t xml:space="preserve">row{i}</w:t></w:r></w:p></w:tc>'
        f'<w:tc><w:tcPr><w:tcW w:w="2500" w:type="dxa"/></w:tcPr><w:p/></w:tc></w:tr>'
        for i in range(1, 6)
    )
    return (
        '<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>'
        '<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="page" w:horzAnchor="margin" w:tblpX="0" w:tblpY="2000"/>'
        '<w:tblW w:w="5000" w:type="dxa"/>' + borders_xml(variant) +
        '<w:tblLayout w:type="fixed"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr>'
        '<w:tblGrid><w:gridCol w:w="2500"/><w:gridCol w:w="2500"/></w:tblGrid>' + rows +
        '</w:tbl><w:p><w:r><w:t xml:space="preserve">' + TEXT + '</w:t></w:r></w:p>'
        '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>'
    )


def write(path: Path, text: str) -> None:
    with ZipFile(path, "w") as zf:
        zf.writestr(ZipInfo("[Content_Types].xml"), CONTENT_TYPES)
        zf.writestr(ZipInfo("_rels/.rels"), PACKAGE_RELS)
        zf.writestr(ZipInfo("word/document.xml"), text)
        zf.writestr(ZipInfo("word/_rels/document.xml.rels"), DOCUMENT_RELS)
        zf.writestr(ZipInfo("word/styles.xml"), STYLES)
        zf.writestr(ZipInfo("word/settings.xml"), SETTINGS)


for variant in VARIANTS:
    write(ROOT / f"zone-{variant}.docx", document_xml(variant))
    print(f"generated zone-{variant}.docx")
