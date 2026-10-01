#!/usr/bin/env bash
# Exercise actual history GIF decoding/rendering using the injected demo only.
# Usage: scripts/macos-gif-smoke.sh [binary] [evidence-directory]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${1:-$ROOT/target/debug/quill}"
OUT="${2:-$(mktemp -d /tmp/quill-gif-evidence.XXXXXX)}"
[[ "$(uname -s)" == Darwin ]]
if pgrep -x quill >/dev/null; then echo 'Close the existing Quill process before this isolated smoke check.' >&2; exit 2; fi
mkdir -p "$OUT"
TMP="$(mktemp -d /tmp/quill-gif-smoke.XXXXXX)"
APP_PID=''
cleanup() { if [[ -n "$APP_PID" ]]; then kill "$APP_PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }
trap cleanup EXIT
cat > "$TMP/check.swift" <<'SWIFT'
import Foundation
import CoreGraphics
import ImageIO
if CommandLine.arguments[1] == "window" {
    let pid = Int(CommandLine.arguments[2])!
    let rows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String:Any]] ?? []
    if let row = rows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }), let id = row[kCGWindowNumber as String] as? Int { print(id) }
} else {
    func pixels(_ path: String) -> [UInt8] {
        let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil)!
        let image = CGImageSourceCreateImageAtIndex(source, 0, nil)!
        let scale = Double(image.width) / 1200.0
        // Inside the GIF image; excludes toolbar, toast, avatar and scroll bar.
        let crop = image.cropping(to: CGRect(x:350*scale, y:170*scale, width:225*scale, height:80*scale))!
        var bytes = [UInt8](repeating:0, count:crop.width*crop.height*4)
        bytes.withUnsafeMutableBytes { buffer in
            let context = CGContext(data:buffer.baseAddress, width:crop.width, height:crop.height, bitsPerComponent:8, bytesPerRow:crop.width*4, space:CGColorSpaceCreateDeviceRGB(), bitmapInfo:CGImageAlphaInfo.premultipliedLast.rawValue)!
            context.draw(crop, in:CGRect(x:0, y:0, width:crop.width, height:crop.height))
        }
        return bytes
    }
    let first = pixels(CommandLine.arguments[2])
    var changed = false
    for path in CommandLine.arguments.dropFirst(3) {
        let other = pixels(path)
        precondition(first.count == other.count)
        var differing = 0
        for i in stride(from:0, to:first.count, by:4) {
            let red = abs(Int(first[i]) - Int(other[i]))
            let green = abs(Int(first[i+1]) - Int(other[i+1]))
            let blue = abs(Int(first[i+2]) - Int(other[i+2]))
            if red + green + blue > 30 { differing += 1 }
        }
        changed = changed || differing > first.count/8
    }
    precondition(changed, "History GIF pixels did not visibly change")
    print("PASS: native history GIF visibly loops across captures")
}
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
rm -f "$OUT/.quill-ready-ready-gif-playback"
cd "$ROOT"
"$BINARY" --screenshot-demo ready-gif-playback "$OUT" > "$OUT/app.log" 2>&1 &
APP_PID=$!
for _ in {1..40}; do [[ -f "$OUT/.quill-ready-ready-gif-playback" ]] && break; sleep 0.1; done
[[ -f "$OUT/.quill-ready-ready-gif-playback" ]]
WINDOW_ID="$("$TMP/check" window "$APP_PID")"
[[ -n "$WINDOW_ID" ]]
for frame in a b c; do screencapture -x -o -l "$WINDOW_ID" "$OUT/frame-$frame.png"; sleep 0.2; done
wait "$APP_PID"
APP_PID=''
"$TMP/check" pixels "$OUT/frame-a.png" "$OUT/frame-b.png" "$OUT/frame-c.png"
printf 'Evidence: %s\n' "$OUT"
