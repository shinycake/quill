#!/usr/bin/env python3
"""Fail unless a binary PPM (P6) or an 8-bit RGB/RGBA PNG looks like a rendered UI.

A blank window is one or two flat colours; a real screen has many distinct
colours and no single colour covering nearly all pixels. Used by the launch
smoke jobs (docs/decisions/ci-launch-smoke.md). Usage: ci-image-check.py FILE
"""
import struct
import sys
import zlib
from collections import Counter


def read_ppm(data):
    parts = data.split(None, 4)
    if parts[0] != b"P6":
        raise ValueError("not a P6 ppm")
    width, height, maxval = int(parts[1]), int(parts[2]), int(parts[3])
    if maxval != 255:
        raise ValueError("unsupported maxval")
    pixels = parts[4][-width * height * 3:]
    return width * height, [pixels[i:i + 3] for i in range(0, len(pixels), 3)]


def read_png(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a png")
    pos, idat, width, height, ctype = 8, b"", 0, 0, 0
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        kind = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + length]
        if kind == b"IHDR":
            width, height, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body[:13])
            if depth != 8 or ctype not in (2, 6) or interlace:
                raise ValueError("unsupported png (need 8-bit RGB/RGBA, no interlace)")
        elif kind == b"IDAT":
            idat += body
        pos += 12 + length
    bpp = 3 if ctype == 2 else 4
    raw = zlib.decompress(idat)
    stride = width * bpp
    prev = bytearray(stride)
    pixels = []
    p = 0
    for _ in range(height):
        ft = raw[p]
        line = bytearray(raw[p + 1:p + 1 + stride])
        p += 1 + stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if ft == 1:
                line[i] = (line[i] + a) & 255
            elif ft == 2:
                line[i] = (line[i] + b) & 255
            elif ft == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif ft == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 255
        prev = line
        pixels.extend(bytes(line[i:i + 3]) for i in range(0, stride, bpp))
    return width * height, pixels


def main():
    path = sys.argv[1]
    with open(path, "rb") as f:
        data = f.read()
    total, pixels = read_ppm(data) if data[:2] == b"P6" else read_png(data)
    counts = Counter(pixels)
    distinct = len(counts)
    dominant = counts.most_common(1)[0][1] / max(total, 1)
    print(f"{path}: {total} px, {distinct} distinct colours, dominant share {dominant:.2f}")
    if distinct < 64 or dominant > 0.9:
        sys.exit(1)


main()
