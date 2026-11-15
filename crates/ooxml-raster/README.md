# nimbleoffice-ooxml-raster

Shared tiny-skia helpers for the NimbleOffice raster backends: PNG encoding,
hex and CSS color parsing, DrawingML geometry and glyph outline paths, RGBA
premultiplication and clip rectangles clamped to the target surface.

```rust
use ooxml_raster::{encode_png, parse_css_color};

let color = parse_css_color("rgba(0, 0, 0, 0.5)")?;
let png: Vec<u8> = encode_png(pixmap, width, height)?;
```

Used by [nimbleoffice-docx-raster](https://crates.io/crates/nimbleoffice-docx-raster),
[nimbleoffice-pptx-raster](https://crates.io/crates/nimbleoffice-pptx-raster),
[nimbleoffice-xlsx-raster](https://crates.io/crates/nimbleoffice-xlsx-raster)
and the VSDX raster backend. Each backend keeps its own color grammar and
error text on top of these helpers.

Part of NimbleOffice. Apache-2.0.
