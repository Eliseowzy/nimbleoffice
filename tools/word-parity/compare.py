#!/usr/bin/env python3
"""Compare the engine's layout of a document with Word's, line by line.

    compare.py WORD_LINES.json ENGINE_CHARS.json [--json OUT.json]

WORD_LINES.json comes from word_lines.py; ENGINE_CHARS.json from the layout_chars example.
Engine characters are grouped into lines the same way word_lines.py groups the PDF's.
Lines are aligned by their whitespace-free text (difflib's matching blocks), so a line
broken differently shows up as an unmatched pair at the exact place it diverges.

A matched line agrees when it lands on the same page with its baseline within
BASELINE_TOL_PT. The first disagreement is what to fix first: what follows it is usually
its consequence.
"""
import argparse
import difflib
import unicodedata
import json
import sys

BASELINE_TOL_PT = 0.5
GROUP_TOL = 0.6
GAP_EM = 1.5


def group(chars):
    """Characters (dicts with t, x, y, s, optional w) into lines sorted top to bottom."""
    chars = sorted(chars, key=lambda c: (c["y"], c["x"]))
    rows = []
    for c in chars:
        if rows and abs(rows[-1]["y"] - c["y"]) <= GROUP_TOL:
            rows[-1]["chars"].append(c)
        else:
            rows.append({"y": c["y"], "chars": [c]})
    lines = []
    for row in rows:
        cs = sorted(row["chars"], key=lambda c: c["x"])
        lines.append({"y": round(row["y"], 2), "x0": round(cs[0]["x"], 2),
                      "size": round(max(c.get("s", 0) for c in cs), 2),
                      "text": "".join(c["t"] for c in cs)})
    return lines


def flatten(pages):
    out = []
    for index, page in enumerate(pages):
        for line in page["lines"]:
            key = unicodedata.normalize("NFKC", "".join(line["text"].split()))
            if key:
                out.append({**line, "page": index, "key": key})
    return out


def compare(word, engine):
    word_lines = flatten(word["pages"])
    engine_pages = [{"lines": group(p["chars"])} for p in engine["pages"]]
    engine_lines = flatten(engine_pages)
    matcher = difflib.SequenceMatcher(a=[l["key"] for l in word_lines],
                                      b=[l["key"] for l in engine_lines], autojunk=False)
    pairs, first = [], None
    matched = agree = 0
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            for i, j in zip(range(i1, i2), range(j1, j2)):
                w, e = word_lines[i], engine_lines[j]
                dy = round(e["y"] - w["y"], 2)
                ok = w["page"] == e["page"] and abs(dy) <= BASELINE_TOL_PT
                matched += 1
                agree += ok
                if not ok and first is None:
                    first = {"kind": "position", "word": w, "engine": e, "dy": dy}
                pairs.append({"ok": ok, "word": w, "engine": e, "dy": dy})
        else:
            if first is None:
                first = {"kind": "break", "word": word_lines[i1:i2][:3], "engine": engine_lines[j1:j2][:3]}
            pairs.append({"ok": False, "word": word_lines[i1:i2], "engine": engine_lines[j1:j2], "tag": tag})
    total = max(len(word_lines), 1)
    return {
        "pages": {"word": len(word["pages"]), "engine": len(engine["pages"])},
        "lines": {"word": len(word_lines), "engine": len(engine_lines), "matched": matched,
                  "agree": agree, "lineParity": round(agree / total, 4)},
        "pageParity": len(word["pages"]) == len(engine["pages"]),
        "fontFallbacks": engine.get("fontFallbacks", {}),
        "first": first,
        "pairs": pairs,
    }


def describe(line):
    return f"p{line['page'] + 1} y={line['y']} 「{line['text'][:24]}」"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("word")
    ap.add_argument("engine")
    ap.add_argument("--json")
    args = ap.parse_args()
    result = compare(json.load(open(args.word)), json.load(open(args.engine)))
    if args.json:
        json.dump(result, open(args.json, "w"), ensure_ascii=False, indent=1)
    p, l = result["pages"], result["lines"]
    print(f"pages word={p['word']} engine={p['engine']}  lines agree {l['agree']}/{l['word']} "
          f"({l['lineParity']:.1%})  fallbacks={result['fontFallbacks'] or '-'}")
    first = result["first"]
    if first is None:
        print("no divergence")
    elif first["kind"] == "position":
        print(f"first divergence (position, dy={first['dy']}pt):\n  word   {describe(first['word'])}\n  engine {describe(first['engine'])}")
    else:
        print("first divergence (line break / content):")
        for w in first["word"]:
            print(f"  word   {describe(w)}")
        for e in first["engine"]:
            print(f"  engine {describe(e)}")


if __name__ == "__main__":
    sys.exit(main())
