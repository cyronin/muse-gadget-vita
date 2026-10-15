#!/usr/bin/env python3
"""Draw the app's LiveArea pictures into vita/static/sce_sys/.

    tools/make_icon.py

A speech bubble holding three dots in the avatar's colours for listening,
thinking and speaking. It is drawn here, not taken from the avatar, because
the avatar is Meta's and stays out of the repository. The Vita wants 8-bit
palette PNGs, which this writes with nothing but the standard library.
"""
import pathlib
import struct
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "vita" / "static" / "sce_sys"
FONT = (ROOT / "vita" / "src" / "msxfont.bin").read_bytes()

PALETTE = [
    (0x24, 0x24, 0x4A),  # 0 background
    (0x15, 0x15, 0x2D),  # 1 shadow
    (0xF4, 0xF1, 0xEA),  # 2 bubble
    (0x4A, 0xA3, 0xFF),  # 3 listening
    (0xFF, 0x6F, 0xAE),  # 4 thinking
    (0x4E, 0xE0, 0x8A),  # 5 speaking
]
# Darker shades of the background for the LiveArea's gradient.
GRADIENT = 24
for step in range(GRADIENT):
    fade = step / (GRADIENT - 1)
    PALETTE.append(tuple(round(a + (b - a) * fade) for a, b in zip(PALETTE[0], (0x10, 0x10, 0x22))))
FIRST_SHADE = 6


def png(path, pixels):
    """Writes rows of palette indexes as an 8-bit palette PNG."""
    height, width = len(pixels), len(pixels[0])

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    raw = b"".join(b"\0" + bytes(row) for row in pixels)
    data = b"\x89PNG\r\n\x1a\n"
    data += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 3, 0, 0, 0))
    data += chunk(b"PLTE", b"".join(bytes(colour) for colour in PALETTE))
    data += chunk(b"IDAT", zlib.compress(raw, 9))
    data += chunk(b"IEND", b"")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    print("wrote %s (%dx%d)" % (path.relative_to(ROOT), width, height))


def canvas(width, height, colour=0):
    return [[colour] * width for _ in range(height)]


def bubble(pixels, left, top, cell):
    """The bubble on a 22 by 18 grid of `cell`-pixel squares, its top left at `left`, `top`."""

    def square(x, y, colour):
        for row in range(top + y * cell, top + (y + 1) * cell):
            pixels[row][left + x * cell:left + (x + 1) * cell] = [colour] * cell

    # Body with rounded corners, then the tail at the bottom left.
    shape = set()
    for y in range(15):
        inset = {0: 2, 1: 1, 13: 1, 14: 2}.get(y, 0)
        shape.update((x, y) for x in range(inset, 22 - inset))
    shape.update({(4, 15), (5, 15), (6, 15), (4, 16), (5, 16), (4, 17)})
    for x, y in shape:
        square(x + 1, y + 1, 1)
    for x, y in shape:
        square(x, y, 2)
    for first, colour in ((3, 3), (9, 4), (15, 5)):
        for x in range(first, first + 4):
            for y in range(5, 9):
                square(x, y, colour)


def text(pixels, left, top, words, scale, colour):
    for place, character in enumerate(words.encode()):
        for line in range(8):
            bits = FONT[character * 8 + line]
            for pixel in range(8):
                if bits & (0x80 >> pixel):
                    x = left + (place * 8 + pixel) * scale
                    for row in range(top + line * scale, top + (line + 1) * scale):
                        pixels[row][x:x + scale] = [colour] * scale


def gradient(width, height):
    return [[FIRST_SHADE + y * GRADIENT // height] * width for y in range(height)]


# The bubble on the home screen, which shows it in a circle.
icon = canvas(128, 128)
bubble(icon, 20, 28, 4)
png(OUT / "icon0.png", icon)

# What the LiveArea shows behind its start button.
startup = canvas(280, 158)
bubble(startup, 24, 40, 4)
text(startup, 130, 63, "Muse", 4, 2)
png(OUT / "livearea" / "contents" / "startup.png", startup)

png(OUT / "livearea" / "contents" / "bg.png", gradient(840, 500))
