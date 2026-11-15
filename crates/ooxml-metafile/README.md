# nimbleoffice-metafile

Bounded replay of the EMF, EMF+ and WMF metafiles Office documents embed.

- `decode`: the strict vector replay PowerPoint pictures draw from: solid
  fills and strokes in fractions of the frame, or nothing
- `replay`: every record it can reproduce, text, bitmaps and clipping
  included, with the ink it drew without listed as omissions
- `to_svg`: that drawing as a self-contained SVG, behind the `svg` feature

Part of NimbleOffice. Apache-2.0.
