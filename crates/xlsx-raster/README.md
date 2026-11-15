# nimbleoffice-xlsx-raster

The native raster backend: paints a
[nimbleoffice-xlsx-render](https://crates.io/crates/nimbleoffice-xlsx-render)
display list to PNG through tiny-skia. Server-side twin of the browser's canvas
backend, and never part of the wasm build.

```rust
use xlsx_raster::render_png;

let png: Vec<u8> = render_png(&display_list)?;
```

Because both backends consume the same display list, a sheet rendered on the
server matches what the browser paints.

Reachable from [nimbleoffice-xlsx](https://crates.io/crates/nimbleoffice-xlsx)
through its default `raster` feature.

Part of NimbleOffice. Apache-2.0.
