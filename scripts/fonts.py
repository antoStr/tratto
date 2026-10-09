"""Builds assets/fonts: static, Latin-only instances of the board and interface fonts.

Run once with fontTools (pip install fonttools): python scripts/fonts.py
Sources are the variable fonts of github.com/google/fonts (OFL / Apache).
"""
import io, os, urllib.request
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools import subset

BASE = "https://raw.githubusercontent.com/google/fonts/main/"
OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "fonts")
# name: (upright, italic or None, regular weight)
FONTS = {
    "sans": ("ofl/inter/Inter[opsz,wght].ttf", "ofl/inter/Inter-Italic[opsz,wght].ttf", 400),
    "rounded": ("ofl/nunito/Nunito[wght].ttf", "ofl/nunito/Nunito-Italic[wght].ttf", 400),
    "geometric": ("ofl/montserrat/Montserrat[wght].ttf", "ofl/montserrat/Montserrat-Italic[wght].ttf", 400),
    "condensed": ("ofl/oswald/Oswald[wght].ttf", None, 400),
    "serif": ("ofl/gelasio/Gelasio[wght].ttf", "ofl/gelasio/Gelasio-Italic[wght].ttf", 400),
    "book": ("ofl/lora/Lora[wght].ttf", "ofl/lora/Lora-Italic[wght].ttf", 400),
    "display": ("ofl/playfairdisplay/PlayfairDisplay[wght].ttf", "ofl/playfairdisplay/PlayfairDisplay-Italic[wght].ttf", 400),
    "mono": ("ofl/jetbrainsmono/JetBrainsMono[wght].ttf", "ofl/jetbrainsmono/JetBrainsMono-Italic[wght].ttf", 400),
    "hand": ("ofl/caveat/Caveat[wght].ttf", None, 500),
    "print": ("ofl/patrickhand/PatrickHand-Regular.ttf", None, None),
    "marker": ("apache/permanentmarker/PermanentMarker-Regular.ttf", None, None),
}
BOLD = 650
UNICODES = "U+0020-007E,U+00A0-017F,U+0192,U+02C6-02DD,U+2000-206F,U+20AC,U+2122,U+2190-21FF,U+2212,U+2713,U+2715,U+25A0-25FF,U+FFFD"

def fetch(path):
    with urllib.request.urlopen(BASE + urllib.parse.quote(path)) as r:
        return r.read()

def build(data, wght, out):
    font = TTFont(io.BytesIO(data))
    if "fvar" in font:
        axes = {a.axisTag: a for a in font["fvar"].axes}
        pin = {tag: (min(max(wght, a.minValue), a.maxValue) if tag == "wght" else a.defaultValue) for tag, a in axes.items()}
        font = instantiateVariableFont(font, pin, updateFontNames=False)
    opts = subset.Options()
    opts.layout_features = ["kern", "liga", "clig", "calt", "ccmp", "locl", "mark", "mkmk"]
    opts.name_IDs = ["*"]
    opts.notdef_outline = True
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=subset.parse_unicodes(UNICODES))
    sub.subset(font)
    font.save(os.path.join(OUT, out))
    print(out, os.path.getsize(os.path.join(OUT, out)))

os.makedirs(OUT, exist_ok=True)
for name, (upright, italic, regular) in FONTS.items():
    up = fetch(upright)
    build(up, regular or 400, f"{name}.ttf")
    if regular:
        build(up, BOLD, f"{name}-bold.ttf")
    if italic:
        it = fetch(italic)
        build(it, regular, f"{name}-italic.ttf")
        build(it, BOLD, f"{name}-bolditalic.ttf")
# Interface: Figma's 450 (regular) and 550 (medium) weights of Inter.
inter = fetch(FONTS["sans"][0])
build(inter, 450, "ui.ttf")
build(inter, 550, "ui-medium.ttf")
