#!/usr/bin/env python3
"""Extract the text lines of a PDF page by page, as Word laid them out.

    word_lines.py FILE.pdf > lines.json

Characters sharing a baseline (within BASELINE_TOL pt) form one line; a horizontal gap
wider than GAP_EM times the font size splits it into segments, so table cells and tab
runs on one baseline stay apart. Coordinates are points from the page's top-left.

Output: {"pages": [{"width", "height", "lines": [{"y", "x0", "x1", "size", "text",
"segments": [{"x0", "x1", "text"}]}]}]}, lines sorted top to bottom.
"""
import json
import sys

import pdfplumber

BASELINE_TOL = 0.6
GAP_EM = 1.5


def baseline(ch, height):
    # matrix[5] is the text origin's y in PDF space (bottom-up)
    return round(height - ch["matrix"][5], 2)


def page_lines(page):
    height = float(page.height)
    chars = [c for c in page.chars if c["text"].strip()]
    chars.sort(key=lambda c: (baseline(c, height), c["x0"]))
    rows = []
    for ch in chars:
        y = baseline(ch, height)
        if rows and abs(rows[-1]["y"] - y) <= BASELINE_TOL:
            rows[-1]["chars"].append(ch)
        else:
            rows.append({"y": y, "chars": [ch]})
    lines = []
    for row in rows:
        cs = sorted(row["chars"], key=lambda c: c["x0"])
        size = max(c["size"] for c in cs)
        segments = []
        for c in cs:
            if segments and c["x0"] - segments[-1]["x1"] <= GAP_EM * size:
                seg = segments[-1]
                seg["text"] += c["text"]
                seg["x1"] = max(seg["x1"], c["x1"])
            else:
                segments.append({"x0": c["x0"], "x1": c["x1"], "text": c["text"]})
        for seg in segments:
            seg["x0"], seg["x1"] = round(seg["x0"], 2), round(seg["x1"], 2)
        lines.append({
            "y": row["y"],
            "x0": segments[0]["x0"],
            "x1": segments[-1]["x1"],
            "size": round(size, 2),
            "text": "".join(s["text"] for s in segments),
            "segments": segments,
        })
    return {"width": round(float(page.width), 2), "height": round(height, 2), "lines": lines}


def main() -> None:
    with pdfplumber.open(sys.argv[1]) as pdf:
        out = {"pages": [page_lines(p) for p in pdf.pages]}
    json.dump(out, sys.stdout, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
