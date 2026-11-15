#!/usr/bin/env python3
"""Per-document comparison of two word-parity runs.

    compare_runs.py BASE_RUN NEW_RUN

Prints documents whose lineParity or pageParity got worse (regressions),
got better, and totals. Exit code 1 if any regression is found.
"""
import json
import os
import sys

RUNS = os.path.expanduser("~/word-parity-data/runs")


def load(run):
    with open(os.path.join(RUNS, run, "summary.json")) as f:
        return {d["sha"]: d for d in json.load(f)["documents"] if "sha" in d}


def main():
    base_name, new_name = sys.argv[1], sys.argv[2]
    base, new = load(base_name), load(new_name)
    worse, better, same, missing = [], [], [], []
    for sha, b in base.items():
        if "error" in b:
            continue
        n = new.get(sha)
        if n is None or "error" in n:
            missing.append(os.path.basename(b["doc"]))
            continue
        if n["lineParity"] < b["lineParity"] or (not n["pageParity"] and b["pageParity"]):
            worse.append((os.path.basename(b["doc"]), b["lineParity"], n["lineParity"],
                          b["pageParity"], n["pageParity"]))
        elif n["lineParity"] > b["lineParity"] or (n["pageParity"] and not b["pageParity"]):
            better.append((os.path.basename(b["doc"]), b["lineParity"], n["lineParity"]))
        else:
            same.append(sha)

    with open(os.path.join(RUNS, base_name, "summary.json")) as f:
        bt = json.load(f)["totals"]
    with open(os.path.join(RUNS, new_name, "summary.json")) as f:
        nt = json.load(f)["totals"]
    print(f"base {base_name}: {bt['documents']} docs, {bt['errors']} errors, "
          f"pages {bt['pageParity']:.2%}, lines {bt['lineParity']:.2%}")
    print(f"new  {new_name}: {nt['documents']} docs, {nt['errors']} errors, "
          f"pages {nt['pageParity']:.2%}, lines {nt['lineParity']:.2%}")
    print(f"\nbetter: {len(better)}  same: {len(same)}  worse: {len(worse)}  "
          f"missing-in-new: {len(missing)}")
    if better:
        print("\nimprovements:")
        for name, b, n in sorted(better, key=lambda x: x[2] - x[1], reverse=True):
            print(f"  {name}: {b:.2%} -> {n:.2%}")
    if worse:
        print("\nREGRESSIONS:")
        for name, bl, nl, bp, np_ in sorted(worse):
            print(f"  {name}: lines {bl:.2%} -> {nl:.2%}, pages {bp} -> {np_}")
    if missing:
        print("\nmissing in new run:")
        for name in missing:
            print(f"  {name}")
    sys.exit(1 if worse or missing else 0)


if __name__ == "__main__":
    main()
