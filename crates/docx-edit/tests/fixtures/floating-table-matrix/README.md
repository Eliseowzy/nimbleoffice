# Floating table matrix

Floating-table combinations the other groups leave uncovered (docs/floating-tables.md,
section 6). Letter pages, 1in margins, Arial 12, single spacing; tables of two 85pt
columns with three one-line paragraphs per cell. Every word is a unique token, so
`floating_table_matrix.rs` compares Word and the engine word by word.

```
python3 generate.py                                        # rewrite the .docx files
~/tools/word-parity-venv/bin/python measure.py ~/word-parity-data/cache/floating-table-matrix
```

`measure.py` exports with the local Word (tools/word-parity) and rewrites `word.json`.

| case | float | what Word 16.114 does |
| --- | --- | --- |
| `margin-c14/15` | `vertAnchor=margin`, `tblpY=0`, 20 rows | c14: splits from the top margin, the next paragraph wraps beside the last fragment. c15: the whole float moves to page 2 and splits from there |
| `margin-pretext-c14/15` | margin, `tblpY=1440`, a long paragraph first | splits on page 1 at 156pt; c14 puts cell text at x=72 (legacy shift), c15 at 77.76. Every fixture carries the default TableNormal style, which the legacy shift depends on |
| `cols2-c14/15` | page, `tblpY=1800`, two-column section, 30 one-column rows | **does not split**: the float is moved up so it ends at the page bottom, and its first 33 tokens are clipped above the page |
| `outside-c14/15` | page, `tblpXSpec=outside`, 30 rows | right-aligned at the margin on page 1 *and* page 2: the continuation keeps the first page's side |
| `ybottom-c14/15` | page, `tblpYSpec=bottom`, 20 rows | the float starts at 174.72pt (bottom-aligned less the overflow), c14 runs past the footer to 789pt, c15 splits at the text bottom (705pt) |
| `ycenter-c14/15` | page, `tblpYSpec=center`, 20 rows | c14: keeps page 1 at 93pt and the paragraph *before* it wraps beside it. c15: moves to page 2 |
| `fullwidth-over-inline-c14/15` | full-width page float, `tblpY=3000`, over a 20-row inline table | c14: the float splits (6 rows on page 1, 2 on page 2), text wraps beside. c15: the float moves to page 2 (rule B1) |
| `text-pretext-c14/15` | text-anchored, 30 words first | splits on page 1 |
