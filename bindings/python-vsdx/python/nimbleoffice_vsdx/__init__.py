"""Read Visio VSDX diagrams on the NimbleOffice Rust engine."""

from typing import TypedDict

from ._nimbleoffice_vsdx import Cell, Connect, Png, Diagram, Page, ParseError, RangeError, RenderError, Shape, VsdxError, __version__
from ._nimbleoffice_vsdx import decode_font_face

class PngTarget(TypedDict):
    page: int


__all__ = ["Png", "PngTarget", "Cell", "Connect", "Diagram", "Page", "ParseError", "RangeError", "RenderError", "Shape", "VsdxError", "__version__", "decode_font_face"]
