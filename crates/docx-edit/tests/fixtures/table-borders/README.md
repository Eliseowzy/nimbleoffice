Run `python3 generate.py` to rebuild these synthetic DOCX fixtures from `tools/word-parity/experiments/table_borders/generate.py`. Each is Arial 12 pt, single spacing, a paragraph, a two-column table and a paragraph; each changes one variable: border size, cell margins, compatibility mode, which of top, insideH and bottom is set, `tblCellSpacing`, or an exact row height.

`word.json` records each line's baseline in points from the page top in Microsoft Word 16.114's PDF export, extracted with `word_lines.py`.
