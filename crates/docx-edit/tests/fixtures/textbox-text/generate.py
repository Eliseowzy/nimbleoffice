from pathlib import Path
import zipfile


W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
CT = "application/vnd.openxmlformats-officedocument.wordprocessingml"
STYLES = f'''<w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:eastAsia="SimSun"/><w:sz w:val="21"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:spacing w:line="300" w:lineRule="auto"/><w:jc w:val="both"/></w:pPr><w:rPr><w:sz w:val="24"/></w:rPr></w:style>
<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont"><w:name w:val="Default Paragraph Font"/></w:style>
<w:style w:type="character" w:styleId="Small"><w:name w:val="Small"/><w:basedOn w:val="DefaultParagraphFont"/><w:rPr><w:sz w:val="18"/></w:rPr></w:style></w:styles>'''
SECTION = '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1797" w:bottom="1440" w:left="1797" w:header="851" w:footer="851"/></w:sectPr>'
SMALL = '<w:sz w:val="18"/>'
STYLED = '<w:rStyle w:val="Small"/>'
TEXT = "根据实际申请学位类别选择博士或硕士学位类别"


def run(text, props):
    return f"<w:r><w:rPr>{props}</w:rPr><w:t>{text}</w:t></w:r>"


def box(paragraph, y, outline=""):
    return f'<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>3200400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>{y}</wp:posOffset></wp:positionV><wp:extent cx="1562100" cy="666750"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="{y // 1000 + 1}" name="b"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1562100" cy="666750"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/>{outline or "<a:ln><a:noFill/></a:ln>"}</wps:spPr><wps:txbx><w:txbxContent>{paragraph}</w:txbxContent></wps:txbx><wps:bodyPr rot="0" vert="horz" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="t"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice></mc:AlternateContent></w:r>'


def document(name, body):
    parts = {
        "[Content_Types].xml": f'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="{CT}.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="{CT}.styles+xml"/></Types>',
        "_rels/.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>',
        "word/_rels/document.xml.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="{REL}/styles" Target="styles.xml"/></Relationships>',
        "word/document.xml": f"<w:document {W}><w:body>{body}{SECTION}</w:body></w:document>",
        "word/styles.xml": STYLES,
    }
    destination = Path(__file__).parent / f"{name}.docx"
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
        for key, value in parts.items():
            archive.writestr(zipfile.ZipInfo(key, date_time=(1980, 1, 1, 0, 0, 0)), value)


def macro(text):
    return f'<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> MACROBUTTON  AcceptAllChangesShown [{text}] </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>'


def main():
    indent = '<w:pPr><w:ind w:firstLine="360"/></w:pPr>'
    indent = '<w:pPr><w:ind w:firstLine="360"/></w:pPr>'
    direct = f"<w:p>{indent}{run('D' + TEXT, SMALL)}</w:p>"
    styled = f"<w:p>{indent}{run('S' + TEXT, STYLED)}</w:p>"
    anchor = f'<w:p>{box(direct, 0)}{box(styled, 1000000)}<w:r><w:t>ANCHOR</w:t></w:r></w:p>'
    document("styles", anchor + f"<w:p>{run('C' + TEXT, STYLED)}</w:p>")
    outline = '<a:ln w="152400"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>'
    plain = f"<w:p>{run(TEXT, SMALL)}</w:p>"
    document("outline", f'<w:p>{box(plain, 0, outline)}<w:r><w:t>ANCHOR</w:t></w:r></w:p>')
    document("macro", f"<w:p>{macro('MACRO TEXT')}</w:p><w:p><w:r><w:t>AFTER</w:t></w:r></w:p>")

if __name__ == "__main__":
    main()
