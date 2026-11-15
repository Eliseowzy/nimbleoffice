#!/usr/bin/env python3
"""Tabulate Word's and the engine's line pitch per rule from a run.py output dir.

    analyze.py RUN_DIR DOCS_DIR

Pitch is the baseline gap from the previous line, in pt: `in` within a paragraph (mean
of lines 1 and 2), `para` from the previous paragraph's last line to line 0.
"""
import glob
import hashlib
import json
import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
import compare  # noqa: E402

TAG = re.compile(r"^([ale]\d\d)-([sn])-(\d)")


def pitches(lines):
    by_tag, prev, out = {}, None, {}
    for line in lines:
        m = TAG.match(line["key"])
        if not m:
            continue
        rule, snap, n = m.group(1), m.group(2), int(m.group(3))
        key = (rule, snap)
        if prev and prev["page"] == line["page"]:
            gap = round(line["y"] - prev["y"], 2)
            out.setdefault(key, {"in": [], "para": None})
            if n == 0:
                out[key]["para"] = gap
            else:
                out[key]["in"].append(gap)
        out.setdefault(key, {"in": [], "para": None})
        prev = line
    return {k: (round(sum(v["in"]) / len(v["in"]), 2) if v["in"] else None, v["para"]) for k, v in out.items()}


def main():
    run, docs = sys.argv[1], sys.argv[2]
    rows = []
    for path in sorted(glob.glob(os.path.join(docs, "*.docx"))):
        sha = hashlib.sha256(open(path, "rb").read()).hexdigest()
        word = compare.flatten(json.load(open(os.path.join(run, sha + ".word.json")))["pages"])
        engine_raw = json.load(open(os.path.join(run, sha + ".engine.json")))
        engine = compare.flatten([{"lines": compare.group(p["chars"])} for p in engine_raw["pages"]])
        w, e = pitches(word), pitches(engine)
        name = os.path.basename(path)[:-5]
        for key in w:
            wi, wp = w[key]
            ei, ep = e.get(key, (None, None))
            bad = any(a is not None and b is not None and abs(a - b) > 0.3 for a, b in ((wi, ei), (wp, ep)))
            rows.append((name, *key, wi, ei, wp, ep, "X" if bad else ""))
    print(f"{'doc':16} rule snap  word-in  eng-in  word-para eng-para")
    for r in rows:
        print(f"{r[0]:16} {r[1]}  {r[2]}   {str(r[3]):>7} {str(r[4]):>7}  {str(r[5]):>8} {str(r[6]):>8} {r[7]}")


if __name__ == "__main__":
    main()
