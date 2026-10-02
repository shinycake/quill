#!/usr/bin/env bash
# Read-only native UI checks against injected releases; never downloads/installs.
set -euo pipefail
BIN="${1:-target/debug/quill}"
OUT="${2:-/tmp/quill-updater-evidence}"
TMP="$(mktemp -d /tmp/quill-updater-check.XXXXXX)"
PID=''
cleanup() { if [[ -n "$PID" ]]; then kill "$PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }
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
precondition(all.contains{name($0).contains("Update complete: improved navigation")},"Release notes absent")
if CommandLine.arguments[2]=="ready-update-changelog" {
    precondition(all.contains{name($0)=="Dismiss changelog"},"Changelog acknowledgement absent")
    precondition(!all.contains{name($0)=="Download, install and restart"},"Installed version still offers installation")
} else {
    precondition(all.contains{name($0)=="Download, install and restart"},"Explicit install/retry control absent")
    if CommandLine.arguments[2]=="ready-update-failure" { precondition(all.contains{name($0).contains("Download interrupted. Retry the update.")},"Failure state hidden") }
}
let rows=CGWindowListCopyWindowInfo(.optionOnScreenOnly,kCGNullWindowID) as? [[String:Any]] ?? []
let row=rows.first{$0[kCGWindowOwnerPID as String] as? Int == Int(pid) && $0[kCGWindowLayer as String] as? Int == 0}!
let capture=Process();capture.executableURL=URL(fileURLWithPath:"/usr/sbin/screencapture");capture.arguments=["-x","-o","-l",String(row[kCGWindowNumber as String] as! Int),CommandLine.arguments[3]+"/"+CommandLine.arguments[2]+".png"];try! capture.run();capture.waitUntilExit();precondition(capture.terminationStatus==0)
print("PASS: native \(CommandLine.arguments[2]) controls and release notes")
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
for DEMO in ready-update-install ready-update-failure ready-update-changelog; do
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
