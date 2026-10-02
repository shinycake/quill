#!/usr/bin/env bash
# PID-based native check; a signed-in Quill instance can remain open.
set -euo pipefail
TMP="$(mktemp -d /tmp/quill-tray-check.XXXXXX)"
PID=''
cleanup() { if [[ -n "$PID" ]]; then kill "$PID" 2>/dev/null || true; fi; cat "$TMP/app.log" 2>/dev/null || true; rm -rf "$TMP"; }
trap cleanup EXIT
cat > "$TMP/check.swift" <<'SWIFT'
import Cocoa
import ApplicationServices
import CoreGraphics
let pid = pid_t(CommandLine.arguments[1])!
let app = AXUIElementCreateApplication(pid)
let running = NSRunningApplication(processIdentifier: pid)!
func attr(_ e: AXUIElement, _ n: String) -> CFTypeRef? {
    var v: CFTypeRef?; _ = AXUIElementCopyAttributeValue(e, n as CFString, &v); return v
}
func nodes(_ e: AXUIElement, _ depth: Int = 0) -> [AXUIElement] {
    if depth > 12 { return [] }
    return [e] + (attr(e, kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap { nodes($0, depth + 1) }
}
func press(_ e: AXUIElement) {
    precondition(AXUIElementPerformAction(e, kAXPressAction as CFString) == .success, "Native action failed")
}
func windows() -> [AXUIElement] { attr(app, kAXWindowsAttribute) as? [AXUIElement] ?? [] }
func key(_ code: CGKeyCode) {
    running.activate(options: [])
    RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.3))
    for down in [true, false] {
        let event = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)!
        event.flags = .maskCommand; event.postToPid(pid)
    }
    RunLoop.current.run(until: Date(timeIntervalSinceNow: 2))
}
func tray(_ label: String) {
    // This fixture has a unique bundle name, so System Events cannot
    // canonicalize its PID reference to the user's other Quill instance.
    let command = Process(); command.executableURL = URL(fileURLWithPath:"/usr/bin/osascript")
    command.arguments = ["-e", "tell application \"System Events\"", "-e", "tell (first process whose unix id is \(pid))", "-e", "click menu bar item 1 of menu bar 2", "-e", "click menu item \"\(label)\" of menu 1 of menu bar item 1 of menu bar 2", "-e", "end tell", "-e", "end tell"]
    try! command.run(); command.waitUntilExit(); precondition(command.terminationStatus == 0)
    RunLoop.current.run(until: Date(timeIntervalSinceNow: 2))
}

func button(_ subrole: String) {
    guard let control = nodes(windows()[0]).first(where: { attr($0, kAXSubroleAttribute) as? String == subrole }) else { fatalError("Native window control absent") }
    press(control); RunLoop.current.run(until: Date(timeIntervalSinceNow: 2))
}
precondition(windows().count == 1)
key(46)
precondition(running.isHidden, "Cmd-M did not hide to tray")
tray("Open Quill")
precondition(!running.isHidden && windows().count == 1, "Tray did not restore retained window")
button(kAXMinimizeButtonSubrole)
precondition(running.isHidden, "Native minimize did not hide to tray")
tray("Open Quill")
key(13)
precondition(running.isHidden, "Cmd-W did not hide to tray")
tray("Open Quill")
button(kAXCloseButtonSubrole)
precondition(running.isHidden, "Native close did not hide to tray")
tray("Quit Quill")
precondition(running.isTerminated, "Tray Quit did not terminate owned demo")
print("PASS: Cmd-M, native minimize, tray restore, retained window, Cmd-W, native close, Quit")
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
mkdir -p "$TMP/TrayProof.app/Contents/MacOS"
cp "${1:-target/debug/quill}" "$TMP/TrayProof.app/Contents/MacOS/quill-tray-proof"
cat > "$TMP/TrayProof.app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.shinycake.quill.tray-proof</string>
<key>CFBundleName</key><string>Quill Tray Proof</string>
<key>CFBundleExecutable</key><string>quill-tray-proof</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
PLIST
codesign --force --sign - "$TMP/TrayProof.app" >/dev/null 2>&1
QUILL_DEMO_LINGER_MS=60000 "$TMP/TrayProof.app/Contents/MacOS/quill-tray-proof" --screenshot-demo ready-tray-behavior "$TMP/demo" > "$TMP/app.log" 2>&1 &
PID=$!
for _ in {1..100}; do
    [[ -f "$TMP/demo/.quill-ready-ready-tray-behavior" ]] && break
    sleep 0.1
done
[[ -f "$TMP/demo/.quill-ready-ready-tray-behavior" ]]
"$TMP/check" "$PID"
