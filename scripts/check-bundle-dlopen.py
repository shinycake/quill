#!/usr/bin/env python3
"""Load each bundled native library with the real dynamic loader and verify that
its dependencies came from the package, not the host.

Usage: check-bundle-dlopen.py <package-dir>
Run with LD_LIBRARY_PATH unset. Linux only (reads /proc/self/maps).
"""
import ctypes
import os
import sys


def main() -> int:
    pkg = os.path.realpath(sys.argv[1])
    lib_dir = os.path.join(pkg, "lib")
    failures = []
    for name in ("libtdjson.so", "libntgcalls.so", "librlottie.so", "libquillvideo.so"):
        path = os.path.join(lib_dir, name)
        try:
            ctypes.CDLL(path)
        except OSError as error:
            failures.append(f"{name}: dlopen failed: {error}")
    with open("/proc/self/maps") as maps:
        mapped = {line.split()[-1] for line in maps if line.strip().endswith(".so") or ".so." in line}
    # OpenSSL (tdjson) and FFmpeg (quillvideo) must come from the package.
    ffmpeg = sorted({os.path.basename(p) for p in mapped if os.path.basename(p).startswith(("libav", "libsw"))})
    if len(ffmpeg) != 5:
        failures.append(f"expected 5 FFmpeg libraries mapped, got {ffmpeg}")
    for dep in ["libssl.so.3", "libcrypto.so.3", *ffmpeg]:
        hits = sorted(p for p in mapped if os.path.basename(p) == dep)
        print(f"{dep}: {hits}")
        if not hits:
            failures.append(f"{dep} was not loaded at all")
        for hit in hits:
            if not os.path.realpath(hit).startswith(lib_dir + os.sep):
                failures.append(f"{dep} loaded from host: {hit}")
    for failure in failures:
        print(f"BAD  {failure}", file=sys.stderr)
    if failures:
        return 1
    print(f"OK: bundled libraries load from {lib_dir} with their bundled OpenSSL and FFmpeg")
    return 0


if __name__ == "__main__":
    sys.exit(main())
