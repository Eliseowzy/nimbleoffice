#!/usr/bin/env python3
"""Regenerate the floating-table-matrix fixtures.

    python3 generate.py

Each case is a floating table (tblpPr) that the earlier fixture groups do not cover:
margin-anchored splits, multi-column sections, inside/outside alignment, tblpYSpec
placement of a split float, legacy text-anchored splits and a full-width page float
over an inline table (docs/floating-tables.md, section 6). Every word in the document
is a unique token (a letter and digits) so Word's PDF and the engine's display list
can be compared word by word; measure.py records Word's positions in word.json.

styles.xml carries Word's default TableNormal style, as every Word-saved document does:
Word's legacy (compat <= 14) table shift depends on it.

Tokens: a### paragraph before the table, b### paragraph after it, rNNL / sNNL row NN
line L of the float's first / second column, iNNL the inline table of the full-width case.
"""
from pathlib import Path
from zipfile import ZIP_STORED, ZipFile, ZipInfo

ROOT = Path(__file__).resolve().parent
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"""
PACKAGE_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rDoc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"""
DOCUMENT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rSettings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"""
STYLES = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial"/><w:sz w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:uiPriority w:val="99"/><w:semiHidden/><w:unhideWhenUsed/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style></w:styles>"""


def settings(compat: int) -> str:
    return (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{W}"><w:compat>'
        f'<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/>'
        "</w:compat></w:settings>"
    )


def para(prefix: str, count: int) -> str:
    words = " ".join(f"{prefix}{i:03d}" for i in range(1, count + 1))
    return f'<w:p><w:r><w:t xml:space="preserve">{words}</w:t></w:r></w:p>'


BORDERS = "".join(
    f'<w:{side} w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
    for side in ("top", "left", "bottom", "right", "insideH", "insideV")
)


def cell(width: int, prefix: str, row: int, lines: int) -> str:
    paragraphs = "".join(
        f'<w:p><w:r><w:t>{prefix}{row:02d}{line}</w:t></w:r></w:p>' for line in range(1, lines + 1)
    )
    return f'<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/></w:tcPr>{paragraphs}</w:tc>'


def table(tblp: str | None, rows: int, lines: int = 3, col: int = 1700, prefixes=("r", "s")) -> str:
    position = f"<w:tblpPr {tblp}/>" if tblp is not None else ""
    body = "".join(
        "<w:tr>" + "".join(cell(col, p, row, lines) for p in prefixes) + "</w:tr>"
        for row in range(1, rows + 1)
    )
    grid = "".join(f'<w:gridCol w:w="{col}"/>' for _ in prefixes)
    return (
        f"<w:tbl><w:tblPr>{position}<w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders>{BORDERS}</w:tblBorders>"
        '<w:tblLayout w:type="fixed"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/>'
        '<w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr>'
        f"<w:tblGrid>{grid}</w:tblGrid>{body}</w:tbl>"
    )


def sect(cols: int = 1) -> str:
    columns = f'<w:cols w:num="{cols}" w:space="720"/>' if cols > 1 else ""
    return (
        '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
        '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>'
        f"{columns}</w:sectPr>"
    )


def float_attrs(vert: str, horz: str = "margin", y: str = 'w:tblpY="1"', x: str = 'w:tblpX="0"') -> str:
    return (
        'w:leftFromText="180" w:rightFromText="180" w:topFromText="0" w:bottomFromText="0" '
        f'w:vertAnchor="{vert}" w:horzAnchor="{horz}" {x} {y}'
    )


def document(before: int, tbl: str, after: int = 160, cols: int = 1, lead: str = "") -> str:
    head = para("a", before) if before else ""
    return (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body>'
        f"{lead}{head}{tbl}{para('b', after)}{sect(cols)}</w:body></w:document>"
    )


def cases():
    for compat in (14, 15):
        # Margin-anchored split: tblpY from the top margin, with and without text above
        # the float's position (the a### paragraph runs past tblpY=1440).
        yield f"margin-c{compat}", compat, document(4, table(float_attrs("margin", y='w:tblpY="0"'), 20))
        yield f"margin-pretext-c{compat}", compat, document(
            60, table(float_attrs("margin", y='w:tblpY="1440"'), 20)
        )
        # Two-column section: a page float narrower than the column, splitting.
        yield f"cols2-c{compat}", compat, document(
            4, table(float_attrs("page", y='w:tblpY="1800"'), 30, col=1500, prefixes=("r",)), cols=2
        )
        # Outside alignment flips with page parity; the float spans three pages.
        yield f"outside-c{compat}", compat, document(
            4, table(float_attrs("page", x='w:tblpXSpec="outside"', y='w:tblpY="1800"'), 30)
        )
        # tblpYSpec placement of a float taller than the room below it.
        yield f"ybottom-c{compat}", compat, document(
            4, table(float_attrs("page", y='w:tblpYSpec="bottom"'), 20)
        )
        yield f"ycenter-c{compat}", compat, document(
            4, table(float_attrs("page", y='w:tblpYSpec="center"'), 20)
        )
        # Full-width page float over an inline table that leaves no room below it.
        inline = table(None, 20, lines=2, col=4680, prefixes=("i",))
        yield f"fullwidth-over-inline-c{compat}", compat, document(
            0,
            table(float_attrs("page", y='w:tblpY="3000"'), 4, lines=2, col=4680, prefixes=("r",)),
            after=60,
            lead=inline,
        )
    # Legacy text-anchored split, with text before it (floating-table-split is all compat 15).
    yield "text-pretext-c14", 14, document(30, table(float_attrs("text"), 20))
    yield "text-pretext-c15", 15, document(30, table(float_attrs("text"), 20))


def write(path: Path, compat: int, text: str) -> None:
    def entry(name: str) -> ZipInfo:
        info = ZipInfo(name, date_time=(2026, 11, 13, 0, 0, 0))
        info.compress_type = ZIP_STORED
        return info

    with ZipFile(path, "w") as zf:
        zf.writestr(entry("[Content_Types].xml"), CONTENT_TYPES)
        zf.writestr(entry("_rels/.rels"), PACKAGE_RELS)
        zf.writestr(entry("word/document.xml"), text)
        zf.writestr(entry("word/_rels/document.xml.rels"), DOCUMENT_RELS)
        zf.writestr(entry("word/styles.xml"), STYLES)
        zf.writestr(entry("word/settings.xml"), settings(compat))


if __name__ == "__main__":
    for name, compat, text in cases():
        write(ROOT / f"{name}.docx", compat, text)
        print(f"regenerated {name}.docx")
