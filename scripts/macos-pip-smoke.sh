#!/usr/bin/env bash
# Owned demo only: verify native floating level and the shared playback controls.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${1:-$ROOT/target/debug/quill}"
OUT="${2:-$(mktemp -d /tmp/quill-pip-evidence.XXXXXX)}"
[[ "$(uname -s)" == Darwin ]]
mkdir -p "$OUT"
TMP="$(mktemp -d /tmp/quill-pip-check.XXXXXX)"
APP_PID=''
cleanup() { if [[ -n "$APP_PID" ]]; then kill "$APP_PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }
trap cleanup EXIT
cat > "$TMP/check.swift" <<'SWIFT'
import Cocoa
import ApplicationServices
import CoreGraphics
import ImageIO
func attr(_ e:AXUIElement,_ n:String)->CFTypeRef? { var v:CFTypeRef?;_=AXUIElementCopyAttributeValue(e,n as CFString,&v);return v }
func nodes(_ e:AXUIElement,_ depth:Int=0)->[AXUIElement] { if depth>20{return []};return [e]+(attr(e,kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap{nodes($0,depth+1)} }
func name(_ e:AXUIElement)->String{[kAXTitleAttribute,kAXDescriptionAttribute].compactMap{attr(e,$0) as? String}.filter{ !$0.isEmpty }.joined(separator:" ")}
let pid=pid_t(Int(CommandLine.arguments[1])!)
let app=AXUIElementCreateApplication(pid)
func windows()->[AXUIElement]{attr(app,kAXWindowsAttribute) as? [AXUIElement] ?? []}
windows().forEach{_=attr($0,kAXChildrenAttribute)}
Thread.sleep(forTimeInterval:0.4)
func all()->[AXUIElement]{windows().flatMap{nodes($0)}}
func press(_ label:String){guard let e=all().first(where:{name($0)==label}) else{fatalError("Missing \(label)")};precondition(AXUIElementPerformAction(e,kAXPressAction as CFString) == .success);Thread.sleep(forTimeInterval:0.25)}
func rows()->[[String:Any]]{(CGWindowListCopyWindowInfo(.optionOnScreenOnly,kCGNullWindowID) as? [[String:Any]] ?? []).filter{$0[kCGWindowOwnerPID as String] as? Int == Int(pid)}}
let floating=rows().first{$0[kCGWindowLayer as String] as? Int == 3}!
precondition(rows().contains{$0[kCGWindowLayer as String] as? Int == 0},"Main window disappeared")
if let finder=NSRunningApplication.runningApplications(withBundleIdentifier:"com.apple.finder").first {
 precondition(finder.activate(options:[]));Thread.sleep(forTimeInterval:0.3)
 precondition(rows().contains{$0[kCGWindowLayer as String] as? Int == 3},"PiP vanished when another app became active")
 NSRunningApplication(processIdentifier:pid)?.activate(options:[])
}
precondition(all().contains{name($0)=="Pause video"},"PiP video was not playing")
let windowID=floating[kCGWindowNumber as String] as! Int
func capture(_ label:String)->[UInt8]{
 let path=CommandLine.arguments[2]+"/"+label+".png"
 let command=Process();command.executableURL=URL(fileURLWithPath:"/usr/sbin/screencapture");command.arguments=["-x","-o","-l",String(windowID),path];try! command.run();command.waitUntilExit();precondition(command.terminationStatus==0)
 let source=CGImageSourceCreateWithURL(URL(fileURLWithPath:path) as CFURL,nil)!
 let image=CGImageSourceCreateImageAtIndex(source,0,nil)!
 let crop=image.cropping(to:CGRect(x:20,y:80,width:image.width-40,height:image.height-180))!
 var pixels=[UInt8](repeating:0,count:160*100*4)
 pixels.withUnsafeMutableBytes{buffer in
  let context=CGContext(data:buffer.baseAddress,width:160,height:100,bitsPerComponent:8,bytesPerRow:640,space:CGColorSpaceCreateDeviceRGB(),bitmapInfo:CGImageAlphaInfo.premultipliedLast.rawValue)!
  context.draw(crop,in:CGRect(x:0,y:0,width:160,height:100))
 }
 return pixels
}
let first=capture("playing-a");Thread.sleep(forTimeInterval:0.5);let second=capture("playing-b")
precondition(first != second,"PiP frames did not advance")
press("Pause video")
precondition(all().contains{name($0)=="Play video"},"Pause did not update the shared clock")
let paused=capture("paused-a");Thread.sleep(forTimeInterval:0.4);precondition(paused==capture("paused-b"),"Paused frames changed")
press("Play video")
precondition(all().contains{name($0)=="Pause video"},"Resume failed")
press("Return to viewer")
precondition(!rows().contains{$0[kCGWindowLayer as String] as? Int == 3},"PiP window remained")
precondition(all().contains{name($0)=="Picture-in-Picture"},"Main viewer did not return")
print("PASS: native always-on-top level, advancing video, pause/resume and return to the same viewer")
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
cd "$ROOT"
rm -f "$OUT/.quill-ready-ready-video-pip"
QUILL_DEMO_LINGER_MS=20000 "$BINARY" --screenshot-demo ready-video-pip "$OUT" > "$OUT/app.log" 2>&1 &
APP_PID=$!
for _ in {1..100}; do [[ -f "$OUT/.quill-ready-ready-video-pip" ]] && break; sleep 0.1; done
[[ -f "$OUT/.quill-ready-ready-video-pip" ]]
"$TMP/check" "$APP_PID" "$OUT" > "$OUT/check.log" 2>&1
cat "$OUT/check.log"
