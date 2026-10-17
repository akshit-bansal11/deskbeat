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

# The mark on its 108-unit grid, as in crates/core/src/mark.rs, coloured as in
# tray.rs and assets/deskbeat-dark.svg.
PAPER, INK, EMBER = (245, 244, 240), (19, 19, 22), (204, 53, 16)
SIDE, CORNER = 108, 26
SUN, SUN_RADIUS = (54, 72), 15
DESK, DESK_WIDTH = ((24, 83), (84, 83)), 12
RAY_FROM, RAY_WIDTH = 23, 10
RAYS = [(162, 11), (126, 20), (90, 27), (54, 22), (18, 14)]
SIZES = [16, 20, 24, 32, 40, 48, 64, 256]

RT_ICON, RT_GROUP_ICON = 3, 14
GROUP_ID = 1


def clamp(v):
    return max(0.0, min(1.0, v))


def rays():
    for degrees, length in RAYS:
        cos, sin = math.cos(math.radians(degrees)), math.sin(math.radians(degrees))
        yield tuple((SUN[0] + r * cos, SUN[1] - r * sin) for r in (RAY_FROM, RAY_FROM + length))


def capsule(x, y, ends, width):
    """How far (x, y) is outside a capsule; negative inside."""
    (ax, ay), (bx, by) = ends
    dx, dy = bx - ax, by - ay
    t = clamp(((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy))
    return math.hypot(x - ax - t * dx, y - ay - t * dy) - width / 2


def render(side):
    """Rows of (r, g, b, a), top to bottom, not premultiplied."""
    unit = side / SIDE
    half = SIDE / 2
    lines = list(rays())
    rows = []
    for row in range(side):
        y = (row + 0.5) / unit
        line = []
        for column in range(side):
            x = (column + 0.5) / unit

            def cover(outside):
                return clamp(0.5 - outside * unit)

            dx = abs(x - half) - (half - CORNER)
            dy = abs(y - half) - (half - CORNER)
            alpha = cover(math.hypot(max(dx, 0), max(dy, 0)) - CORNER)
            ink = cover(
                min([capsule(x, y, DESK, DESK_WIDTH)] + [capsule(x, y, r, RAY_WIDTH) for r in lines])
            )
            # A disc with everything below the horizon cut away.
            ember = cover(max(math.hypot(x - SUN[0], y - SUN[1]) - SUN_RADIUS, y - SUN[1]))
            rgb = [
                round(PAPER[i] * (1 - ink - ember) + INK[i] * ink + EMBER[i] * ember)
                for i in range(3)
            ]
            line.append((*rgb, round(255 * alpha)))
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
