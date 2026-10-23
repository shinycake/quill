#!/usr/bin/env bash
# Verify native AX roles, readable messages, protected spoilers and actions in the demo.
# Usage: scripts/macos-accessibility-smoke.sh [ui-binary] [evidence-directory] [demo|flows]
# `flows` walks the main flows one demo at a time: chat list, conversation and
# composer, an animating chat (the tree must stay whole), the media viewer,
# settings, group info and a dialog. Every flow also fails on an unnamed
# control (window chrome and the kit tab strip's overflow button aside).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${1:-$ROOT/target/debug/quill}"
OUT="${2:-$(mktemp -d /tmp/quill-ax-evidence.XXXXXX)}"
DEMO="${3:-ready-text-entities}"
FLOWS="ready-chats-composer ready-typing ready-media ready-privacy ready-group-info-edit ready-profile-edit"
if [[ "$DEMO" == flows ]]; then
  for flow in $FLOWS; do
    "$0" "$BINARY" "$OUT/$flow" "$flow"
  done
  exit 0
fi
case " ready-text-entities ready-marketplace-gift ready-unsupported-message ready-emoji-packs $FLOWS " in *" $DEMO "*) ;; *) exit 2 ;; esac
[[ "$(uname -s)" == Darwin ]]
# Every accessibility read, action and screenshot targets the owned demo PID.
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
func nodes(_ element: AXUIElement, _ depth: Int = 0, limit: Int = 16) -> [AXUIElement] {
    if depth > limit { return [] }
    return [element] + (attribute(element, kAXChildrenAttribute) as? [AXUIElement] ?? []).flatMap { nodes($0, depth+1, limit: limit) }
}
func role(_ element: AXUIElement) -> String { attribute(element, kAXRoleAttribute) as? String ?? "" }
func name(_ element: AXUIElement) -> String {
    [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute].compactMap { attribute(element, $0) as? String }.joined(separator: " ").trimmingCharacters(in: .whitespaces)
}
let app = AXUIElementCreateApplication(pid_t(Int(CommandLine.arguments[1])!))
_ = AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
Thread.sleep(forTimeInterval: 0.2)
let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement] ?? []
if windows.isEmpty {
    print("APP_CHILDREN: \((attribute(app, kAXChildrenAttribute) as? [AXUIElement] ?? []).map { role($0) })")
    fflush(stdout)
}
precondition(!windows.isEmpty, "Demo has no accessible window")
// AccessKit returns its initial root immediately and fills children on the next frame.
for window in windows { _ = attribute(window, kAXChildrenAttribute) }
Thread.sleep(forTimeInterval: 0.4)
let before = windows.flatMap { nodes($0) }
for element in before { print("\(role(element)) \(name(element))") }
fflush(stdout)
func has(_ elements: [AXUIElement], _ wantRole: String, _ wantName: String) -> Bool {
    elements.contains { role($0) == wantRole && (name($0) == wantName || (wantName.hasSuffix("*") && name($0).hasPrefix(String(wantName.dropLast())))) }
}
func press(_ elements: [AXUIElement], _ wantName: String) {
    let hit = elements.first { role($0) == "AXButton" && name($0) == wantName }
    precondition(hit != nil, "No button named \(wantName)")
    precondition(AXUIElementPerformAction(hit!, kAXPressAction as CFString) == .success, "Cannot press \(wantName)")
    Thread.sleep(forTimeInterval: 0.4)
}
// Named controls everywhere in a flow. Exempt: the window's own title-bar
// buttons (direct window children) and the kit tab strip's overflow button
// (no action; the kit gives it no name).
func unnamedControls() -> [String] {
    let interactive: Set<String> = ["AXButton", "AXLink", "AXCheckBox", "AXRadioButton", "AXMenuItem", "AXTextArea", "AXTextField", "AXSlider", "AXComboBox"]
    var found: [String] = []
    func walk(_ element: AXUIElement, _ parentRole: String, _ depth: Int) {
        let kids = attribute(element, kAXChildrenAttribute) as? [AXUIElement] ?? []
        for kid in kids {
            let kidRole = role(kid)
            var actions: CFArray?
            _ = AXUIElementCopyActionNames(kid, &actions)
            let pressable = ((actions as? [String]) ?? []).contains(kAXPressAction as String)
            let chrome = depth == 0 && kidRole == "AXButton"
            let overflow = parentRole == "AXTabGroup" && kidRole == "AXButton" && !pressable
            if interactive.contains(kidRole) && name(kid).isEmpty && !chrome && !overflow { found.append(kidRole) }
            if depth < 40 { walk(kid, kidRole, depth + 1) }
        }
    }
    for window in windows { walk(window, "AXWindow", 0) }
    return found
}
let flow = CommandLine.arguments[2]
if " \(CommandLine.arguments[3]) ".contains(" \(flow) ") {
    let unnamed = unnamedControls()
    precondition(unnamed.isEmpty, "Unnamed controls: \(unnamed)")
    switch flow {
    case "ready-chats-composer", "ready-typing":
        if flow == "ready-typing" {
            // The typing dots animate: frames where only the animation layer
            // redraws must still carry the chat list, history and composer.
            Thread.sleep(forTimeInterval: 1.5)
        }
        let now = windows.flatMap { nodes($0) }
        for (wantRole, wantName) in [("AXButton", "Search"), ("AXButton", "Main menu"), ("AXRadioButton", "Chats"), ("AXButton", "Demo chat A — *"), ("AXGroup", "Message history — Demo chat A"), ("AXButton", "Chat actions"), ("AXButton", "Attach"), ("AXTextArea", "Message*")] {
            precondition(has(now, wantRole, wantName), "\(flow): missing \(wantRole) \(wantName)")
        }
        print("PASS: \(flow): chat list, history and composer named and present")
        if flow == "ready-chats-composer" {
            // Focus entering the history picks a message, as tdesktop's
            // announceAccessibilityFocusedChild does; its full name ends in
            // the delivery line. GPUI's view answers the app-level
            // AXFocusedUIElement with the window, so read the focused node
            // from the tree; the focus-in pick lands on the next frame.
            let history = now.first { role($0) == "AXGroup" && name($0) == "Message history — Demo chat A" }!
            var spoken = ""
            for _ in 0..<20 {
                // AccessKit reports focus only while the window is key; keep
                // the demo in front in case another app took focus.
                _ = AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
                if spoken.isEmpty {
                    precondition(AXUIElementSetAttributeValue(history, kAXFocusedAttribute as CFString, kCFBooleanTrue) == .success, "Cannot focus the history")
                }
                Thread.sleep(forTimeInterval: 0.2)
                let focusedNodes = windows.flatMap { nodes($0, limit: 40) }.filter { role($0) != "AXWindow" && (attribute($0, kAXFocusedAttribute) as? Bool) == true }
                spoken = focusedNodes.map { "\(role($0)) \(name($0))" }.joined(separator: "; ")
                if spoken.contains("Received") || spoken.contains("Sent") { break }
            }
            print("FOCUSED: \(spoken.replacingOccurrences(of: "\n", with: " | "))")
            fflush(stdout)
            precondition(spoken.contains("Received") || spoken.contains("Sent"), "History focus did not land on a message")
            print("PASS: history focus reads a whole message")
        }
    case "ready-media":
        precondition(has(before, "AXButton", "Jump to bottom, 1"), "Jump to bottom unnamed")
        press(before, "Open photo")
        let viewer = windows.flatMap { nodes($0) }
        for wantName in ["Close media viewer", "Zoom In", "Zoom Out", "Save", "Share"] {
            precondition(has(viewer, "AXButton", wantName), "Media viewer: missing \(wantName)")
        }
        press(viewer, "Close media viewer")
        precondition(!has(windows.flatMap { nodes($0) }, "AXButton", "Close media viewer"), "Viewer did not close")
        print("PASS: media viewer opened, named and closed through AX")
    case "ready-privacy":
        precondition(has(before, "AXButton", "Phone Number · *"), "Privacy rows unnamed")
        precondition(has(before, "AXButton", "Close"), "Settings close unnamed")
        print("PASS: settings rows named")
    case "ready-group-info-edit":
        precondition(has(before, "AXCheckBox", "Approve new members"), "Group toggle switch unnamed")
        print("PASS: group info toggles named")
    case "ready-profile-edit":
        precondition(has(before, "AXTextArea", "First name*") || has(before, "AXTextField", "First name*"), "Profile dialog field unnamed")
        print("PASS: profile dialog fields named")
    default:
        break
    }
} else if CommandLine.arguments[2] == "ready-marketplace-gift" {
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
} else if CommandLine.arguments[2] == "ready-emoji-packs" {
    let packs = ["Animals", "Fun faces", "Food and drink"]
    let order = before.filter { role($0) == "AXButton" && packs.contains(name($0)) }.map { name($0) }
    precondition(order == packs, "Packs not listed most recent first: \(order)")
    for pack in packs {
        precondition(before.contains { role($0) == "AXButton" && name($0) == "Remove \(pack)" }, "Missing remove button: \(pack)")
    }
    for label in ["Search", "Retry", "Download or retry pack", "Dynamic emoji pack order"] {
        precondition(!before.contains { name($0).hasPrefix(label) && role($0) == "AXButton" }, "Manual control still shown: \(label)")
    }
    print("PASS: pack rows named, most recent first, with remove buttons and no manual load, search or retry controls")
} else {
precondition(before.contains { role($0) == "AXStaticText" && name($0).hasPrefix("Bold italic") }, "Message text absent")
precondition(before.contains { role($0) == "AXButton" && name($0) == "Formatting" }, "Formatting label absent")
precondition(before.contains { role($0) == "AXButton" && name($0) == "Manage chat folders" }, "Folder menu label absent")
precondition(!before.contains { role($0) == "AXStaticText" && name($0).isEmpty }, "Empty text node exposed")
// The message reads as one text, spoilers included, as tdesktop's accessible
// message name does (`originalText()`, history_inner_widget_accessibility.cpp).
precondition(before.contains { role($0) == "AXStaticText" && name($0).contains("secret") && name($0).contains("https://example.com") }, "Message text not read whole")
let composer = before.first { role($0) == "AXTextArea" && name($0).hasPrefix("Message") }!
precondition(AXUIElementSetAttributeValue(composer, kAXValueAttribute as CFString, "Accessible draft" as CFString) == .success, "Cannot edit composer through AX")
Thread.sleep(forTimeInterval: 0.3)
precondition(attribute(composer, kAXValueAttribute) as? String == "Accessible draft", "Composer did not retain AX edit")
print("PASS: message text read whole, named controls and composer editing")
}
let pid = Int(CommandLine.arguments[1])!
let rows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String:Any]] ?? []
if let row = rows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }), let id = row[kCGWindowNumber as String] as? Int { print("WINDOW_ID:\(id)") }
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
rm -f "$OUT/.quill-ready-$DEMO"
cd "$ROOT"
QUILL_DEMO_LINGER_MS=10000 "$BINARY" --screenshot-demo "$DEMO" "$OUT" > "$OUT/app.log" 2>&1 &
APP_PID=$!
for _ in {1..40}; do [[ -f "$OUT/.quill-ready-$DEMO" ]] && break; sleep 0.1; done
[[ -f "$OUT/.quill-ready-$DEMO" ]]
"$TMP/check" "$APP_PID" "$DEMO" "$FLOWS" > "$OUT/accessibility.log" 2>&1 || { tail -5 "$OUT/accessibility.log"; exit 1; }
WINDOW_ID="$(sed -n 's/^WINDOW_ID://p' "$OUT/accessibility.log")"
[[ -n "$WINDOW_ID" ]]
screencapture -x -o -l "$WINDOW_ID" "$OUT/window.png"
wait "$APP_PID"
APP_PID=''
cat "$OUT/accessibility.log"
printf 'Evidence: %s\n' "$OUT"
