#!/usr/bin/env python3
"""Write a solid-colour 64x64 RGBA PNG.

Usage: make-solid-skin.py OUT.png [R G B]

Used by CI to build a skin whose whole surface is one colour, which is what a
diffusion model produces when it has learned that backgrounds are white: every
overlay rectangle comes back filled. Hand-rolling the PNG keeps the check free
of an image dependency in the harness.
"""

import struct
import sys
import zlib

SIZE = 64


def chunk(tag: bytes, data: bytes) -> bytes:
    body = tag + data
    crc = zlib.crc32(body) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + body + struct.pack(">I", crc)


def build(r: int, g: int, b: int) -> bytes:
    pixel = bytes([r, g, b, 255])
    # Each scanline is prefixed with a filter byte; 0 means no filtering.
    raw = b"".join(b"\x00" + pixel * SIZE for _ in range(SIZE))
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    return png


def main() -> int:
    if len(sys.argv) not in (2, 5):
        print(__doc__, file=sys.stderr)
        return 2
    path = sys.argv[1]
    r, g, b = (int(v) for v in sys.argv[2:5]) if len(sys.argv) == 5 else (255, 255, 255)
    with open(path, "wb") as handle:
        handle.write(build(r, g, b))
    print(f"wrote {path} ({SIZE}x{SIZE} solid {r},{g},{b})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
