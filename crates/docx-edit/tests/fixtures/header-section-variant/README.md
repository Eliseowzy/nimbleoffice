# Header variants per section

`portrait-then-landscape.docx` is LibreOffice's shape-anchored-in-follow (MPL-2.0): two portrait
three-column sections, then a landscape one, all sharing header1. Word lays page one's header out
at the portrait width (its lines end by 522pt on a 612pt page); the engine measures the header once
per section and must paint each page with its own section's measure.
