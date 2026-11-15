from pathlib import Path
from xml.sax.saxutils import escape
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
SIZES = range(8, 27)


def run(text, size, align=""):
    vert = f'<w:vertAlign w:val="{align}"/>' if align else ""
    return f'<w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="{size * 2}"/>{vert}</w:rPr><w:t>{escape(text)}</w:t></w:r>'


def main():
    body = "".join(
        f"<w:p>{run(f'S{size}x', size)}{run('1', size, 'superscript')}{run('2', size, 'subscript')}</w:p>"
        for size in SIZES
    )
    section = '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>'
    parts = {
        "[Content_Types].xml": '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>',
        "_rels/.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>',
        "word/document.xml": f"<w:document {W}><w:body>{body}{section}</w:body></w:document>",
    }
    with zipfile.ZipFile(Path(__file__).parent / "script.docx", "w", zipfile.ZIP_DEFLATED) as archive:
        for key, value in parts.items():
            info = zipfile.ZipInfo(key, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, value)


main()
