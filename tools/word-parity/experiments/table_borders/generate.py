"""Build one-variable table-border experiments for the Word oracle.

    python3 generate.py OUT_DIR
"""
import sys
import zipfile
from pathlib import Path

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
CT = "application/vnd.openxmlformats-officedocument.wordprocessingml"
EDGES = ("top", "left", "bottom", "right", "insideH", "insideV")


def paragraph(text):
    return f'<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>'


def borders(sizes):
    inner = "".join(f'<w:{edge} w:val="single" w:sz="{size}" w:space="0" w:color="000000"/>'
                    if size else f'<w:{edge} w:val="nil"/>' for edge, size in sizes.items())
    return f"<w:tblBorders>{inner}</w:tblBorders>"


def document(name, sizes, margin=0, spacing=0, mode=15, rows=4, height=0, top=None, bottom=None):
    mar = "".join(f'<w:{edge} w:w="{value}" w:type="dxa"/>'
                  for edge, value in (("top", margin if top is None else top), ("left", 108),
                                       ("bottom", margin if bottom is None else bottom), ("right", 108)))
    gap = f'<w:tblCellSpacing w:w="{spacing}" w:type="dxa"/>' if spacing else ""
    body = [paragraph(f"{name} before")]
    body.append(f'<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/>{gap}{borders(sizes)}'
                f'<w:tblLayout w:type="fixed"/><w:tblCellMar>{mar}</w:tblCellMar></w:tblPr>'
                '<w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid>')
    tr_pr = f'<w:trPr><w:trHeight w:val="{height}" w:hRule="exact"/></w:trPr>' if height else ""
    for row in range(1, rows + 1):
        body.append(f'<w:tr>{tr_pr}' + ''.join(
            f'<w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr>{paragraph(f"{name} r{row}c{col}")}</w:tc>'
            for col in (1, 2)) + '</w:tr>')
    body.append('</w:tbl>')
    body.append(paragraph(f"{name} after"))
    sect = ('<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" '
            'w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>')
    styles = (f'<w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr>'
              '<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial"/><w:sz w:val="24"/></w:rPr></w:rPrDefault>'
              '<w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>'
              '</w:pPr></w:pPrDefault></w:docDefaults>'
              '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
              '</w:styles>')
    settings = (f'<w:settings {W}><w:compat><w:compatSetting w:name="compatibilityMode" '
                f'w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat></w:settings>')
    return {
        '[Content_Types].xml': '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        + ''.join(f'<Override PartName="/word/{part}.xml" ContentType="{CT}.{kind}+xml"/>'
                  for part, kind in [('document', 'document.main'), ('styles', 'styles'), ('settings', 'settings')])
        + '</Types>',
        '_rels/.rels': '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        f'<Relationship Id="rDoc" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>',
        'word/_rels/document.xml.rels': '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        f'<Relationship Id="rStyles" Type="{REL}/styles" Target="styles.xml"/>'
        f'<Relationship Id="rSettings" Type="{REL}/settings" Target="settings.xml"/></Relationships>',
        'word/document.xml': f'<w:document {W}><w:body>{"".join(body)}{sect}</w:body></w:document>',
        'word/styles.xml': styles,
        'word/settings.xml': settings,
    }


def write(directory, name, **kw):
    directory.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(directory / f"{name}.docx", "w") as archive:
        for key, value in document(name, **kw).items():
            info = zipfile.ZipInfo(key, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' + value)


def uniform(size):
    return {edge: size for edge in EDGES}


def cases():
    for size in (0, 2, 4, 8, 12, 24, 48):
        yield f"sz{size}", {"sizes": uniform(size)}
    for size in (4, 24):
        for margin in (40, 100):
            yield f"sz{size}-mar{margin}", {"sizes": uniform(size), "margin": margin}
        yield f"sz{size}-gap", {"sizes": uniform(size), "spacing": 40}
        yield f"sz{size}-compat14", {"sizes": uniform(size), "mode": 14}
    yield "top24", {"sizes": {**uniform(0), "top": 24}}
    yield "insideh24", {"sizes": {**uniform(0), "insideH": 24}}
    yield "bottom24", {"sizes": {**uniform(0), "bottom": 24}}
    yield "top24-inside4", {"sizes": {**uniform(4), "top": 24}}
    yield "top4-inside24", {"sizes": {**uniform(24), "top": 4}}
    for gap in (20, 40, 100):
        for size in (0, 4, 24):
            yield f"gap{gap}-sz{size}", {"sizes": uniform(size), "spacing": gap}
    yield "gap40-sz4-mar40", {"sizes": uniform(4), "spacing": 40, "margin": 40}
    yield "gap40-sz4-compat14", {"sizes": uniform(4), "spacing": 40, "mode": 14}
    yield "gap40-top24", {"sizes": {**uniform(0), "top": 24}, "spacing": 40}
    yield "gap40-insideh24", {"sizes": {**uniform(0), "insideH": 24}, "spacing": 40}
    for height, size in ((850, 0), (850, 4), (850, 24), (600, 4), (437, 4)):
        yield f"exact{height}-sz{size}", {"sizes": uniform(size), "height": height, "rows": 12}
    yield "exact850-sz4-mar40", {"sizes": uniform(4), "height": 850, "rows": 12, "margin": 40}
    yield "exact850-sz4-gap40", {"sizes": uniform(4), "height": 850, "rows": 12, "spacing": 40}
    yield "exact850-sz4-top100", {"sizes": uniform(4), "height": 850, "rows": 12, "top": 100}
    yield "exact850-sz4-bottom100", {"sizes": uniform(4), "height": 850, "rows": 12, "bottom": 100}
    yield "exact850-sz0-top100", {"sizes": uniform(0), "height": 850, "rows": 12, "top": 100}
    for gap in (20, 100):
        yield f"exact850-sz0-gap{gap}", {"sizes": uniform(0), "height": 850, "rows": 12, "spacing": gap}
    yield "exact850-sz24-gap40", {"sizes": uniform(24), "height": 850, "rows": 12, "spacing": 40}


if __name__ == "__main__":
    out = Path(sys.argv[1])
    for name, kw in cases():
        write(out, name, **kw)
