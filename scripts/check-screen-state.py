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

A listed field is a struct, and a reason for the whole struct hid the
account-bound parts inside it (the Account dialog, call ids, the share
chooser's flag; round 6 of the review). So every field of a listed
struct that is defined in src/ui, and of the src/ui structs it holds in
turn, is justified one by one: its type is keyed by its owner
(`AccountKey`, `OfAccount`, `AccountChat`, `ConversationKey`, `OwnedBy`),
or it is listed in scripts/app-wide-fields.txt with its reason. A field
listed there with `*` after its name covers the struct it holds too.

One level down, the same for the chat shown: what a view has open for its
chat is one value, `ChatUi` (src/ui/chat_ui_state.rs), replaced whole when
the chat changes. A field of a per-view struct (the ones a chat window's
`WindowSlot` swaps in), or of a src/ui struct one of them holds, whose type
names one chat's ids (typed, or a raw `i64`) goes in `ChatUi`, or is listed
in scripts/view-state-not-chat.txt with the reason it may outlive that
chat. A `ChatUi` field declared again in a per-view struct fails too.

Both levels look through `Option`, `Vec`, `VecDeque`, map values and the
payloads of src/ui enums (round 7: an `Option<SchedulePicker>` whose
target enum held a message id slipped past). Under a keyed map or set the
chat check stands down; under a value keyed by its account, the account
check does.

A dialog whose `DialogSpec` predicate reads the `Session` is listed in
scripts/session-dialogs.txt with the closer a farewell calls
(scripts/session_dialogs.py; round 10: eight such boxes came back on
switch-back).

Run from the repository root:

    python3 scripts/check-screen-state.py [--root DIR]
"""

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import session_dialogs  # noqa: E402

APP = "src/ui/app.rs"
SCREEN = "src/ui/on_screen.rs"
LIST = "scripts/app-wide-state.txt"
FIELD_LIST = "scripts/app-wide-fields.txt"
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
# A type that names one chat's ids: typed, or TDLib's raw `int53` ids. A
# keyed map or set holds every chat's entries and is fine where it is.
CHAT_IDS = re.compile(
    r"\b(ChatId|MessageId|MessageKey|ChatMessage|DeleteConfirm|ComposerReplyTo|ComposerEdit|i64)\b"
)
KEYED = re.compile(r"\b(HashSet|HashMap|BTreeSet|BTreeMap)\b")
# A type keyed by the account it belongs to.
OWNER_KEYED = re.compile(r"\b(AccountKey|OfAccount|AccountChat|ConversationKey|OwnedBy)\b")

# The fields that make the design work, rather than hold state of their own.
INFRASTRUCTURE = {"screen", "live", "demo_session", "credentials"}

FIELD = re.compile(
    r"^(?:pub(?:\([^)]*\))?\s+)?([a-z_][a-z0-9_]*)\s*:\s*(.+)$", re.S
)


def struct_body(text, name):
    """The text between `struct <name> {` and its closing brace."""
    match = re.search(rf"\bstruct {name}\s*(?:<[^{{]*>)?\s*\{{", text)
    if match is None:
        return None
    start = match.end()
    depth = 1
    for i in range(start, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[start:i]
    return None


def strip_noise(body):
    """The struct body without comments and attributes."""
    body = re.sub(r"//[^\n]*", "", body)
    out = []
    i = 0
    while i < len(body):
        if body.startswith("#[", i):
            depth = 0
            while i < len(body):
                if body[i] == "[":
                    depth += 1
                elif body[i] == "]":
                    depth -= 1
                    if depth == 0:
                        i += 1
                        break
                i += 1
            continue
        out.append(body[i])
        i += 1
    return "".join(out)


def struct_fields(body):
    """`(name, type)` for each field, types on as many lines as they take."""
    body = strip_noise(body)
    fields, depth, part = [], 0, []
    for i, ch in enumerate(body):
        if ch in "<([{":
            depth += 1
        elif ch in ">)]}" and not (ch == ">" and i > 0 and body[i - 1] == "-"):
            depth -= 1
        if ch == "," and depth == 0:
            fields.append("".join(part))
            part = []
        else:
            part.append(ch)
    fields.append("".join(part))
    out = []
    for chunk in fields:
        chunk = chunk.strip()
        if not chunk:
            continue
        match = FIELD.match(chunk)
        if match:
            out.append((match.group(1), " ".join(match.group(2).split())))
    return out


def split_top(text):
    """`text` split at its top-level commas."""
    parts, depth, part = [], 0, []
    for i, ch in enumerate(text):
        if ch in "<([{":
            depth += 1
        elif ch in ">)]}" and not (ch == ">" and i > 0 and text[i - 1] == "-"):
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(part).strip())
            part = []
        else:
            part.append(ch)
    parts.append("".join(part).strip())
    return [p for p in parts if p]


def enum_body(text, name):
    """The text between `enum <name> {` and its closing brace."""
    return struct_body(text.replace(f"enum {name}", f"struct {name}"), name)


def enum_fields(body):
    """`(Variant.field, type)` for each payload of an enum: `Variant.0` for
    a tuple variant's types, `Variant.name` for a struct variant's fields."""
    out = []
    for chunk in split_top(strip_noise(body)):
        match = re.match(r"([A-Z][A-Za-z0-9_]*)\s*(.*)$", chunk, re.S)
        if not match:
            continue
        variant, rest = match.group(1), match.group(2).strip()
        if rest.startswith("(") and rest.endswith(")"):
            for n, ty in enumerate(split_top(rest[1:-1])):
                out.append((f"{variant}.{n}", " ".join(ty.split())))
        elif rest.startswith("{") and rest.endswith("}"):
            for field, ty in struct_fields(rest[1:-1]):
                out.append((f"{variant}.{field}", ty))
    return out


class Structs:
    """Every `struct Name {` under src/, by name."""

    def __init__(self):
        self.by_name = {}
        self.enums = set()
        for root, _, names in os.walk("src"):
            for name in names:
                if not name.endswith(".rs"):
                    continue
                path = os.path.join(root, name)
                with open(path, encoding="utf-8") as f:
                    text = f.read()
                for match in re.finditer(
                    r"\b(struct|enum) ([A-Z][A-Za-z0-9_]*)\s*(?:<[^{;]*>)?\s*\{", text
                ):
                    self.by_name.setdefault(match.group(2), []).append(path)
                    if match.group(1) == "enum":
                        self.enums.add((match.group(2), path))

    def ui_struct(self, ty):
        """The src/ui struct or enum a field's type is, with its file, or
        `None` (a std, kit or core type, an entity handle: those are
        leaves). `Box`, `Rc`, `RefCell`, `Option`, `Vec`, `VecDeque` and a
        map's values are looked through: an `Option<Picker>` holds the
        picker's fields as much as a `Picker` does (round 7, the Reschedule
        picker)."""
        ty = ty.strip()
        for wrapper in (
            "Box<", "std::rc::Rc<", "Rc<", "std::cell::RefCell<", "RefCell<",
            "Option<", "Vec<", "VecDeque<", "std::collections::VecDeque<",
        ):
            if ty.startswith(wrapper) and ty.endswith(">"):
                return self.ui_struct(ty[len(wrapper):-1])
        for wrapper in ("HashMap<", "BTreeMap<", "std::collections::HashMap<"):
            if ty.startswith(wrapper) and ty.endswith(">"):
                args = split_top(ty[len(wrapper):-1])
                return self.ui_struct(args[-1]) if len(args) == 2 else None
        if not re.fullmatch(r"(?:[a-z_][a-z0-9_]*::)*[A-Z][A-Za-z0-9_]*", ty):
            return None
        name = ty.rsplit("::", 1)[-1]
        paths = [p for p in self.by_name.get(name, []) if p.startswith("src/ui/")]
        if len(paths) != 1:
            # Two src/ui structs of one name: the module path decides.
            module = ty.rsplit("::", 1)[0].replace("super::", "").replace("crate::ui::", "")
            paths = [p for p in paths if module and module.replace("::", "/") in p]
        if len(paths) != 1:
            return None
        return name, paths[0]


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def app_typed_fields():
    body = struct_body(read(APP), "QuillApp")
    if body is None:
        sys.exit(f"{APP}: no `struct QuillApp`")
    return struct_fields(body)


def app_fields():
    return [name for name, _ in app_typed_fields()]


def listed_fields(path=LIST):
    out = {}
    if not os.path.exists(path):
        return out
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


def walk_struct(structs, prefix, name, path, seen, outer=()):
    """`(path.to.field, type, file, outer)` for every field of `name`, and
    of the src/ui structs and enums its fields hold, depth first. `outer`:
    the types of the fields it sits under."""
    text = read(path)
    if (name, path) in structs.enums:
        body = enum_body(text, name)
        fields = enum_fields(body) if body is not None else []
    else:
        body = struct_body(text, name)
        fields = struct_fields(body) if body is not None else []
    for field, ty in fields:
        key = f"{prefix}.{field}"
        yield key, ty, path, outer
        inner = structs.ui_struct(ty)
        if inner and inner[0] not in seen:
            yield from walk_struct(
                structs, key, inner[0], inner[1], seen | {inner[0]}, outer + (ty,)
            )


def app_wide_subfields(structs=None):
    """Every field inside the listed app-wide structs: `(key, type, file)`."""
    structs = structs or Structs()
    listed = listed_fields()
    for name, ty in app_typed_fields():
        if name not in listed:
            continue
        found = structs.ui_struct(ty)
        if found:
            yield from walk_struct(structs, name, found[0], found[1], {found[0]})


def app_wide_field_problems(structs):
    problems = []
    justified = listed_fields(FIELD_LIST)
    covered = [key[:-1].rstrip() for key in justified if key.endswith("*")]
    seen = set()
    for key, ty, path, outer in app_wide_subfields(structs):
        seen.update(prefixes(key))
        if any(key.startswith(whole + ".") for whole in covered):
            continue
        if key in justified or f"{key} *" in justified or f"{key}*" in justified:
            continue
        # Under a value keyed by its owner (a map by account, an account's
        # client), the owner key covers it.
        if OWNER_KEYED.search(ty) or any(OWNER_KEYED.search(o) for o in outer):
            continue
        problems.append(
            f"{path}: `{key}` ({ty}) is inside an app-wide struct but neither keyed "
            f"by its account nor listed in {FIELD_LIST}. What belongs to the account "
            f"on screen goes in OnScreen ({SCREEN}); what the process keeps is listed "
            f"with its reason."
        )
    for key in justified:
        bare = key[:-1].rstrip() if key.endswith("*") else key
        if bare not in seen:
            problems.append(f"{FIELD_LIST}: `{key}` is listed but no such field exists")
    return problems


def prefixes(key):
    """`a.b.c` and the paths above it: `a.b`, `a` (an enum variant is a
    path of its own)."""
    parts = key.split(".")
    return {".".join(parts[: n + 1]) for n in range(len(parts))}


def covered(key, listed):
    """`key` is listed, or sits under a field listed with `*`."""
    if key in listed:
        return True
    return any(
        name.endswith("*") and key.startswith(name[:-1].rstrip() + ".")
        for name in listed
    )


def chat_scope_problems(structs):
    """The chat level: one line per problem."""
    problems = []
    chat_body = struct_body(read(CHAT_UI), "ChatUi")
    if chat_body is None:
        return [f"{CHAT_UI}: no `struct ChatUi`"]
    chat_fields = {name for name, _ in struct_fields(chat_body)}
    listed = listed_fields(VIEW_LIST)
    seen = set()
    for struct, path in VIEW_STRUCTS.items():
        if struct_body(read(path), struct) is None:
            problems.append(f"{path}: no `struct {struct}`")
            continue
        for key, ty, at, outer in walk_struct(structs, struct, struct, path, {struct}):
            seen.update(prefixes(key))
            top = key.count(".") == 1
            name = key.rsplit(".", 1)[-1]
            if top and name in chat_fields:
                problems.append(
                    f"{at}: `{key}` is declared in ChatUi too; the chat's copy is the only one"
                )
                continue
            # A keyed map or set, or anything under one, holds every
            # chat's entries.
            keyed = KEYED.search(ty) or any(KEYED.search(o) for o in outer)
            if not CHAT_IDS.search(ty) or keyed or covered(key, listed):
                continue
            problems.append(
                f"{at}: `{key}` ({ty}) names one chat's ids but is not in ChatUi "
                f"({CHAT_UI}), which is replaced when the chat shown changes. "
                f"Move it there, or list it in {VIEW_LIST} with the reason "
                f"it may outlive its chat."
            )
    for key in listed:
        bare = key[:-1].rstrip() if key.endswith("*") else key
        if bare not in seen:
            problems.append(f"{VIEW_LIST}: `{key}` is listed but no such field exists")
    return problems


def screen_declared_by_macro():
    text = read(SCREEN)
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
    if "--root" in sys.argv:
        os.chdir(sys.argv[sys.argv.index("--root") + 1])
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
    structs = Structs()
    for problem in app_wide_field_problems(structs) + chat_scope_problems(structs):
        print(problem)
        failed = True
    dialog_problems, session_backed = session_dialogs.problems()
    for problem in dialog_problems:
        print(problem)
        failed = True
    if failed:
        sys.exit(1)
    print(
        f"screen state: OK ({len(fields)} QuillApp fields, {len(listed)} listed, "
        f"{len(listed_fields(FIELD_LIST))} app-wide sub-fields justified, "
        f"{session_backed} session-held dialogs closed)"
    )


if __name__ == "__main__":
    main()
