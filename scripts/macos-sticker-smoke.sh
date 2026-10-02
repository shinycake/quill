#!/usr/bin/env bash
# Exercise actual history and picker sticker decoding/rendering using the injected demo only.
# Usage: scripts/macos-sticker-smoke.sh [binary] [evidence-directory]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${1:-$ROOT/target/debug/quill}"
OUT="${2:-$(mktemp -d /tmp/quill-sticker-evidence.XXXXXX)}"
[[ "$(uname -s)" == Darwin ]]
# Native reads, actions and captures target only the owned fixture PID.
mkdir -p "$OUT"
TMP="$(mktemp -d /tmp/quill-sticker-smoke.XXXXXX)"
APP_PID=''
cleanup() { if [[ -n "$APP_PID" ]]; then kill "$APP_PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }
trap cleanup EXIT
cat > "$TMP/check.swift" <<'SWIFT'
import Foundation
import CoreGraphics
import ImageIO
import ApplicationServices
if CommandLine.arguments[1] == "window" {
    let pid = Int(CommandLine.arguments[2])!
    let rows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String:Any]] ?? []
    if let row = rows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }), let id = row[kCGWindowNumber as String] as? Int { print(id) }
} else if CommandLine.arguments[1] == "close" {
    func attr(_ e: AXUIElement, _ key: String) -> CFTypeRef? {var value:CFTypeRef?;_=AXUIElementCopyAttributeValue(e,key as CFString,&value);return value}
    func nodes(_ e: AXUIElement,_ depth:Int=0)->[AXUIElement] {if depth>40{return []};return [e]+(attr(e,kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap{nodes($0,depth+1)}}
    let app=AXUIElementCreateApplication(pid_t(CommandLine.arguments[2])!)
    _=nodes(app);Thread.sleep(forTimeInterval:0.4)
    let close=nodes(app).first {e in
        attr(e,kAXRoleAttribute) as? String == kAXButtonRole && [kAXTitleAttribute,kAXDescriptionAttribute].contains {attr(e,$0) as? String == "Close"}
    }
    guard let close else {
        for e in nodes(app) {print("\(attr(e,kAXRoleAttribute) as? String ?? "") \(attr(e,kAXDescriptionAttribute) as? String ?? "") \(attr(e,kAXTitleAttribute) as? String ?? "")")}
        fflush(stdout);fatalError("Sticker picker close control absent")
    }
    precondition(AXUIElementPerformAction(close,kAXPressAction as CFString) == .success)

} else {
    let region = CommandLine.arguments[2]
    func pixels(_ path: String) -> [UInt8] {
        let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil)!
        let image = CGImageSourceCreateImageAtIndex(source, 0, nil)!
        let scale = Double(image.width) / 1200.0
        // Inside the sticker image; excludes toolbar, toast, avatar and scroll bar.
        let rect: CGRect
        switch region {
        case "tgs": rect = CGRect(x:291*scale, y:310*scale, width:72*scale, height:72*scale)
        case "webm": rect = CGRect(x:385*scale, y:310*scale, width:72*scale, height:72*scale)
        case "history-tgs": rect = CGRect(x:340*scale, y:130*scale, width:128*scale, height:128*scale)
        default: rect = CGRect(x:340*scale, y:320*scale, width:128*scale, height:128*scale)
        }
        let crop = image.cropping(to: rect)!
        var bytes = [UInt8](repeating:0, count:crop.width*crop.height*4)
        bytes.withUnsafeMutableBytes { buffer in
            let context = CGContext(data:buffer.baseAddress, width:crop.width, height:crop.height, bitsPerComponent:8, bytesPerRow:crop.width*4, space:CGColorSpaceCreateDeviceRGB(), bitmapInfo:CGImageAlphaInfo.premultipliedLast.rawValue)!
            context.draw(crop, in:CGRect(x:0, y:0, width:crop.width, height:crop.height))
        }
        return bytes
    }
    let first = pixels(CommandLine.arguments[3])
    var changed = false
    for path in CommandLine.arguments.dropFirst(4) {
        let other = pixels(path)
        precondition(first.count == other.count)
        var differing = 0
        for i in stride(from:0, to:first.count, by:4) {
            let red = abs(Int(first[i]) - Int(other[i]))
            let green = abs(Int(first[i+1]) - Int(other[i+1]))
            let blue = abs(Int(first[i+2]) - Int(other[i+2]))
            if red + green + blue > 30 { differing += 1 }
        }
        changed = changed || differing > first.count/240
    }
    precondition(changed == (CommandLine.arguments[1] != "still"), "Incorrect sticker playback for \(region)")
    print("PASS: \(region) \(CommandLine.arguments[1])")
}
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
cd "$ROOT"
for mode in on off; do
  rm -f "$OUT/.quill-ready-ready-sticker-playback"
  QUILL_DEMO_LINGER_MS=30000 QUILL_DEMO_LOOP_STICKERS="$mode" QUILL_RLOTTIE_PATH="${QUILL_RLOTTIE_PATH:-$ROOT/vendor/rlottie/prefix/lib/librlottie.dylib}" "$BINARY" --screenshot-demo ready-sticker-playback "$OUT" > "$OUT/app-$mode.log" 2>&1 &
  APP_PID=$!
  for _ in {1..40}; do [[ -f "$OUT/.quill-ready-ready-sticker-playback" ]] && break; sleep 0.1; done
  WINDOW_ID="$("$TMP/check" window "$APP_PID")"
  [[ -n "$WINDOW_ID" ]]
  for frame in a b c; do screencapture -x -o -l "$WINDOW_ID" "$OUT/$mode-picker-$frame.png"; sleep 0.2; done
  for region in tgs webm; do "$TMP/check" pixels "$region" "$OUT/$mode-picker-a.png" "$OUT/$mode-picker-b.png" "$OUT/$mode-picker-c.png"; done
  if [[ "$mode" == off ]]; then
    sleep 2.3
    for frame in a b c; do screencapture -x -o -l "$WINDOW_ID" "$OUT/off-final-$frame.png"; sleep 0.2; done
    for region in tgs webm; do "$TMP/check" still "$region" "$OUT/off-final-a.png" "$OUT/off-final-b.png" "$OUT/off-final-c.png"; done
  else
    "$TMP/check" close "$APP_PID"
    sleep 0.2
    for frame in a b c; do screencapture -x -o -l "$WINDOW_ID" "$OUT/history-$frame.png"; sleep 0.2; done
    for region in history-tgs history-webm; do "$TMP/check" pixels "$region" "$OUT/history-a.png" "$OUT/history-b.png" "$OUT/history-c.png"; done
  fi
  kill "$APP_PID"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=''
done
printf 'Evidence: %s\n' "$OUT"
