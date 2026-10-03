#!/usr/bin/env python3
"""Write a minimal, valid GLB (binary glTF 2.0) for smoke-testing lyco-cli.

Usage: make-test-glb.py OUT.glb

Kept as a script rather than a checked-in binary so the container layout
stays readable and reviewable: a 12-byte header, a JSON chunk and a BIN
chunk, both padded to a 4-byte boundary.
"""

import json
import struct
import sys

POSITIONS = [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)]
INDICES = (0, 1, 2)


def build() -> bytes:
    binary = b"".join(struct.pack("<3f", *p) for p in POSITIONS)
    binary += struct.pack("<%dH" % len(INDICES), *INDICES)
    binary += b"\x00" * (-len(binary) % 4)

    gltf = {
        "asset": {"version": "2.0", "generator": "lyco-ai-toolkit/ci"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "Body", "mesh": 0}],
        "meshes": [
            {
                "name": "demo_triangle",
                "primitives": [{"attributes": {"POSITION": 0}, "indices": 1}],
            }
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"},
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 6},
        ],
        "buffers": [{"byteLength": 42}],
    }

    encoded = json.dumps(gltf, separators=(",", ":")).encode("utf-8")
    # chunkData(JSON) is padded with spaces, chunkData(BIN) with zeros.
    encoded += b" " * (-len(encoded) % 4)

    total = 12 + 8 + len(encoded) + 8 + len(binary)
    out = b"glTF" + struct.pack("<II", 2, total)
    out += struct.pack("<I", len(encoded)) + b"JSON" + encoded
    out += struct.pack("<I", len(binary)) + b"BIN\x00" + binary
    return out


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    data = build()
    with open(sys.argv[1], "wb") as handle:
        handle.write(data)
    print(f"wrote {sys.argv[1]} ({len(data)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
