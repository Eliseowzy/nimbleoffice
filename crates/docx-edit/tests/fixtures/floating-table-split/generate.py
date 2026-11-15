from pathlib import Path
from zipfile import ZIP_STORED, ZipFile, ZipInfo

"""Regenerate the floating-table-split fixtures.

Two documents share one body: a floating table whose two atLeast rows together
(2 x 1247 twips) overflow the 225pt-tall page, so the table splits across pages,
followed by a paragraph that wraps beside the last fragment and then continues.
float3600's table is 180pt wide (wrap column at x0=261.5), float7000's is 350pt
wide (wrap column at x0=431.5). The Word-measured output is in word.json.
"""

ROOT = Path(__file__).resolve().parent
FIXTURES = (("float3600", "1800"), ("float7000", "3500"))

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"""
PACKAGE_RELS = """<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rDoc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"""
DOCUMENT_RELS = """<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rSettings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"""
STYLES = """<?xml version="1.0" encoding="UTF-8"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial"/><w:sz w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"""
SETTINGS = """<?xml version="1.0" encoding="UTF-8"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"""


def document_xml(name: str, col: str) -> str:
    row = (
        '<w:tr><w:trPr><w:trHeight w:val="1247" w:hRule="atLeast"/></w:trPr>'
        '<w:tc><w:tcPr><w:tcW w:w="' + col + '" w:type="dxa"/></w:tcPr>'
        '<w:p><w:r><w:t xml:space="preserve">' + name + ' r{row}c1</w:t></w:r></w:p></w:tc>'
        '<w:tc><w:tcPr><w:tcW w:w="' + col + '" w:type="dxa"/></w:tcPr>'
        '<w:p><w:r><w:t xml:space="preserve">' + name + ' r{row}c2</w:t></w:r></w:p></w:tc></w:tr>'
    )
    rows = row.format(row=1) + row.format(row=2)
    return (
        '<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>'
        '<w:p><w:r><w:t xml:space="preserve">' + name + ' before</w:t></w:r></w:p>'
        '<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:vertAnchor="text" w:horzAnchor="margin" w:tblpY="1" w:tblpX="0"/>'
        '<w:tblW w:w="0" w:type="auto"/><w:tblBorders>'
        '<w:top w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:left w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
        '<w:bottom w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
        '<w:insideH w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
        '</w:tblBorders><w:tblLayout w:type="fixed"/><w:tblCellMar>'
        '<w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/>'
        '</w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="' + col + '"/><w:gridCol w:w="' + col + '"/></w:tblGrid>'
        + rows +
        '</w:tbl><w:p><w:r><w:t xml:space="preserve">After table words wrap around the final rows of this floating table and continue on the page.</w:t></w:r></w:p>'
        '<w:sectPr><w:pgSz w:w="11906" w:h="4502"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>'
    )


def write(path: Path, text: str) -> None:
    info = ZipInfo(path.name, date_time=(2026, 11, 9, 11, 39, 0))
    info.compress_type = ZIP_STORED
    with ZipFile(path, "w") as zf:
        zf.writestr("[Content_Types].xml", CONTENT_TYPES)
        zf.writestr(ZipInfo("_rels/.rels"), PACKAGE_RELS)
        zf.writestr(ZipInfo("word/document.xml"), text)
        zf.writestr(ZipInfo("word/_rels/document.xml.rels"), DOCUMENT_RELS)
        zf.writestr(ZipInfo("word/styles.xml"), STYLES)
        zf.writestr(ZipInfo("word/settings.xml"), SETTINGS)


for name, col in FIXTURES:
    write(ROOT / f"{name}.docx", document_xml(name, col))
    print(f"regenerated {name}.docx")


def write_page_fixture(row_height: int, page_height: int, offset: int = 0) -> None:
    suffix = f"x{offset}" if offset else ""
    path = ROOT / f"r{row_height}p{page_height}{suffix}.docx"
    with ZipFile(ROOT / "float7000.docx") as source, ZipFile(path, "w") as dest:
        for item in source.infolist():
            contents = source.read(item.filename).replace(
                b'encoding="UTF-8"?>', b'encoding="UTF-8" standalone="yes"?>'
            )
            if item.filename == "word/document.xml":
                contents = contents.replace(b'w:val="1247"', f'w:val="{row_height}"'.encode())
                contents = contents.replace(b'w:h="4502"', f'w:h="{page_height}"'.encode())
                contents = contents.replace(b'w:tblpX="0"', f'w:tblpX="{offset}"'.encode())
            dest.writestr(item, contents)


for row_height, page_height in [
    (1000, 4502),
    (1100, 4502),
    (1247, 4502),
    (1400, 4502),
    (1247, 5000),
    (1400, 5000),
]:
    write_page_fixture(row_height, page_height)
for row_height, page_height in [
    (1000, 4502),
    (1100, 4502),
    (1247, 4502),
    (1247, 5000),
]:
    write_page_fixture(row_height, page_height, 2500)
for row_height in (1000, 1100, 1247, 1400):
    for page_height in (4600, 4700, 4800, 4900):
        write_page_fixture(row_height, page_height)

for name, tag in [
    ("widow1", '<w:widowControl w:val="1"/>'),
    ("widow0", '<w:widowControl w:val="0"/>'),
    ("keep", "<w:keepLines/>"),
]:
    with ZipFile(ROOT / "float7000.docx") as source, ZipFile(
        ROOT / f"control-{name}.docx", "w"
    ) as dest:
        for item in source.infolist():
            contents = source.read(item.filename).replace(
                b'encoding="UTF-8"?>', b'encoding="UTF-8" standalone="yes"?>'
            )
            if item.filename == "word/document.xml":
                needle = b'<w:p><w:r><w:t xml:space="preserve">After'
                contents = contents.replace(
                    needle,
                    b"<w:p><w:pPr>"
                    + tag.encode()
                    + b'</w:pPr><w:r><w:t xml:space="preserve">After',
                )
            dest.writestr(item, contents)
