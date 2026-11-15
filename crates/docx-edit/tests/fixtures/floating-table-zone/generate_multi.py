from pathlib import Path
from zipfile import ZipFile

root = Path(__file__).resolve().parent
text = " ".join(f"wrapword{i:02d}" for i in range(1, 41))
for split in (4, 12, 20, 28):
    left = " ".join(f"wrapword{i:02d}" for i in range(1, split + 1))
    right = " ".join(f"wrapword{i:02d}" for i in range(split + 1, 41))
    replacement = (
        left + '</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">' + right
    )
    with ZipFile(root / "zone-sz4.docx") as source, ZipFile(
        root / f"multi-s{split}.docx", "w"
    ) as dest:
        for item in source.infolist():
            contents = source.read(item.filename)
            if item.filename == "word/document.xml":
                contents = contents.replace(text.encode(), replacement.encode())
            dest.writestr(item, contents)
