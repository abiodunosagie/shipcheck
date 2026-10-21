"""Outlines the word "shipcheck" from a Poppins font file into an SVG path, so the logo needs no font.

Run: python3 brand/outline_wordmark.py path/to/Poppins-SemiBold.ttf
Prints WORD, WORD_BOX and X_HEIGHT (font units, y down, baseline at 0), ready to paste into generate.py.
Poppins is licensed under the SIL Open Font License 1.1, which allows it in logos.
Needs fontTools (pip install fonttools). Kerning (GPOS) is not applied; the letters of "shipcheck" need none.
"""
import sys

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

WORD = "shipcheck"
TRACKING = -8  # font units between letters; Poppins sets a little loose at display size


def outline(font_path):
    font = TTFont(font_path)
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    pen = SVGPathPen(glyphs, ntos=lambda v: f"{v:g}")
    bounds = BoundsPen(glyphs)
    x = 0
    for ch in WORD:
        name = cmap[ord(ch)]
        # Flip y so the path is y down like the rest of the SVG masters.
        glyphs[name].draw(TransformPen(pen, (1, 0, 0, -1, x, 0)))
        glyphs[name].draw(TransformPen(bounds, (1, 0, 0, -1, x, 0)))
        x += glyphs[name].width + TRACKING
    return pen.getCommands(), bounds.bounds, font["OS/2"].sxHeight


if __name__ == "__main__":
    path, box, x_height = outline(sys.argv[1])
    print(f'WORD = "{path}"')
    print(f"WORD_BOX = {tuple(round(v) for v in box)}")
    print(f"X_HEIGHT = {x_height}")
