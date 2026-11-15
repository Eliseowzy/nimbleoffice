# Image wrap in the float flow

`page-anchored-square.docx` is LibreOffice's tdf163149 (MPL-2.0): a wrapSquare image anchored
in an empty paragraph, positioned 243.6pt from the page top, with the body text in the next
paragraph after two spaced empty ones. Word 16.114 wraps the text's first line beside the image
(it ends at 376.2pt, the image's left edge 385.1pt less distL 9pt): the line's frame sits at the
image's top only once every earlier paragraph's spacing is counted.
