#!/usr/bin/env python3
"""Index the font files Word can use, keyed the way the layout engine asks for faces.

    fonts.py OUT.json

Keys are "<family lowercase>|<bold 0/1>|<italic 0/1>", matching the engine's font-chain
keys. Every localized family name in a face's name table is indexed (宋体 and SimSun
both reach Simsun.ttc). Word's own bundled fonts win over system fonts, as in Word.
Collection faces are extracted to standalone .ttf files under CACHE, since the engine
reads only the first face of a file.
"""
import glob
import json
import os
import sys

from fontTools.ttLib import TTCollection, TTFont

WORD_FONTS = "/Applications/Microsoft Word.app/Contents/Resources/DFonts"
SYSTEM_DIRS = ["/System/Library/Fonts", "/System/Library/Fonts/Supplemental",
               "/Library/Fonts", os.path.expanduser("~/Library/Fonts")]
CACHE = os.path.expanduser("~/.cache/word-parity/faces")


def names(font, ids):
    out = set()
    for rec in font["name"].names:
        if rec.nameID in ids:
            try:
                out.add(rec.toUnicode().strip())
            except UnicodeDecodeError:
                pass
    return {n for n in out if n}


def style(font):
    os2 = font["OS/2"] if "OS/2" in font else None
    bold = bool(os2 and (os2.fsSelection & 0x20 or os2.usWeightClass >= 600))
    italic = bool(os2 and os2.fsSelection & 0x01)
    if not os2:
        sub = " ".join(names(font, {2})).lower()
        bold, italic = "bold" in sub, "italic" in sub or "oblique" in sub
    return int(bold), int(italic)


def faces(path):
    lower = path.lower()
    if lower.endswith((".ttc", ".otc")):
        os.makedirs(CACHE, exist_ok=True)
        for i, font in enumerate(TTCollection(path).fonts):
            out = os.path.join(CACHE, f"{os.path.basename(path)}.{i}.ttf")
            if not os.path.exists(out):
                font.save(out)
            yield out, font
    elif lower.endswith((".ttf", ".otf")):
        yield path, TTFont(path, lazy=True)


def main() -> None:
    index = {}
    for directory in [WORD_FONTS] + SYSTEM_DIRS:
        for path in sorted(glob.glob(os.path.join(directory, "*"))):
            try:
                for face_path, font in faces(path):
                    if "name" not in font:
                        continue
                    b, i = style(font)
                    for family in names(font, {1, 16}):
                        key = f"{family.lower()}|{b}|{i}"
                        index.setdefault(key, face_path)
            except Exception as e:  # unreadable or unsupported file: skip it
                print(f"skip {path}: {e}", file=sys.stderr)
    json.dump(index, open(sys.argv[1], "w"), ensure_ascii=False, indent=0, sort_keys=True)
    print(f"{len(index)} keys", file=sys.stderr)


if __name__ == "__main__":
    main()
