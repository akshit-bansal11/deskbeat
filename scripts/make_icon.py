"""Writes crates/app/icon.res, the exe's icon as a compiled Windows resource.

The build links that file as it is (see crates/app/build.rs), so building
needs no resource compiler. Run this again only when the mark changes:

    python scripts/make_icon.py

The drawing is the one in crates/core/src/mark.rs, at eight sizes. Standard
library only.
"""

import math
import struct
import zlib
from pathlib import Path

# The mark on its 108-unit grid, as in crates/core/src/mark.rs.
INK = (0x15, 0x15, 0x1C)
SIDE, CORNER = 108, 26
# (kind, centre, radius, arc start, arc end, colour). The arcs run clockwise.
PIECES = [
    ("quarter", (20, 52), 32, (20, 20), (52, 52), (0x33, 0x55, 0xFF)),
    ("disc", (72, 36), 16, None, None, (0xFF, 0x6B, 0x2C)),
    ("disc", (36, 72), 16, None, None, (0xFF, 0xC9, 0x3C)),
    ("quarter", (88, 88), 32, (56, 88), (88, 56), (0xF3, 0xEF, 0xE6)),
]
SIZES = [16, 20, 24, 32, 40, 48, 64, 256]

RT_ICON, RT_GROUP_ICON = 3, 14
GROUP_ID = 1


def clamp(v):
    return max(0.0, min(1.0, v))


def outside(piece, x, y):
    """How far (x, y) is outside a piece, in units; negative inside."""
    kind, (cx, cy), radius, start, end, _ = piece
    disc = math.hypot(x - cx, y - cy) - radius
    if kind == "disc":
        return disc

    def behind(point):
        dx, dy = (point[0] - cx) / radius, (point[1] - cy) / radius
        return -((x - cx) * dx + (y - cy) * dy)

    return max(disc, behind(start), behind(end))


def render(side):
    """Rows of (r, g, b, a), top to bottom, not premultiplied."""
    unit = side / SIDE
    half = SIDE / 2
    rows = []
    for row in range(side):
        y = (row + 0.5) / unit
        line = []
        for column in range(side):
            x = (column + 0.5) / unit

            def cover(distance):
                return clamp(0.5 - distance * unit)

            dx = abs(x - half) - (half - CORNER)
            dy = abs(y - half) - (half - CORNER)
            alpha = cover(math.hypot(max(dx, 0), max(dy, 0)) - CORNER)
            # The pieces never overlap, so each blends over the tile alone.
            rgb = list(INK)
            for piece in PIECES:
                c = cover(outside(piece, x, y))
                rgb = [v + (p - v) * c for v, p in zip(rgb, piece[5])]
            line.append((*(round(v) for v in rgb), round(255 * alpha)))
        rows.append(line)
    return rows


def bitmap(rows):
    """An icon image as a DIB: the pixels bottom up, then an empty mask."""
    side = len(rows)
    pixels = b"".join(
        bytes((b, g, r, a)) for line in reversed(rows) for (r, g, b, a) in line
    )
    mask = bytes(((side + 31) // 32) * 4 * side)
    header = struct.pack(
        "<IiiHHIIiiII", 40, side, side * 2, 1, 32, 0, len(pixels) + len(mask), 0, 0, 0, 0
    )
    return header + pixels + mask


def png(rows):
    side = len(rows)

    def chunk(kind, body):
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body))
        )

    raw = b"".join(b"\x00" + bytes(v for px in line for v in px) for line in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", side, side, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def resource(kind, name, flags, data):
    """One entry of a .res file, with a numeric type and name."""
    header = struct.pack(
        "<IIHHHHIHHII", len(data), 32, 0xFFFF, kind, 0xFFFF, name, 0, flags, 0x0409, 0, 0
    )
    return header + data + bytes(-len(data) % 4)


def main():
    # Windows reads a PNG for the large size and bitmaps for the rest.
    images = [png(render(s)) if s == 256 else bitmap(render(s)) for s in SIZES]

    out = resource(0, 0, 0, b"")[:32]
    group = struct.pack("<HHH", 0, 1, len(SIZES))
    for index, (side, image) in enumerate(zip(SIZES, images), start=1):
        out += resource(RT_ICON, index, 0x1010, image)
        # 256 does not fit a byte and is written as 0.
        group += struct.pack("<BBBBHHIH", side % 256, side % 256, 0, 0, 1, 32, len(image), index)
    out += resource(RT_GROUP_ICON, GROUP_ID, 0x1030, group)

    target = Path(__file__).resolve().parent.parent / "crates" / "app" / "icon.res"
    target.write_bytes(out)
    print(f"wrote {target} ({len(out)} bytes, sizes {SIZES})")


if __name__ == "__main__":
    main()
