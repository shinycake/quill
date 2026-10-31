#!/usr/bin/env python3
"""The manifest embedded in quill.exe keeps the settings Quill relies on.

gpui-pre's `windows-manifest` feature embeds
third_party/gpui-pre/resources/windows/gpui.manifest.xml into the Windows
executable (docs/decisions/platform-small-fixes.md). Telegram Desktop ships
the same per-monitor DPI and long-path settings. Without `longPathAware`,
Win32 calls that take a plain path (the shell, FFmpeg's file opens) fail
beyond 260 characters even where the system allows long paths.
"""
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

MANIFEST = Path(__file__).resolve().parent.parent / (
    "third_party/gpui-pre/resources/windows/gpui.manifest.xml"
)
SETTINGS_2016 = "{http://schemas.microsoft.com/SMI/2016/WindowsSettings}"
REQUIRED = {
    f"{SETTINGS_2016}longPathAware": "true",
    f"{SETTINGS_2016}dpiAwareness": "PerMonitorV2",
}


def main() -> int:
    root = ET.parse(MANIFEST).getroot()
    found = {element.tag: (element.text or "").strip() for element in root.iter()}
    failures = [
        f"{tag.split('}')[1]} must be {value!r}, found {found.get(tag)!r}"
        for tag, value in REQUIRED.items()
        if found.get(tag) != value
    ]
    for failure in failures:
        print(f"{MANIFEST.name}: {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
