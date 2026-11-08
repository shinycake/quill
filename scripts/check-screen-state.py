#!/usr/bin/env python3
"""Every `QuillApp` field is the process's, or the screen's.

docs/decisions/account-scoped-ui-state.md: the UI of the account on screen
lives in one value, `OnScreen` (src/ui/on_screen.rs), declared through
`on_screen_fields!` so that every field names its goodbye. `QuillApp`
keeps only the process's state, and each such field is listed in
scripts/app-wide-state.txt with the reason it survives an account switch.

This check fails when a `QuillApp` field is neither infrastructure nor
listed, when the list names a field that is gone, or when `OnScreen` is
declared outside the macro.

One level down, the same for the chat shown: what a view has open for its
chat is one value, `ChatUi` (src/ui/chat_ui_state.rs), replaced whole when
the chat changes. A field of a per-view struct (the ones a chat window's
`WindowSlot` swaps in) whose type names one chat's ids goes in `ChatUi`,
or is listed in scripts/view-state-not-chat.txt with the reason it may
outlive that chat. A `ChatUi` field declared again in a per-view struct
fails too. Run from the repository root:

    python3 scripts/check-screen-state.py
"""

import re
import sys

APP = "src/ui/app.rs"
SCREEN = "src/ui/on_screen.rs"
LIST = "scripts/app-wide-state.txt"
CHAT_UI = "src/ui/chat_ui_state.rs"
VIEW_LIST = "scripts/view-state-not-chat.txt"

# The per-view structs a chat window's `WindowSlot` swaps in, and where.
VIEW_STRUCTS = {
    "ComposerUi": "src/ui/composer_state.rs",
    "MessageUi": "src/ui/message_state.rs",
    "HistoryUi": "src/ui/history_state.rs",
    "PickerUi": "src/ui/pickers_state.rs",
    "ShareUi": "src/ui/share_state.rs",
    "ViewerUi": "src/ui/viewer_state.rs",
}
# A type that names one chat's ids. A keyed map or set holds every chat's
# entries and is fine where it is.
CHAT_IDS = re.compile(r"\b(ChatId|MessageId|DeleteConfirm|ComposerReplyTo|ComposerEdit)\b")
KEYED = re.compile(r"\b(HashSet|HashMap|BTreeSet|BTreeMap)\b")
TYPED_FIELD = re.compile(
    r"^\s{4}(?:pub(?:\([^)]*\))?\s+)?([a-z_][a-z0-9_]*)\s*:\s*(.+?),?\s*$", re.M
)

# The fields that make the design work, rather than hold state of their own.
INFRASTRUCTURE = {"screen", "live", "demo_session", "credentials"}

FIELD = re.compile(r"^\s{4}pub(?:\([^)]*\))?\s+([a-z_][a-z0-9_]*)\s*:", re.M)


def struct_body(text, name):
    start = text.find(f"struct {name} {{")
    if start < 0:
        return None
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[start:i]
    return None


def app_fields():
    with open(APP, encoding="utf-8") as f:
        body = struct_body(f.read(), "QuillApp")
    if body is None:
        sys.exit(f"{APP}: no `struct QuillApp`")
    return FIELD.findall(body)


def listed_fields(path=LIST):
    out = {}
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            name, _, reason = line.partition("|")
            name, reason = name.strip(), reason.strip()
            if not reason:
                sys.exit(f"{path}:{n}: `{name}` needs a reason after `|`")
            out[name] = reason
    return out


def chat_scope_problems():
    """The chat level: one line per problem."""
    problems = []
    with open(CHAT_UI, encoding="utf-8") as f:
        chat_body = struct_body(f.read(), "ChatUi")
    if chat_body is None:
        return [f"{CHAT_UI}: no `struct ChatUi`"]
    chat_fields = {name for name, _ in TYPED_FIELD.findall(chat_body)}
    listed = listed_fields(VIEW_LIST)
    seen = set()
    for struct, path in VIEW_STRUCTS.items():
        with open(path, encoding="utf-8") as f:
            body = struct_body(f.read(), struct)
        if body is None:
            problems.append(f"{path}: no `struct {struct}`")
            continue
        for name, ty in TYPED_FIELD.findall(body):
            key = f"{struct}.{name}"
            seen.add(key)
            if name in chat_fields:
                problems.append(
                    f"{path}: `{key}` is declared in ChatUi too; the chat's copy is the only one"
                )
                continue
            if not CHAT_IDS.search(ty) or KEYED.search(ty) or key in listed:
                continue
            problems.append(
                f"{path}: `{key}` names one chat's ids but is not in ChatUi "
                f"({CHAT_UI}), which is replaced when the chat shown changes. "
                f"Move it there, or list it in {VIEW_LIST} with the reason "
                f"it may outlive its chat."
            )
    for key in listed:
        if key not in seen:
            problems.append(f"{VIEW_LIST}: `{key}` is listed but no such field exists")
    return problems


def screen_declared_by_macro():
    with open(SCREEN, encoding="utf-8") as f:
        text = f.read()
    if "on_screen_fields! {" not in text:
        return "no `on_screen_fields! {` call"
    # The struct's only declaration is the one inside the macro's body.
    outside = [
        m.start()
        for m in re.finditer(r"^pub(?:\([^)]*\))?\s+struct OnScreen\b", text, re.M)
    ]
    if outside:
        return "`struct OnScreen` is declared outside `on_screen_fields!`"
    return None


def main():
    failed = False
    fields = app_fields()
    listed = listed_fields()
    for name in fields:
        if name in INFRASTRUCTURE or name in listed:
            continue
        print(
            f"{APP}: `{name}` is a QuillApp field but neither infrastructure nor "
            f"listed in {LIST}. The account's UI goes in OnScreen "
            f"({SCREEN}); the process's state is listed with its reason."
        )
        failed = True
    for name in listed:
        if name not in fields:
            print(f"{LIST}: `{name}` is listed but QuillApp has no such field")
            failed = True
    problem = screen_declared_by_macro()
    if problem:
        print(f"{SCREEN}: {problem}")
        failed = True
    chat_problems = chat_scope_problems()
    for problem in chat_problems:
        print(problem)
    if chat_problems:
        failed = True
    if failed:
        sys.exit(1)
    print(f"screen state: OK ({len(fields)} QuillApp fields, {len(listed)} listed)")


if __name__ == "__main__":
    main()
