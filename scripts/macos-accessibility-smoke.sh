#!/usr/bin/env bash
# Verify native AX roles, readable messages, protected spoilers and actions in the demo.
# Usage: scripts/macos-accessibility-smoke.sh [ui-binary] [evidence-directory] [ready-text-entities|ready-marketplace-gift|ready-unsupported-message]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${1:-$ROOT/target/debug/quill}"
OUT="${2:-$(mktemp -d /tmp/quill-ax-evidence.XXXXXX)}"
DEMO="${3:-ready-text-entities}"
case "$DEMO" in ready-text-entities|ready-marketplace-gift|ready-unsupported-message) ;; *) exit 2 ;; esac
[[ "$(uname -s)" == Darwin ]]
if pgrep -x quill >/dev/null; then echo 'Close the existing Quill process before this isolated smoke check.' >&2; exit 2; fi
mkdir -p "$OUT"
TMP="$(mktemp -d /tmp/quill-ax-smoke.XXXXXX)"
APP_PID=''
cleanup() { if [[ -n "$APP_PID" ]]; then kill "$APP_PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }
trap cleanup EXIT
cat > "$TMP/check.swift" <<'SWIFT'
import ApplicationServices
import Foundation
import CoreGraphics
func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    if AXUIElementCopyAttributeValue(element, name as CFString, &value) != .success { return nil }
    return value
}
func nodes(_ element: AXUIElement, _ depth: Int = 0) -> [AXUIElement] {
    if depth > 16 { return [] }
    return [element] + (attribute(element, kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap { nodes($0, depth+1) }
}
func role(_ element: AXUIElement) -> String { attribute(element, kAXRoleAttribute) as? String ?? "" }
func name(_ element: AXUIElement) -> String {
    [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute].compactMap { attribute(element, $0) as? String }.joined(separator: " ").trimmingCharacters(in: .whitespaces)
}
let app = AXUIElementCreateApplication(pid_t(Int(CommandLine.arguments[1])!))
let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement] ?? []
precondition(!windows.isEmpty, "Demo has no accessible window")
// AccessKit returns its initial root immediately and fills children on the next frame.
for window in windows { _ = attribute(window, kAXChildrenAttribute) }
Thread.sleep(forTimeInterval: 0.4)
let before = windows.flatMap { nodes($0) }
for element in before { print("\(role(element)) \(name(element))") }
if CommandLine.arguments[2] == "ready-marketplace-gift" {
    precondition(before.contains { role($0) == "AXStaticText" && name($0) == "Plush Pepe — PlushPepe-123" }, "Gift quote absent")
    let buy = before.first { role($0) == "AXButton" && name($0) == "Buy for 25 Stars and send to Demo chat A" }!
    let comment = before.first { role($0) == "AXTextArea" && name($0).hasPrefix("Personal comment") }!
    precondition(AXUIElementSetAttributeValue(comment, kAXValueAttribute as CFString, "Native comment" as CFString) == .success)
    let visibility = before.first { role($0) == "AXButton" && name($0) == "Comment and sender: receiver only" }!
    precondition(AXUIElementPerformAction(visibility, kAXPressAction as CFString) == .success)
    Thread.sleep(forTimeInterval:0.3)
    precondition(windows.flatMap { nodes($0) }.contains { role($0) == "AXButton" && name($0) == "Comment and sender: visible to everyone" }, "Visibility did not update")
    precondition(attribute(comment, kAXValueAttribute) as? String == "Native comment")
    let giftName = before.first { role($0) == "AXTextArea" && name($0).hasPrefix("Collectible gift name") }!
    precondition(AXUIElementSetAttributeValue(giftName, kAXValueAttribute as CFString, "PlushPepe-124" as CFString) == .success)
    precondition(AXUIElementPerformAction(buy, kAXPressAction as CFString) == .success)
    Thread.sleep(forTimeInterval:0.3)
    precondition(windows.flatMap { nodes($0) }.contains { name($0) == "Load the newly entered gift before buying." }, "Changed gift name did not refuse purchase")
    let close = windows.flatMap { nodes($0) }.first { role($0) == "AXButton" && name($0) == "Close" }!
    precondition(AXUIElementPerformAction(close, kAXPressAction as CFString) == .success)
    Thread.sleep(forTimeInterval:0.3)
    let menu = windows.flatMap { nodes($0) }.first { role($0) == "AXButton" && name($0) == "Chat actions" }!
    precondition(AXUIElementPerformAction(menu, kAXPressAction as CFString) == .success)
    Thread.sleep(forTimeInterval:0.3)
    let open = windows.flatMap { nodes($0) }.first { role($0) == "AXMenuItem" && name($0) == "Send collectible gift" }!
    precondition(AXUIElementPerformAction(open, kAXPressAction as CFString) == .success)
    Thread.sleep(forTimeInterval:0.3)
    let fresh = windows.flatMap { nodes($0) }
    let freshName = fresh.first { role($0) == "AXTextArea" && name($0).hasPrefix("Collectible gift name") }!
    precondition(attribute(freshName,kAXValueAttribute) as? String == "", "Gift draft not cleared on reopen")
    precondition(!fresh.contains { name($0).hasPrefix("Buy for 25 Stars") }, "Old quote remained on reopen")
    print("PASS: native quote/price, comment editing, visibility, stale-gift refusal and fresh-dialog clearing")
} else if CommandLine.arguments[2] == "ready-unsupported-message" {
    precondition(before.contains { role($0) == "AXStaticText" && name($0) == "Quill cannot display this message. A newer release may support it." }, "Unsupported message card absent")
    precondition(before.filter { role($0) == "AXButton" && name($0) == "Get latest Quill" }.count == 1, "Expected one release action, with none on expired media")
    precondition(before.contains { role($0) == "AXStaticText" && name($0) == "This message has expired." }, "Expired-media notice absent")
    precondition(!before.contains { name($0).contains("messageFutureFeature") }, "Raw API constructor shown to user")
    print("PASS: unsupported card, accessible release action, expiry notice without update action and no raw API constructor")
} else {
precondition(before.contains { role($0) == "AXStaticText" && name($0) == "Bold" }, "Message text absent")
precondition(before.contains { role($0) == "AXButton" && name($0) == "Bold" }, "Formatting label absent")
precondition(before.contains { role($0) == "AXButton" && name($0) == "Manage chat folders" }, "Folder menu label absent")
precondition(!before.contains { role($0) == "AXStaticText" && name($0).isEmpty }, "Empty text node exposed")
precondition(before.contains { role($0) == "AXLink" && name($0) == "https://example.com" }, "Link semantics absent")
precondition(!before.contains { name($0) == "secret" }, "Hidden spoiler exposed")
let spoiler = before.first { role($0) == "AXButton" && name($0) == "Reveal spoiler" }!
precondition(AXUIElementPerformAction(spoiler, kAXPressAction as CFString) == .success, "Cannot activate spoiler")
Thread.sleep(forTimeInterval: 0.3)
let after = windows.flatMap { nodes($0) }
precondition(after.contains { role($0) == "AXStaticText" && name($0) == "secret" }, "AX action did not reveal spoiler")
let composer = after.first { role($0) == "AXTextArea" && name($0).hasPrefix("Message") }!
precondition(AXUIElementSetAttributeValue(composer, kAXValueAttribute as CFString, "Accessible draft" as CFString) == .success, "Cannot edit composer through AX")
Thread.sleep(forTimeInterval: 0.3)
precondition(attribute(composer, kAXValueAttribute) as? String == "Accessible draft", "Composer did not retain AX edit")
print("PASS: native message/link roles, named controls, protected spoiler activation and composer editing")
}
let pid = Int(CommandLine.arguments[1])!
let rows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String:Any]] ?? []
if let row = rows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }), let id = row[kCGWindowNumber as String] as? Int { print("WINDOW_ID:\(id)") }
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
rm -f "$OUT/.quill-ready-$DEMO"
cd "$ROOT"
"$BINARY" --screenshot-demo "$DEMO" "$OUT" > "$OUT/app.log" 2>&1 &
APP_PID=$!
for _ in {1..40}; do [[ -f "$OUT/.quill-ready-$DEMO" ]] && break; sleep 0.1; done
[[ -f "$OUT/.quill-ready-$DEMO" ]]
"$TMP/check" "$APP_PID" "$DEMO" > "$OUT/accessibility.log" 2>&1
WINDOW_ID="$(sed -n 's/^WINDOW_ID://p' "$OUT/accessibility.log")"
[[ -n "$WINDOW_ID" ]]
screencapture -x -o -l "$WINDOW_ID" "$OUT/window.png"
wait "$APP_PID"
APP_PID=''
cat "$OUT/accessibility.log"
printf 'Evidence: %s\n' "$OUT"
