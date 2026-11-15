# Word parity

Measures how far the docx engine's layout is from Microsoft Word's, line by line, over a
corpus, using Word itself as the oracle.

```
python3 tools/word-parity/run.py OUT_DIR corpus/ more.docx
```

For each document `run.py`:

1. exports it to PDF with the local Word (`word_export.py`, cached by content hash in
   `OUT_DIR`, so Word is only needed for new documents);
2. extracts every text line Word laid out: page, baseline, x, size (`word_lines.py`);
3. lays it out with the engine, measuring with **Word's own font files**
   (`fonts.py` → `layout_chars` example), so font metrics are not mistaken for layout;
4. aligns the two by line text and reports page parity, line parity and the first
   divergence (`compare.py`).

A line agrees when it lands on the same page with its baseline within 0.5pt. The first
divergence is the one to fix: what follows is usually its consequence.

## Baseline

| | |
| --- | --- |
| Word | Microsoft Word for Mac 16.114 (build 26110224), Apple silicon |
| Fonts | Word's bundled `Contents/Resources/DFonts` first, then system fonts |
| Tolerances | baseline ±0.5pt; page of every line must match |

Re-export the whole corpus (`rm OUT_DIR/*.pdf`) whenever Word is updated; `word_export.py`
re-exports automatically when the recorded build differs.

## Setup

- `python3 -m venv ~/tools/word-parity-venv && ~/tools/word-parity-venv/bin/pip install pdfplumber fonttools`
  (or point `WORD_PARITY_PYTHON` at another interpreter that has them).
- Word is sandboxed: grant it the staging folder once. The first export shows Word's
  "Grant File Access" dialog; choose `~/Documents/word-parity` (or the folder in
  `WORD_PARITY_STAGE`).

## Known limits

- Bold/italic faces Word synthesises (宋体 bold) fall back to the regular face and are
  listed under `fontFallbacks`.
- Only text is compared; images, borders and shapes are not yet.
- Lines are grouped by baseline, so two table cells on one baseline form one line.
