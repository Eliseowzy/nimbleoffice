#!/usr/bin/env python3
"""Documents probing line height rules on a document grid.

    generate.py OUT_DIR

One document per grid pitch and font size; each holds one three-line paragraph per
line rule, with snapToGrid on and off. Every line starts with a tag
`<rule>-<snap>-<line>` so compare.py can align it.
"""
from pathlib import Path
import sys
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PITCHES = [312, 360]
SIZES = [10.5, 12, 14, 16]
RULES = [
    ("a08", 'w:line="192" w:lineRule="auto"'),
    ("a10", 'w:line="240" w:lineRule="auto"'),
    ("a15", 'w:line="360" w:lineRule="auto"'),
    ("a20", 'w:line="480" w:lineRule="auto"'),
    ("l00", 'w:line="0" w:lineRule="atLeast"'),
    ("l12", 'w:line="240" w:lineRule="atLeast"'),
    ("l20", 'w:line="400" w:lineRule="atLeast"'),
    ("l30", 'w:line="600" w:lineRule="atLeast"'),
    ("e12", 'w:line="240" w:lineRule="exact"'),
]
TEXT = "中文行距网格测试"


def paragraph(tag, spacing, snap):
    snap_xml = "" if snap else '<w:snapToGrid w:val="0"/>'
    runs = []
    for line in range(3):
        if line:
            runs.append("<w:r><w:br/></w:r>")
        runs.append(f"<w:r><w:t xml:space=\"preserve\">{tag}-{'s' if snap else 'n'}-{line} {TEXT}</w:t></w:r>")
    return f'<w:p><w:pPr>{snap_xml}<w:spacing w:before="0" w:after="0" {spacing}/></w:pPr>{"".join(runs)}</w:p>'


def write(path, pitch, size):
    half = round(size * 2)
    styles = (
        f'<w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr>'
        f'<w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:eastAsia="宋体" w:cs="Times New Roman"/>'
        f'<w:sz w:val="{half}"/><w:szCs w:val="{half}"/><w:lang w:val="en-US" w:eastAsia="zh-CN"/>'
        f'</w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:widowControl w:val="0"/></w:pPr></w:pPrDefault></w:docDefaults>'
        f'<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>'
    )
    body = "".join(paragraph(tag, spacing, snap) for tag, spacing in RULES for snap in (True, False))
    section = (
        '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>'
        '<w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800" w:header="851" w:footer="992" w:gutter="0"/>'
        f'<w:docGrid w:type="lines" w:linePitch="{pitch}"/></w:sectPr>'
    )
    parts = {
        "[Content_Types].xml": '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>',
        "_rels/.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>',
        "word/_rels/document.xml.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="{REL}/styles" Target="styles.xml"/><Relationship Id="settings" Type="{REL}/settings" Target="settings.xml"/></Relationships>',
        "word/document.xml": f'<w:document {W}><w:body>{body}{section}</w:body></w:document>',
        "word/styles.xml": styles,
        "word/settings.xml": f'<w:settings {W}><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>',
    }
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        for key, value in parts.items():
            archive.writestr(zipfile.ZipInfo(key, date_time=(1980, 1, 1, 0, 0, 0)), value, zipfile.ZIP_DEFLATED)


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    out.mkdir(parents=True, exist_ok=True)
    for pitch in PITCHES:
        for size in SIZES:
            write(out / f"grid{pitch}-{str(size).replace('.', '_')}pt.docx", pitch, size)


if __name__ == "__main__":
    main()
