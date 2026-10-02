#!/usr/bin/env bash
# Read-only native accessibility checks of injected fixtures.
set -euo pipefail
BIN="${1:-target/debug/quill}"
OUT="${2:-/tmp/quill-accessibility-evidence}"
TMP="$(mktemp -d /tmp/quill-accessibility-check.XXXXXX)"
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
let expected: [String]
switch CommandLine.arguments[2] {
case "wait-phone": expected=["Phone number"]
case "wait-code": expected=["Sign-in code"]
case "wait-password": expected=["Two-step verification password"]
case "ready-profile-edit": expected=["First name","Last name","Public username","Bio","Edit profile"]
case "ready-sessions": expected=["Active Sessions","Current session","Other sessions"]
case "ready-storage-usage": expected=["Data & Storage","Storage by file type","Storage by chat","Export JSON…","Export with media…"]
default: fatalError("Unknown fixture")
}
for label in expected {
    precondition(all.contains{name($0).contains(label)}, "Missing accessible name: \(label)")
}
let rows=CGWindowListCopyWindowInfo(.optionOnScreenOnly,kCGNullWindowID) as? [[String:Any]] ?? []
let row=rows.first{$0[kCGWindowOwnerPID as String] as? Int == Int(pid) && $0[kCGWindowLayer as String] as? Int == 0}!
let capture=Process();capture.executableURL=URL(fileURLWithPath:"/usr/sbin/screencapture");capture.arguments=["-x","-o","-l",String(row[kCGWindowNumber as String] as! Int),CommandLine.arguments[3]+"/"+CommandLine.arguments[2]+".png"];try! capture.run();capture.waitUntilExit();precondition(capture.terminationStatus==0)
if CommandLine.arguments[2] == "ready-storage-usage" {
    let control=all.first{name($0).contains("Wi-Fi automatic download settings")}!
    precondition(attr(control,kAXRoleAttribute) as? String == kAXButtonRole)
    precondition(AXUIElementPerformAction(control,kAXPressAction as CFString) == .success)
    RunLoop.current.run(until:Date(timeIntervalSinceNow:0.5))
    precondition(nodes(window).contains{name($0)=="Save"},"Native action did not open network editor")
}
print("PASS: native \(CommandLine.arguments[2]) accessible titles and controls")
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
for DEMO in wait-phone wait-code wait-password ready-profile-edit ready-sessions ready-storage-usage; do
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
