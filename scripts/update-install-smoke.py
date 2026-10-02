#!/usr/bin/env python3
"""Exercise the real installer on private core-binary copies, never a live app."""
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

binary = pathlib.Path(sys.argv[1]).resolve()
assert subprocess.check_output([binary, "--build-info"], text=True, timeout=5).strip() == "core", "Use a core build without UI for this check"
asset_name = subprocess.check_output([binary, "--release-asset-name"], text=True).strip()
with tempfile.TemporaryDirectory(prefix="quill-update-proof-") as root:
    root = pathlib.Path(root).resolve()
    target = root / "quill"
    staged = root / "new"
    marker = root / "relaunched"
    plan = root / "plan.json"
    release = {
        "version": "0.2.0",
        "notes": "Release notes preserved 😀\nSecond line",
        "url": "https://github.com/shinycake/quill/releases/tag/v0.2.0",
        "asset": {
            "url": "https://github.com/shinycake/quill/releases/download/v0.2.0/" + asset_name,
            "size": 0,
            "sha256": "",
        },
    }
    data = f"#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'Quill 0.2.0'; elif [ \"$1\" = --build-info ]; then echo ui; else touch '{marker}'; fi\n".encode()
    for corrupt in [False, True]:
        shutil.copy2(binary, target)
        staged.write_bytes(data)
        staged.chmod(0o700)
        release["asset"]["size"] = len(data)
        release["asset"]["sha256"] = hashlib.sha256(data).hexdigest() if not corrupt else "0" * 64
        plan.write_text(json.dumps({"target": str(target), "staged": str(staged), "parent": 99999999, "release": release}))
        result = subprocess.run([target, "--apply-update", plan], capture_output=True, timeout=15)
        receipt = json.loads(target.with_suffix(".last-update.json").read_text())
        assert receipt["release"]["notes"] == release["notes"]
        assert receipt["failed"] == corrupt
        if corrupt:
            assert result.returncode == 1
            assert hashlib.sha256(target.read_bytes()).digest() == hashlib.sha256(binary.read_bytes()).digest()
        else:
            assert result.returncode == 0 and target.read_bytes() == data
            for _ in range(50):
                if marker.exists():
                    break
                time.sleep(0.02)
            assert marker.exists(), "Updated executable was not relaunched"
    assert not target.with_suffix(".update-backup").exists()
print("PASS: real handoff, checksum rejection, atomic replacement, relaunch, original preserved, changelog receipt")
