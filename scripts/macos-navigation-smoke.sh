#!/usr/bin/env bash
# Uses injected demos and closes only its own processes; a live app may stay open.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${1:-$ROOT/target/debug/quill}"
OUT="${2:-$(mktemp -d /tmp/quill-navigation-evidence.XXXXXX)}"
[[ "$(uname -s)" == Darwin ]]
mkdir -p "$OUT"
TMP="$(mktemp -d /tmp/quill-navigation-check.XXXXXX)"
APP_PID=''
cleanup() { if [[ -n "$APP_PID" ]]; then kill "$APP_PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }
trap cleanup EXIT
cat > "$TMP/check.swift" <<'SWIFT'
import Cocoa
import ApplicationServices
import CoreGraphics
func attr(_ e: AXUIElement,_ n:String)->CFTypeRef? { var v:CFTypeRef?; _=AXUIElementCopyAttributeValue(e,n as CFString,&v);return v }
func nodes(_ e: AXUIElement,_ d:Int=0)->[AXUIElement] { if d>25{return []}; return [e]+(attr(e,kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap{nodes($0,d+1)} }
func name(_ e:AXUIElement)->String{[kAXTitleAttribute,kAXDescriptionAttribute,kAXValueAttribute].compactMap{attr(e,$0) as? String}.filter{ !$0.isEmpty }.joined(separator:" ")}
func position(_ e:AXUIElement)->CGPoint { var p=CGPoint.zero; if let v=attr(e,kAXPositionAttribute){AXValueGetValue(v as! AXValue,.cgPoint,&p)}; return p }
let pid=pid_t(Int(CommandLine.arguments[1])!)
NSRunningApplication(processIdentifier:pid)?.activate(options:[])
Thread.sleep(forTimeInterval:0.2)
let app=AXUIElementCreateApplication(pid)
let windows=attr(app,kAXWindowsAttribute) as? [AXUIElement] ?? []
precondition(!windows.isEmpty,"Demo window absent")
windows.forEach{_=attr($0,kAXChildrenAttribute)}
Thread.sleep(forTimeInterval:0.4)
func all()->[AXUIElement]{windows.flatMap{nodes($0)}}
func press(_ label:String){guard let e=all().first(where:{name($0)==label}) else{fatalError("Missing \(label)")};precondition(AXUIElementPerformAction(e,kAXPressAction as CFString) == .success);Thread.sleep(forTimeInterval:0.4)}
func capture(_ label:String){
 let rows=CGWindowListCopyWindowInfo(.optionOnScreenOnly,kCGNullWindowID) as? [[String:Any]] ?? []
 let row=rows.first{$0[kCGWindowOwnerPID as String] as? Int == Int(pid) && $0[kCGWindowLayer as String] as? Int == 0}!
 let command=Process();command.executableURL=URL(fileURLWithPath:"/usr/sbin/screencapture");command.arguments=["-x","-o","-l",String(row[kCGWindowNumber as String] as! Int),CommandLine.arguments[3]+"/"+label+".png"];try! command.run();command.waitUntilExit();precondition(command.terminationStatus==0)
}
if CommandLine.arguments[2]=="wait-password" {
 let password=all().first{attr($0,kAXSubroleAttribute) as? String == "AXSecureTextField"}!
 precondition(name(password)=="Two-step verification password")
 precondition(AXUIElementSetAttributeValue(password,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success)
 precondition(AXUIElementSetAttributeValue(password,kAXValueAttribute as CFString,"DemoSecret123" as CFString) == .success,"Cannot edit secure input")
 Thread.sleep(forTimeInterval:0.3)
 precondition(!(attr(password,kAXValueAttribute) as? String ?? "").contains("DemoSecret"),"Password exposed to accessibility")
 precondition(!all().contains{name($0).contains("DemoSecret")},"Password exposed in UI")
 capture("password-masked")
 print("PASS: native secure password input hides typed text and accessibility value")
} else {
 precondition(all().contains{name($0).hasPrefix("Demo chat A —")},"Chat rows absent")
 for label in ["Data and storage","Two-step verification","Appearance","Active sessions","Accounts"] {precondition(!all().contains{name($0)==label},"Settings still clutter sidebar")}
 precondition(!all().contains{attr($0,kAXRoleAttribute) as? String == "AXAlert"},"Internal status became a notification")
 capture("chat-list")
 press("Main menu");press("Settings")
 precondition(all().contains{name($0)=="Privacy and security"})
 capture("settings")
 press("Privacy and security")
 precondition(all().contains{name($0)=="Two-step verification"},"Privacy settings unreachable")
 press("Back to Settings");press("Chat settings")
 precondition(all().contains{name($0)=="Archived stickers"},"Archived stickers unreachable")
 press("Back to Settings");press("Appearance")
 guard let scroll=all().first(where:{name($0)=="Dialog content"}) else { for e in all(){print("\(attr(e,kAXRoleAttribute) ?? "" as CFString) \(name(e))")}; fflush(stdout); fatalError("Dialog scroll body missing") }
 let last=all().last{name($0)=="Reset"}!
 let before=position(last).y
 capture("appearance-top")
 let p=position(scroll)

 let move=CGEvent(mouseEventSource:nil,mouseType:.mouseMoved,mouseCursorPosition:CGPoint(x:p.x+80,y:p.y+80),mouseButton:.left)!
 move.post(tap:.cghidEventTap)
 for _ in 0..<10 {let event=CGEvent(scrollWheelEvent2Source:nil,units:.pixel,wheelCount:1,wheel1:-500,wheel2:0,wheel3:0)!;event.location=CGPoint(x:p.x+80,y:p.y+80);precondition(NSWorkspace.shared.frontmostApplication?.processIdentifier==pid);event.post(tap:.cghidEventTap);Thread.sleep(forTimeInterval:0.08)}
 Thread.sleep(forTimeInterval:0.3)
 let after=position(all().last{name($0)=="Reset"}!).y
 precondition(after<before-100,"Dialog did not scroll: \(before) -> \(after)")
 let origin=position(windows[0])
 precondition(after>origin.y && after<origin.y+500,"Bottom controls still clipped")
 capture("appearance-scrolled")
 press("Close")
 precondition(!all().contains{name($0)=="Dialog content"},"Dialog did not close")
 print("PASS: chat list visible, accessible menu, settings destinations, no status toast, short-window dialog scroll and close")
}
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
cd "$ROOT"
for DEMO in ready-chats wait-password; do
    CASE="$OUT/$DEMO"
    mkdir -p "$CASE"
    rm -f "$CASE/.quill-ready-$DEMO"
    QUILL_DEMO_WINDOW_SIZE=900x500 QUILL_DEMO_LINGER_MS=15000 "$BINARY" --screenshot-demo "$DEMO" "$CASE" > "$CASE/app.log" 2>&1 &
    APP_PID=$!
    for _ in {1..50}; do [[ -f "$CASE/.quill-ready-$DEMO" ]] && break; sleep 0.1; done
    [[ -f "$CASE/.quill-ready-$DEMO" ]]
    "$TMP/check" "$APP_PID" "$DEMO" "$CASE" > "$CASE/check.log" 2>&1
    cat "$CASE/check.log"
    kill "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
    APP_PID=''
done
