# Table split bands

Floating tables (`tblpPr vertAnchor="page"`, `tblpY=1700`, sz36 borders, exact 16pt lines)
that split across pages. Word 16.114 PDF exports measured the positions pinned in
`table_split_bands.rs`.

| fixture | content | Word |
| --- | --- | --- |
| `continuation-compat14.docx` | one 57-line row + a short row, compat 14, no text before the table | page 1 ends with line 41 (baseline 742.32pt, past the 727.2pt text limit: the legacy deadline is min(footer line, top + body height) = 754.6pt); page 2 resumes line 42 at 74.88pt, one band below the top margin |
| `rows-compat14.docx` | 25 two-line rows, compat 14, no text before the table | page 1 ends with row 18 (second baseline 738.72pt); page 2 starts row 19 at 74.88pt |
| `continuation-compat15.docx` | the continuation table under the title, compat 15 | the float moves to page 2 at 85pt (line 01 at 102.24pt), line 39 ends page 2 at 710.40pt, line 40 opens page 3 at 74.88pt |
| `continuation-compat15-clear.docx` | the same with one short line above the float | the float stays on page 1 (line 01 at 102.24pt) |
| `continuation-clipping.docx`, `page-bottom-boundary.docx` | the same tables after a title and a paragraph | only the paint invariant: no border or cut line crosses a text line |

Text before a page-anchored float wraps beside it in Word; the engine does not yet, so the
last two are not position-checked.
