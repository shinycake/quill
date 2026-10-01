#!/usr/bin/env bash
# Verify native AX roles, readable messages, protected spoilers and actions in the demo.
# Usage: scripts/macos-accessibility-smoke.sh [ui-binary] [evidence-directory]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${1:-$ROOT/target/debug/quill}"
OUT="${2:-$(mktemp -d /tmp/quill-ax-evidence.XXXXXX)}"
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
SWIFT
swiftc "$TMP/check.swift" -o "$TMP/check"
rm -f "$OUT/.quill-ready-ready-text-entities"
cd "$ROOT"
"$BINARY" --screenshot-demo ready-text-entities "$OUT" > "$OUT/app.log" 2>&1 &
APP_PID=$!
for _ in {1..40}; do [[ -f "$OUT/.quill-ready-ready-text-entities" ]] && break; sleep 0.1; done
[[ -f "$OUT/.quill-ready-ready-text-entities" ]]
"$TMP/check" "$APP_PID" > "$OUT/accessibility.log" 2>&1
wait "$APP_PID"
APP_PID=''
cat "$OUT/accessibility.log"
printf 'Evidence: %s\n' "$OUT"
