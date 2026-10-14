#!/usr/bin/env python3
"""Rebuilds ultrahdr.jpg and pq.jxl, the fixtures of the Windows HDR
decoders (src/ui/hdr_image/cpu). Needs libultrahdr's `ultrahdr_app` and
libjxl's `cjxl` on PATH (Homebrew: `brew install libultrahdr jpeg-xl`).

    python3 tests/fixtures/hdr/make-cpu-fixtures.py

ultrahdr.jpg: 64x32, a warm gradient climbing to 4x SDR white on the right
(top four rows mid gray), multichannel gain map at half size, ISO 21496-1
metadata. pq.jxl: 32x16 lossless PQ with sRGB primaries, the left half at
SDR reference white (203 nits), the right half at 812 nits.

The reference values in cpu/tests.rs come from `ultrahdr_app -m 1 -j
ultrahdr.jpg -o 0 -O 4` (linear half floats)."""

import os
import struct
import subprocess
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))


def srgb_byte(linear):
    value = min(max(linear, 0.0), 1.0)
    if value <= 0.0031308:
        value *= 12.92
    else:
        value = 1.055 * value ** (1 / 2.4) - 0.055
    return round(value * 255)


def ultra_hdr(work):
    width, height = 64, 32
    hdr, sdr = bytearray(), bytearray()
    for y in range(height):
        for x in range(width):
            t = x / (width - 1)
            linear = [0.05 + t * 3.95, 0.04 + t * 2.4, 0.03 + t * 1.2]
            if y < 4:
                linear = [0.18, 0.18, 0.18]
            hdr += struct.pack("<eeee", *linear, 1.0)
            sdr += bytes([srgb_byte(c) for c in linear] + [255])
    hdr_path, sdr_path = os.path.join(work, "hdr.raw"), os.path.join(work, "sdr.raw")
    open(hdr_path, "wb").write(hdr)
    open(sdr_path, "wb").write(sdr)
    subprocess.run(
        ["ultrahdr_app", "-m", "0", "-p", hdr_path, "-y", sdr_path, "-a", "4", "-b", "3",
         "-t", "0", "-C", "0", "-c", "0", "-w", str(width), "-h", str(height),
         "-q", "95", "-Q", "95", "-M", "1", "-s", "2",
         "-z", os.path.join(HERE, "ultrahdr.jpg")],
        check=True,
        stdout=subprocess.DEVNULL,
    )


def pq_jxl(work):
    m1, m2 = 0.1593017578125, 78.84375
    c1, c2, c3 = 0.8359375, 18.8515625, 18.6875

    def pq(nits):
        y = (nits / 10000.0) ** m1
        return ((c1 + c2 * y) / (1 + c3 * y)) ** m2

    width, height = 32, 16
    ppm = bytearray(b"P6\n%d %d\n65535\n" % (width, height))
    for _ in range(height):
        for x in range(width):
            nits = 203.0 if x < width // 2 else 812.0
            ppm += round(pq(nits) * 65535).to_bytes(2, "big") * 3
    ppm_path = os.path.join(work, "pq.ppm")
    open(ppm_path, "wb").write(ppm)
    subprocess.run(
        ["cjxl", ppm_path, os.path.join(HERE, "pq.jxl"), "-d", "0",
         "-x", "color_space=RGB_D65_SRG_Rel_PeQ"],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


if __name__ == "__main__":
    with tempfile.TemporaryDirectory() as work:
        ultra_hdr(work)
        pq_jxl(work)
