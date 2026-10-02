#!/usr/bin/env python3
"""Verify generated checklist updates pass, while manual or mixed edits fail."""
import pathlib
import shutil
import subprocess
import tempfile

source = pathlib.Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix="quill-parity-check-") as scratch:
    root = pathlib.Path(scratch)

    def run(*args, success=True):
        result = subprocess.run(args, cwd=root, text=True, capture_output=True)
        assert (result.returncode == 0) == success, result.stdout + result.stderr
        return result.stdout.strip()

    def commit():
        run("git", "add", ".")
        run("git", "commit", "-qm", "fixture")

    run("git", "init", "-q")
    run("git", "config", "user.name", "Fixture")
    run("git", "config", "user.email", "fixture@example.invalid")
    (root / "scripts").mkdir()
    for name in ["apply-parity-fragment.sh", "check-parity-fragments.sh"]:
        shutil.copy(source / name, root / "scripts" / name)
    (root / "parity-fragments").mkdir()
    (root / "README.md").write_text("- [ ] Fixture <!-- parity:fixture-one -->\n")
    (root / "parity-fragments/done.txt").write_text("parity:fixture-one\n")
    commit()
    base = run("git", "rev-parse", "HEAD")
    run("bash", "scripts/apply-parity-fragment.sh", "parity-fragments/done.txt")
    commit()
    generated = run("git", "rev-parse", "HEAD")
    run("bash", "scripts/check-parity-fragments.sh", base)
    (root / "README.md").write_text((root / "README.md").read_text() + "Manual claim\n")
    commit()
    run("bash", "scripts/check-parity-fragments.sh", base, success=False)
    run("git", "reset", "--hard", generated)
    (root / "implementation.rs").write_text("// mixed code change\n")
    commit()
    run("bash", "scripts/check-parity-fragments.sh", base, success=False)
    run("git", "reset", "--hard", generated)
    (root / "parity-fragments/new.txt").write_text("parity:fixture-one\n")
    commit()
    run("bash", "scripts/check-parity-fragments.sh", base, success=False)
print("PASS: generated reconciliation accepted; manual, mixed and unmerged claims rejected")
