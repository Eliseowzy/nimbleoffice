# nimbleoffice-xlsx-parse

Streaming SpreadsheetML parser and serializer over
[nimbleoffice-xlsx-model](https://crates.io/crates/nimbleoffice-xlsx-model).

`parse_workbook` treats every byte as attacker-controlled: bounded nesting and
counts, and no allocation sized from a value in the file.

| Limit | Value |
| --- | --- |
| `MAX_DEPTH` | 64 |
| `MAX_CELLS` | 10,000,000 per worksheet |
| `MAX_SHARED_STRINGS` | 10,000,000 |
| `MAX_DEFINED_NAME_BYTES` | 32 MiB of name and formula text, plus 64 bytes per name |
| `MAX_HYPERLINKS` | 65,536 per worksheet |
| `MAX_STYLE_ENTRIES` | 65,536 |

Shared formulas expand only for explicitly shared cells, preserving absolute
and mixed references. Expansion is limited to 32 KiB per formula and 32 MiB per
worksheet; missing or invalid shared groups are rejected. Cached values remain
unchanged until recalculation.

`serialize_workbook` regenerates modeled package parts.

Used by [nimbleoffice-xlsx](https://crates.io/crates/nimbleoffice-xlsx).

Part of NimbleOffice. Apache-2.0.
