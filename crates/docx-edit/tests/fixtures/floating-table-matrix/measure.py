#!/usr/bin/env python3
"""Record Word's position of every token in the floating-table-matrix fixtures.

    ~/tools/word-parity-venv/bin/python measure.py PDF_CACHE_DIR

Exports each fixture with tools/word-parity/word_export.py (cached by content hash in
PDF_CACHE_DIR) and writes word.json: {"source", "cases": {name: {token: [page, y, x0]}}}
with page 0-based, y the baseline and x0 the token's left edge, both in points from the
page's top-left. A token is a letter followed by digits (see generate.py).
"""
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

import pdfplumber

ROOT = Path(__file__).resolve().parent
EXPORT = ROOT.parents[4] / "tools/word-parity/word_export.py"
TOKEN = re.compile(r"[a-z]\d+")
BASELINE_TOL = 0.6


def tokens(pdf_path: Path) -> dict:
    out = {}
    with pdfplumber.open(pdf_path) as pdf:
        for index, page in enumerate(pdf.pages):
            height = float(page.height)
            chars = [c for c in page.chars if c["text"].strip()]
            for c in chars:
                c["base"] = round(height - c["matrix"][5], 2)
            chars.sort(key=lambda c: (c["base"], c["x0"]))
            rows = []
            for c in chars:
                if rows and abs(rows[-1][0]["base"] - c["base"]) <= BASELINE_TOL:
                    rows[-1].append(c)
                else:
                    rows.append([c])
            for row in rows:
                row.sort(key=lambda c: c["x0"])
                text = "".join(c["text"] for c in row)
                for match in TOKEN.finditer(text):
                    first = row[match.start()]
                    if match.group() in out:
                        raise SystemExit(f"{pdf_path}: token {match.group()} seen twice")
                    out[match.group()] = [index, first["base"], round(first["x0"], 2)]
    return out


def main() -> None:
    cache = Path(sys.argv[1]).expanduser()
    cache.mkdir(parents=True, exist_ok=True)
    docs = sorted(ROOT.glob("*.docx"))
    subprocess.run([sys.executable, str(EXPORT), str(cache), *map(str, docs)], check=True)
    cases = {}
    build = None
    for doc in docs:
        digest = hashlib.sha256(doc.read_bytes()).hexdigest()
        meta = json.loads((cache / f"{digest}.json").read_text())
        build = meta.get("word", build)
        cases[doc.stem] = tokens(cache / f"{digest}.pdf")
    (ROOT / "word.json").write_text(
        json.dumps({"source": f"Microsoft Word for Mac {build}, PDF export", "cases": cases}, indent=1)
        + "\n"
    )
    print(f"wrote word.json: {len(cases)} cases")


if __name__ == "__main__":
    main()
