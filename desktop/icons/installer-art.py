#!/usr/bin/env python3
"""Regenerate the Windows installer artwork from the app icon.

NSIS and WiX both want bitmaps at fixed sizes, and neither will scale one for
you: a wrong-sized image is stretched, cropped or ignored depending on which
page it lands on. So the sizes here are the installers' sizes, not a design
choice, and the art is generated rather than drawn by hand so that changing
the icon or the wordmark does not leave four stale bitmaps behind.

    python3 desktop/icons/installer-art.py

Run from anywhere; paths are resolved against this file. Needs Pillow, which
is the only thing in this repository that does, which is why the output is
committed: a Windows build should not have to install a Python imaging
library to produce an installer.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent

# The icon's own gradient, top-left to bottom-right.
INDIGO = (91, 110, 232)
VIOLET = (139, 92, 246)
# The wordmark on white, dark enough to read as text rather than as a logo.
INK = (43, 47, 69)

FONTS = ROOT / "native" / "assets" / "fonts"


def font(name: str, size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(FONTS / name), size)


def gradient(size: tuple[int, int]) -> Image.Image:
    """The brand gradient, running corner to corner."""
    width, height = size
    image = Image.new("RGB", size)
    pixels = image.load()
    span = (width - 1) + (height - 1)
    for y in range(height):
        for x in range(width):
            t = (x + y) / span
            pixels[x, y] = tuple(
                round(start + (end - start) * t) for start, end in zip(INDIGO, VIOLET)
            )
    return image


def mark(height: int) -> Image.Image:
    """The speech bubble from the app icon, as a mask.

    Lifted from the icon rather than redrawn, so the two cannot drift apart.
    The bubble is the white part; everything else is the rounded square behind
    it.
    """
    icon = Image.open(HERE / "icon.png").convert("RGBA")
    red, green, blue, alpha = icon.split()
    white = Image.new("L", icon.size, 0)
    pixels = white.load()
    source = icon.load()
    for y in range(icon.height):
        for x in range(icon.width):
            r, g, b, a = source[x, y]
            pixels[x, y] = 255 if a > 128 and min(r, g, b) > 235 else 0
    del red, green, blue, alpha

    box = white.getbbox()
    cropped = white.crop(box)
    width = round(cropped.width * height / cropped.height)
    return cropped.resize((width, height), Image.LANCZOS)


def stamp(canvas: Image.Image, shape: Image.Image, xy: tuple[int, int], fill) -> None:
    """Paint `shape` (a mask) onto `canvas` in one colour."""
    if isinstance(fill, Image.Image):
        canvas.paste(fill.resize(shape.size, Image.LANCZOS), xy, shape)
    else:
        canvas.paste(fill, xy, shape)


def centred(draw: ImageDraw.ImageDraw, text: str, y: int, width: int, **kwargs) -> None:
    draw.text((width / 2, y), text, anchor="ma", **kwargs)


def lockup(canvas: Image.Image, edge: int, middle: int, size: int, align: str) -> None:
    """Mark and wordmark on white, for the strips the installers draw text on.

    `align` is which edge `edge` is: "left" puts the mark first and the
    wordmark after it, "right" the other way about, so the lockup always
    reads outward from the side it sits on.
    """
    glyph = mark(size)
    gap = round(size * 0.34)
    draw = ImageDraw.Draw(canvas)
    wordmark = font("Inter-Bold.ttf", round(size * 0.62))

    if align == "left":
        stamp(canvas, glyph, (edge, middle - size // 2), gradient(glyph.size))
        draw.text((edge + glyph.width + gap, middle), "MiniChat", font=wordmark,
                  fill=INK, anchor="lm")
    else:
        stamp(canvas, glyph, (edge - glyph.width, middle - size // 2), gradient(glyph.size))
        draw.text((edge - glyph.width - gap, middle), "MiniChat", font=wordmark,
                  fill=INK, anchor="rm")


def panel(canvas: Image.Image, width: int, height: int, scale: float = 1.0) -> None:
    """The tall brand panel: mark, wordmark, what the app is."""
    canvas.paste(gradient((width, height)), (0, 0))
    draw = ImageDraw.Draw(canvas)

    glyph = mark(round(74 * scale))
    stamp(canvas, glyph, ((width - glyph.width) // 2, round(62 * scale)), (255, 255, 255))

    centred(
        draw,
        "MiniChat",
        round(166 * scale),
        width,
        font=font("Inter-Bold.ttf", round(26 * scale)),
        fill=(255, 255, 255),
    )
    rule = round(30 * scale)
    y = round(205 * scale)
    # A tint of the gradient rather than white: a divider, not a second
    # thing to look at. `ImageDraw` on an RGB canvas ignores alpha, so the
    # blend is done here.
    draw.line(
        [(width / 2 - rule, y), (width / 2 + rule, y)],
        fill=(203, 206, 252),
        width=1,
    )
    for index, line in enumerate(("Voice, video and text", "for one community")):
        centred(
            draw,
            line,
            round((218 + index * 16) * scale),
            width,
            font=font("Inter-Medium.ttf", round(11 * scale)),
            fill=(235, 236, 255),
        )
    centred(
        draw,
        "S E L F - H O S T E D",
        round(286 * scale),
        width,
        font=font("Inter-SemiBold.ttf", round(8 * scale)),
        fill=(216, 219, 255),
    )


def nsis_sidebar() -> Image.Image:
    """MUI's welcome and finish pages: 164x314, exactly."""
    canvas = Image.new("RGB", (164, 314))
    panel(canvas, 164, 314)
    return canvas


def nsis_header() -> Image.Image:
    """MUI's interior header: 150x57, over a white strip.

    Left-aligned because that is where MUI puts it: the header bitmap sits at
    the left edge and the page's title is drawn to the right of it, unless
    the script defines `MUI_HEADERIMAGE_RIGHT`, which Tauri's does not.
    """
    canvas = Image.new("RGB", (150, 57), (255, 255, 255))
    lockup(canvas, edge=12, middle=28, size=30, align="left")
    return canvas


def wix_banner() -> Image.Image:
    """The MSI's top banner: 493x58, with WiX's own text down the left."""
    canvas = Image.new("RGB", (493, 58), (255, 255, 255))
    lockup(canvas, edge=477, middle=29, size=32, align="right")
    return canvas


def wix_dialog() -> Image.Image:
    """The MSI's welcome dialog: 493x312, art down the left, text on the right."""
    canvas = Image.new("RGB", (493, 312), (255, 255, 255))
    strip = Image.new("RGB", (164, 312))
    panel(strip, 164, 312)
    canvas.paste(strip, (0, 0))
    return canvas


def main() -> None:
    for name, build in (
        ("installer-sidebar.bmp", nsis_sidebar),
        ("installer-header.bmp", nsis_header),
        ("installer-banner.bmp", wix_banner),
        ("installer-dialog.bmp", wix_dialog),
    ):
        image = build()
        # 24-bit, bottom-up, 40-byte header: what NSIS and WiX both read. A
        # bit depth with an alpha channel is what makes one of them render a
        # black rectangle instead.
        image.convert("RGB").save(HERE / name, "BMP")
        print(f"{name}: {image.width}x{image.height}")


if __name__ == "__main__":
    main()
