#!/usr/bin/env python3
"""Run the Word-parity comparison over a corpus and summarise it.

    run.py OUT_DIR DOCX_OR_DIR [...]

For every .docx: export it with Word (cached by content hash), extract Word's lines, lay
it out with the engine using Word's own fonts, and compare. Writes OUT_DIR/<sha>.cmp.json
per document and OUT_DIR/summary.json, and prints the documents ranked by how early they
diverge from Word. Word itself is needed only for documents not yet in the cache.
"""
import glob
import hashlib
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
VENV_PY = os.environ.get("WORD_PARITY_PYTHON", os.path.expanduser("~/tools/word-parity-venv/bin/python"))

sys.path.insert(0, HERE)
import compare  # noqa: E402


def docx_files(args):
    for arg in args:
        if os.path.isdir(arg):
            yield from sorted(p for p in glob.glob(os.path.join(arg, "**", "*.docx"), recursive=True)
                              if not os.path.basename(p).startswith("~$"))
        else:
            yield arg


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, **kw)


def main() -> None:
    out, inputs = sys.argv[1], list(docx_files(sys.argv[2:]))
    os.makedirs(out, exist_ok=True)
    fonts = os.path.join(out, "fonts.json")
    if not os.path.exists(fonts):
        run([VENV_PY, "-I", os.path.join(HERE, "fonts.py"), fonts])
    # WORD_PARITY_NO_EXPORT=1 compares only what is already cached, without Word.
    if os.environ.get("WORD_PARITY_NO_EXPORT") != "1":
        run([sys.executable, "-I", os.path.join(HERE, "word_export.py"), out, *inputs])
    run(["cargo", "build", "-q", "-p", "nimbleoffice-docx-edit", "--example", "layout_chars"], cwd=ROOT)
    engine_bin = os.path.join(ROOT, "target", "debug", "examples", "layout_chars")

    summary = []
    for path in inputs:
        sha = hashlib.sha256(open(path, "rb").read()).hexdigest()
        pdf = os.path.join(out, sha + ".pdf")
        if not os.path.exists(pdf):
            summary.append({"doc": path, "error": "no Word export"})
            continue
        word_json = os.path.join(out, sha + ".word.json")
        if not os.path.exists(word_json):
            with open(word_json, "w") as f:
                run([VENV_PY, "-I", os.path.join(HERE, "word_lines.py"), pdf], stdout=f)
        engine_json = os.path.join(out, sha + ".engine.json")
        with open(engine_json, "w") as f:
            try:
                proc = subprocess.run([engine_bin, path, fonts], stdout=f, stderr=subprocess.PIPE, text=True,
                                      timeout=300)
            except subprocess.TimeoutExpired:
                summary.append({"doc": path, "error": "engine: timed out"})
                continue
        if proc.returncode != 0:
            summary.append({"doc": path, "error": "engine: " + proc.stderr.strip()[-300:]})
            continue
        result = compare.compare(json.load(open(word_json)), json.load(open(engine_json)))
        json.dump(result, open(os.path.join(out, sha + ".cmp.json"), "w"), ensure_ascii=False, indent=1)
        first = result["first"]
        where = None
        if first:
            line = first["word"] if first["kind"] == "position" else (first["word"] or first["engine"] or [{}])[0]
            where = {"kind": first["kind"], "page": line.get("page", -1) + 1, "text": line.get("text", "")[:30],
                     "dy": first.get("dy")}
        summary.append({"doc": path, "sha": sha, "pages": result["pages"], "pageParity": result["pageParity"],
                        "lineParity": result["lines"]["lineParity"], "first": where,
                        "fontFallbacks": result["fontFallbacks"]})

    ok = [s for s in summary if "error" not in s]
    totals = {
        "documents": len(summary),
        "errors": len(summary) - len(ok),
        "pageParity": round(sum(s["pageParity"] for s in ok) / max(len(ok), 1), 4),
        "lineParity": round(sum(s["lineParity"] for s in ok) / max(len(ok), 1), 4),
    }
    json.dump({"totals": totals, "documents": summary}, open(os.path.join(out, "summary.json"), "w"),
              ensure_ascii=False, indent=1)
    print(f"\n{totals['documents']} docs, {totals['errors']} errors, page parity {totals['pageParity']:.1%}, "
          f"mean line parity {totals['lineParity']:.1%}")
    for s in sorted(ok, key=lambda s: (s["pageParity"], s["lineParity"])):
        f = s["first"]
        where = "identical" if not f else f"{f['kind']} p{f['page']} 「{f['text']}」" + (f" dy={f['dy']}" if f["dy"] is not None else "")
        print(f"{s['lineParity']:6.1%} pages {s['pages']['word']}/{s['pages']['engine']}  {os.path.basename(s['doc'])}: {where}")
    for s in summary:
        if "error" in s:
            print(f"ERROR {s['doc']}: {s['error']}")


if __name__ == "__main__":
    main()
