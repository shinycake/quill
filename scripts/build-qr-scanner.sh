#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/target/qr-scanner/quill-qr-scanner"
mkdir -p "$(dirname "$OUT")"
# Developer runs are standalone executables; embed the same camera purpose.
cat > "$(dirname "$OUT")/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.shinycake.quill.qrscanner</string>
<key>CFBundleName</key><string>Quill</string>
<key>NSCameraUsageDescription</key><string>Quill scans login QR codes to link another device after your confirmation.</string>
</dict></plist>
PLIST
xcrun clang -fobjc-arc -mmacosx-version-min=14.0 -Wall -Wextra -Werror \
  -Wno-unused-parameter -framework AppKit -framework AVFoundation \
  -framework CoreImage -framework Vision -framework CoreVideo "$ROOT/native/qr-scanner.m" \
  -Wl,-sectcreate,__TEXT,__info_plist,"$(dirname "$OUT")/Info.plist" -o "$OUT"
# QUILL_QR_SELF_TEST=warn reports a failed self-test without failing the build,
# for virtual machines (GitHub-hosted macOS) whose Vision framework may not
# decode barcodes; real Macs keep the hard failure.
if ! "$OUT" --self-test; then
  if [[ "${QUILL_QR_SELF_TEST:-}" == warn ]]; then
    echo "warning: quill-qr-scanner --self-test failed on this machine (QUILL_QR_SELF_TEST=warn); continuing" >&2
  else
    echo "error: quill-qr-scanner --self-test failed (set QUILL_QR_SELF_TEST=warn on a VM without Vision support)" >&2
    exit 1
  fi
fi
