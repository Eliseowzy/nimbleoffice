from pathlib import Path
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
RUN = '<w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="24"/></w:rPr>'


def build(name, section):
    lines = "".join(f"<w:p><w:r>{RUN}<w:t>L{i:02d}</w:t></w:r></w:p>" for i in range(10))
    wrapped = f"<w:p><w:r>{RUN}<w:t>" + " ".join(f"w{i:03d}" for i in range(60)) + "</w:t></w:r></w:p>"
    right = f'<w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r>{RUN}<w:t>RIGHT</w:t></w:r></w:p>'
    parts = {
        "[Content_Types].xml": '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>',
        "_rels/.rels": f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="d" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>',
        "word/document.xml": f"<w:document {W}><w:body>{lines}{right}{wrapped}{section}</w:body></w:document>",
    }
    with zipfile.ZipFile(Path(__file__).parent / f"{name}.docx", "w", zipfile.ZIP_DEFLATED) as archive:
        for key, value in parts.items():
            info = zipfile.ZipInfo(key, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, value)


build("no-sectpr", "")
build("empty-sectpr", "<w:sectPr/>")
build("pgsz-only", '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>')
