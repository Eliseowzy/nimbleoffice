# nimbleoffice-docx-parse

WordprocessingML parsing: the typed DOCX model the rest of the DOCX engine is
built on. Bounded XML and relationship parsing sit on top of
[nimbleoffice-opc](https://crates.io/crates/nimbleoffice-opc)'s zip trust
boundary, so this crate can treat part bytes as already length- and path-checked
while still validating everything inside them.

The model covers the body block and inline trees, paragraph and run formatting,
tables and borders, styles, numbering, fonts, themes, headers and footers,
footnotes and endnotes, comments, media, images, charts, and the typed
relationship graph.

`canonical` freezes the cross-language canonical contract (the same shape the
TypeScript packages consume) so the Rust and wasm paths cannot drift.

Used by [nimbleoffice-docx](https://crates.io/crates/nimbleoffice-docx).

Part of NimbleOffice. Apache-2.0.
