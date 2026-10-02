#!/usr/bin/env bash
# Owned synthetic UI only: no camera access, account or real device login.
set -euo pipefail
BIN="${1:-target/debug/quill}"
TMP="$(mktemp -d /tmp/quill-device-link.XXXXXX)"
PID=''
cleanup() { if [[ -n "$PID" ]]; then kill "$PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }
trap cleanup EXIT
cat > "$TMP/check.swift" <<'SWIFT'
import Cocoa
import ApplicationServices
let pid = pid_t(CommandLine.arguments[1])!
NSRunningApplication(processIdentifier: pid)?.activate(options: [])
func attr(_ e: AXUIElement, _ name: String) -> CFTypeRef? { var v:CFTypeRef?; _=AXUIElementCopyAttributeValue(e,name as CFString,&v); return v }
func nodes(_ e:AXUIElement,_ depth:Int=0)->[AXUIElement] { if depth>30{return []}; return [e]+(attr(e,kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap{nodes($0,depth+1)} }
func name(_ e:AXUIElement)->String { [kAXTitleAttribute,kAXDescriptionAttribute,kAXValueAttribute].compactMap{attr(e,$0) as? String}.joined(separator:" ") }
let app=AXUIElementCreateApplication(pid)
let window=(attr(app,kAXWindowsAttribute) as! [AXUIElement])[0]
_=attr(window,kAXChildrenAttribute);RunLoop.current.run(until:Date(timeIntervalSinceNow:0.5))
let all=nodes(window)
precondition(all.contains{name($0).contains("Allow the device displaying this QR code")})
precondition(all.contains{name($0)=="Link device"})
precondition(!all.contains{name($0).contains("tg://login")},"Login token exposed in accessibility tree")
let cancel=all.first{attr($0,kAXRoleAttribute) as? String==kAXButtonRole && name($0)=="Cancel"}!
precondition(AXUIElementPerformAction(cancel,kAXPressAction as CFString) == .success)
RunLoop.current.run(until:Date(timeIntervalSinceNow:0.5))
let after=nodes(window)
precondition(!after.contains{name($0)=="Link device"})
precondition(after.contains{name($0)=="Link device with camera"})
print("PASS: native confirmation, token privacy and cancellation; camera/account unused")
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
QUILL_DEMO_DEVICE_LINK=1 QUILL_DEMO_LINGER_MS=30000 "$BIN" --screenshot-demo ready-sessions "$TMP" > "$TMP/app.log" 2>&1 &
PID=$!
for _ in {1..100}; do [[ -f "$TMP/.quill-ready-ready-sessions" ]] && break; sleep 0.1; done
"$TMP/check" "$PID"
