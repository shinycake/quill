#!/usr/bin/env bash
# Read-only native UI check of account export controls; never exports live data.
set -euo pipefail
BIN="${1:-target/debug/quill}"
OUT="${2:-/tmp/quill-account-export-evidence}"
TMP="$(mktemp -d /tmp/quill-account-export-check.XXXXXX)"
PID=''
cleanup() { if [[ -n "$PID" ]]; then kill "$PID" 2>/dev/null || true; fi; cat "$TMP"/*.log 2>/dev/null || true; rm -rf "$TMP"; }
trap cleanup EXIT
mkdir -p "$OUT"
cat > "$TMP/check.swift" <<'SWIFT'
import Cocoa
import ApplicationServices
import CoreGraphics
let pid = pid_t(CommandLine.arguments[1])!
NSRunningApplication(processIdentifier: pid)?.activate(options: [])
let app = AXUIElementCreateApplication(pid)
func attr(_ e: AXUIElement, _ n: String) -> CFTypeRef? { var value:CFTypeRef?; _=AXUIElementCopyAttributeValue(e,n as CFString,&value);return value }
func nodes(_ e: AXUIElement, _ depth:Int=0) -> [AXUIElement] { if depth>25{return []}; return [e]+(attr(e,kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap{nodes($0,depth+1)} }
func name(_ e:AXUIElement)->String { [kAXTitleAttribute,kAXDescriptionAttribute,kAXValueAttribute].compactMap{attr(e,$0) as? String}.joined(separator:" ") }
let window=(attr(app,kAXWindowsAttribute) as! [AXUIElement])[0]
_=attr(window,kAXChildrenAttribute);Thread.sleep(forTimeInterval:0.5)
let all=nodes(window)
for label in ["Export JSON…", "Export with media…"] {
    precondition(all.contains{name($0)==label}, "Missing export control: \(label)")
}
precondition(all.contains{name($0).contains("Data & Storage")},"Wrong settings destination")
let rows=CGWindowListCopyWindowInfo(.optionOnScreenOnly,kCGNullWindowID) as? [[String:Any]] ?? []
let row=rows.first{$0[kCGWindowOwnerPID as String] as? Int == Int(pid) && $0[kCGWindowLayer as String] as? Int == 0}!
let capture=Process();capture.executableURL=URL(fileURLWithPath:"/usr/sbin/screencapture");capture.arguments=["-x","-o","-l",String(row[kCGWindowNumber as String] as! Int),CommandLine.arguments[3]+"/"+CommandLine.arguments[2]+".png"];try! capture.run();capture.waitUntilExit();precondition(capture.terminationStatus==0)
print("PASS: native \(CommandLine.arguments[2]) account export controls")
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
for DEMO in ready-storage-usage; do
  rm -f "$OUT/.quill-ready-$DEMO"
  QUILL_DEMO_LINGER_MS=30000 "$BIN" --screenshot-demo "$DEMO" "$OUT" > "$TMP/$DEMO.log" 2>&1 &
  PID=$!
  for _ in {1..100}; do
    [[ -f "$OUT/.quill-ready-$DEMO" ]] && break
    sleep 0.1
  done
  "$TMP/check" "$PID" "$DEMO" "$OUT"
  kill "$PID"
  wait "$PID" 2>/dev/null || true
  PID=''
done
