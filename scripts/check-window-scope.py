#!/usr/bin/env python3
"""Async work in src/ui must say which screen and which window it belongs to.

While a chat window is drawn or handles input, its state (composer,
history, viewer, pickers, message menu, share box, the session's view) is
swapped into the shared `QuillApp` fields (`src/ui/chat_window/slot.rs`).
A timer, picker, subscription or background job that runs later finds
whichever window's state is in place by then, so a plain `cx.spawn` /
`cx.defer` whose closure reads or writes that state can land in the wrong
window.

This check flags calls in src/ui that run a closure later (`cx.spawn`,
`spawn_in`, `cx.defer`, `defer_in`, `window.spawn`, `window.defer`,
`on_next_frame`, `cx.subscribe*`, `cx.observe*`, `on_focus*`, `on_blur`)
whose closure touches per-window state:

- one of the swapped fields on the app (`this.history`, `app.viewer`, ...;
  the receiver is `self`, `this`, `app` or the first parameter of any
  closure in the call, so `|q, cx| q.composer` counts too);
- the session's view (`open_chat`, `chat_search`, `viewed_chat()`, ...);
- a driver method that reads or changes the session's view
  (`live.driver.commit_debounced_chat_search()`, `send_voice_note()`):
  `ConnectDriver` and `Session` methods in src are followed the same way,
  so a driver call that only takes ids (`download_file(file_id)`) passes;
- a `QuillApp` method that does any of these, directly or through other
  `QuillApp` methods (matched by name, so a false hit is possible).

Fix one by starting the work with `spawn_scoped` / `spawn_scoped_in`
(it finishes in the window it started in, and not at all once that window
closed), `QuillApp::spawn_screen` / `spawn_screen_in` / `spawn_app` /
`spawn_app_in` (it belongs to no chat window), or by doing the window work
inside `in_window_scope(window, ...)` in the closure (only what is outside
that call is checked). Work that is really the app's, with no window state
involved, says why in a comment on one of the three lines above the call:

    // window-scope: <why this needs no window>

The screen rule (docs/decisions/account-scoped-ui-state.md): a
continuation (`cx.spawn`, `spawn_in`, `cx.defer`, `defer_in`,
`window.spawn`, `window.defer`, `on_next_frame`) acts for the account that
was on screen when it started. One that reads or writes the account on
screen (`live`, `demo_session`, `session()`, `session_mut()`, a field of
`OnScreen`, or a `QuillApp` method that does) must start with
`spawn_scoped*` / `spawn_screen*` / `defer_screen*`, or do that work inside
`in_screen_scope(epoch, ...)` in the closure. A `spawn_app*` body (the
app's own work, which runs whichever account is on screen) must not touch
them at all. The exception marker is

    // screen-scope: <why this acts for whichever account is on screen>

Subscriptions and observers are not continuations: they react to events
and act for the account on screen at event time, and one made on a
screen-owned entity dies with it. The `OnScreen` field list is read from
the `on_screen_fields!` declaration in src/ui/on_screen.rs.

Known limits: a method is matched by name, not type; a field reached
through a local binding (`let h = &mut this.history;` in an outer
function, captured by the closure) or a helper on another type is not
seen; the bodies of `spawn_scoped*` / `spawn_screen*` / `defer_screen*` are
not checked (they name their screen and window by construction).

Usage: python3 scripts/check-window-scope.py [src/ui directory]
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "src" / "ui"

START = re.compile(
    r"\b(cx\.spawn|\.spawn_in|cx\.defer|\.defer_in|window\.spawn|window\.defer"
    r"|on_next_frame|cx\.subscribe(?:_in)?|cx\.observe\w*"
    r"|\.on_focus_in|\.on_focus_out|\.on_focus|\.on_blur)\s*\("
)
# Where a chat window's state is declared and exchanged with the app's.
SLOT_FILE = pathlib.Path("chat_window") / "slot.rs"
SLOT_SWAP = re.compile(
    r"std::mem::swap\(\s*&mut\s+self\.([a-z_][a-z0-9_.]*)\s*,\s*&mut\s+slot\.([a-z_][a-z0-9_]*)\s*\)"
)
# Continuations: a closure that runs later and acts for the account that
# was on screen when it was made.
CONTINUATION = re.compile(
    r"\b(cx\.spawn|\.spawn_in|cx\.defer|\.defer_in|window\.spawn|window\.defer"
    r"|on_next_frame)\s*\("
)
# The app's own work, whose body must not touch the screen's state.
APP_WORK = re.compile(r"\bspawn_app(?:_in)?\s*\(")
ON_SCREEN_FIELD = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?([a-z_][a-z0-9_]*)\s*:", re.M)
SCREEN_ACCESS = r"(?:live|demo_session)\b|session(?:_mut)?\s*\("
SCREEN_MARKER = re.compile(r"//\s*screen-scope:\s*\S")
SCREEN_SCOPE_CALL = re.compile(r"\bin_screen_scope\s*\(")
# The session's view (`Session::swap_view`) and the accessors that read it.
SESSION_VIEW = re.compile(
    r"search_ui\.chat_input|\bopen_chat\b|\bopen_topic\b|threads\.thread\b"
    r"|saved\.(sublist|tag_search)|\bchat_search\b|\bshared_media\b|\bmessages\.instant_view\b"
    r"|\bopen_chat_id\(|\bviewed_chat\(|\bviewed_topic\("
)
# The same view as the session sees it, and as the driver reaches it.
SESSION_SELF_VIEW = re.compile(
    r"\bself\.(?:open_chat|open_topic)\b|\bself\.threads\.(?:thread|saved\.(?:sublist|tag_search))\b"
    r"|\bself\.search\.chat_search\b|\bself\.media\.shared_media\b"
    r"|\bself\.messages\.instant_view\b"
)
DRIVER_SELF_VIEW = re.compile(
    r"\bself\.session\.(?:open_chat|open_topic)\b"
    r"|\bself\.session\.threads\.(?:thread|saved\.(?:sublist|tag_search))\b"
    r"|\bself\.session\.search\.chat_search\b|\bself\.session\.media\.shared_media\b"
    r"|\bself\.session\.messages\.instant_view\b"
)
SELF_CALL = re.compile(r"\bself\.([a-z_0-9]+)\s*\(")
SELF_SESSION_CALL = re.compile(r"\bself\.session\.([a-z_0-9]+)\s*\(")
DRIVER_CALL = re.compile(r"\.driver\.([a-z_0-9]+)\s*\(")
DRIVER_SESSION_CALL = re.compile(r"\.driver\.session\.([a-z_0-9]+)\s*\(")
APP_IMPL = re.compile(r"\bimpl\b[^{;]*?\bQuillApp\s*\{")
SESSION_IMPL = re.compile(r"\bimpl\b[^{;]*?\bSession\s*\{")
DRIVER_IMPL = re.compile(r"\bimpl\b[^{;]*?\bConnectDriver\b[^{;]*\{")
METHOD = re.compile(r"\bfn\s+([a-z_0-9]+)\s*(<[^>]*>)?\s*\(\s*&(mut\s+)?self")
# The first parameter of a closure with two or more: the app, in
# `entity.update(cx, |app, cx| ...)` and the scope helpers.
CLOSURE_APP = re.compile(r"\|\s*(?:mut\s+)?([a-z][a-z0-9_]*)\s*(?::[^,|]*)?,")
CLOSURE_START = re.compile(r"(?:\basync\s+)?(?:\bmove\s+)?\|")
MARKER = re.compile(r"//\s*window-scope:\s*\S")
SCOPE_CALL = re.compile(r"\b(?:in_window_scope|in_window_of)\s*\(")
# The scope machinery itself and GPUI's entity methods, which every
# closure calls.
NOT_WINDOW_STATE = {
    "update",
    "update_in",
    "read_with",
    "read",
    "in_window_scope",
    "set_window_scope",
    "note_window_input",
    "swap_slot",
}
BASE_RECEIVERS = {"self", "this", "app"}


def block_after(text: str, at: int) -> str:
    start = text.find("{", at)
    if start < 0:
        return ""
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[start : i + 1]
    return text[start:]


def call_args(text: str, at: int) -> str:
    depth = 1
    i = at
    while i < len(text) and depth:
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
        i += 1
    return text[at:i]


def without_calls(body: str, call: re.Pattern) -> str:
    """The body with the arguments of every `call` blanked."""
    out = body
    for m in reversed(list(call.finditer(body))):
        args = call_args(body, m.end())
        out = out[: m.end()] + " " * len(args) + out[m.end() + len(args) :]
    return out


def without_scoped_calls(body: str) -> str:
    """The body with the arguments of every `in_window_scope(...)` blanked:
    what runs there has its window's state in place."""
    return without_calls(body, SCOPE_CALL)


def without_screen_calls(body: str) -> str:
    """The body with the arguments of every `in_screen_scope(...)` blanked:
    what runs there runs only on its screen."""
    return without_calls(body, SCREEN_SCOPE_CALL)


def screen_fields(root: pathlib.Path) -> list[str]:
    """The `OnScreen` fields, as `on_screen_fields!` declares them."""
    on_screen = root / "on_screen.rs"
    if not on_screen.exists():
        sys.exit(f"{on_screen} is missing: the OnScreen field list lives there")
    text = on_screen.read_text()
    start = text.find("on_screen_fields! {")
    if start < 0:
        sys.exit(f"{on_screen} has no `on_screen_fields!` declaration")
    body = block_after(text, start)
    return [f for f in ON_SCREEN_FIELD.findall(body) if f not in ("epoch", "client_filled")]


def screen_field_hits(body: str, recv: set[str], fields: list[str]) -> list[str]:
    names = "|".join(re.escape(f) for f in fields)
    pattern = re.compile(
        rf"\b(?:{alternation(recv)})\.(?:(?:{names})\b(?!_)|{SCREEN_ACCESS})"
    )
    return [m.group(0) for m in pattern.finditer(body)]


def closure_part(args: str) -> str:
    """From the first closure on: the arguments before it (the entity
    observed, the window) are evaluated now, not later."""
    m = CLOSURE_START.search(args)
    return args[m.start() :] if m else args


def receivers(body: str) -> set[str]:
    return BASE_RECEIVERS | set(CLOSURE_APP.findall(body))


def alternation(names: set[str]) -> str:
    return "|".join(sorted(re.escape(n) for n in names))


def slot_fields(root: pathlib.Path) -> str:
    """The app fields `swap_slot` exchanges, as a pattern to put after a
    receiver: every field `WindowSlot` declares (`on_screen_fields!` in
    chat_window/slot.rs) that `swap_slot` swaps, by the path it has on the
    app. A field of `OnScreen` (`self.screen.chat_ui`) is also reached
    through `Deref` (`this.chat_ui`). Fails when a declared view field is
    not swapped, or a swap names a field the slot does not declare."""
    path = root / SLOT_FILE
    if not path.exists():
        sys.exit(f"{path} is missing: the WindowSlot declaration lives there")
    text = path.read_text()
    start = text.find("on_screen_fields! {")
    if start < 0:
        sys.exit(f"{path} has no `on_screen_fields!` declaration of WindowSlot")
    body = block_after(text, start)
    # The bookkeeping fields (no goodbye) end at a line holding only `;`.
    parts = re.split(r"\n\s*;\s*\n", body, maxsplit=1)
    if len(parts) != 2:
        sys.exit(f"{path}: the WindowSlot declaration has no `;` line before its view fields")
    bookkeeping, view = parts
    bookkeeping_fields = ON_SCREEN_FIELD.findall(bookkeeping)
    view_fields = ON_SCREEN_FIELD.findall(view)
    swap_at = text.find("fn swap_slot")
    if swap_at < 0:
        sys.exit(f"{path} has no `swap_slot`")
    swaps = {slot: app for app, slot in SLOT_SWAP.findall(block_after(text, swap_at))}
    declared = set(bookkeeping_fields) | set(view_fields)
    problems = [f"`WindowSlot::{f}` is not exchanged in `swap_slot`" for f in view_fields if f not in swaps]
    problems += [f"`swap_slot` exchanges `slot.{f}`, which `WindowSlot` does not declare" for f in swaps if f not in declared]
    if problems:
        sys.exit(f"{path}: " + "; ".join(problems))
    paths = set(swaps.values())
    paths |= {p.removeprefix("screen.") for p in paths if p.startswith("screen.")}
    names = "|".join(re.escape(p) for p in sorted(paths, key=len, reverse=True))
    return rf"(?:{names})\b(?!_)"


SLOT_FIELDS = ""


def slot_field_hits(body: str, recv: set[str]) -> list[str]:
    pattern = re.compile(rf"\b(?:{alternation(recv)})\.{SLOT_FIELDS}")
    return [m.group(0) for m in pattern.finditer(body)]


def calls_on(body: str, recv: set[str]) -> list[str]:
    pattern = re.compile(rf"\b(?:{alternation(recv)})\.([a-z_0-9]+)\s*\(")
    return pattern.findall(body)


def is_test(path: pathlib.Path) -> bool:
    name = path.name
    return (
        "tests" in path.parts
        or name == "tests.rs"
        or name.endswith("_tests.rs")
        or "_tests_" in name
    )


def flat(text: str) -> str:
    """`this\n    .history` reads as `this.history`."""
    return re.sub(r"\s+\.", ".", text)


def methods(text: str, impl_re: re.Pattern) -> list[tuple[str, str]]:
    """(name, body) of every `&self` method in an impl block `impl_re` finds."""
    found = []
    for impl in impl_re.finditer(text):
        block = block_after(text, impl.end() - 1)
        for m in METHOD.finditer(block):
            found.append((m.group(1), flat(block_after(block, m.end()))))
    return found


def closure_of(bodies: dict[str, list[str]], seeded, calls) -> set[str]:
    """Methods for which `seeded(body)` holds, plus those calling one."""
    touching = {
        n
        for n, blocks in bodies.items()
        if n not in NOT_WINDOW_STATE and any(seeded(b) for b in blocks)
    }
    grew = True
    while grew:
        grew = False
        for name, blocks in bodies.items():
            if name in touching or name in NOT_WINDOW_STATE:
                continue
            if any(c in touching for b in blocks for c in calls(b)):
                touching.add(name)
                grew = True
    return touching


def view_methods(src: pathlib.Path) -> tuple[set[str], set[str]]:
    """The `Session` and `ConnectDriver` methods that read or change the
    session's view, directly or through each other."""
    session: dict[str, list[str]] = {}
    driver: dict[str, list[str]] = {}
    for path in sorted(src.rglob("*.rs")):
        if "ui" in path.relative_to(src).parts or is_test(path):
            continue
        text = path.read_text()
        for name, body in methods(text, SESSION_IMPL):
            session.setdefault(name, []).append(body)
        for name, body in methods(text, DRIVER_IMPL):
            driver.setdefault(name, []).append(body)
    session_view = closure_of(
        session, SESSION_SELF_VIEW.search, lambda b: SELF_CALL.findall(b)
    )
    driver_view = closure_of(
        driver,
        lambda b: DRIVER_SELF_VIEW.search(b)
        or any(c in session_view for c in SELF_SESSION_CALL.findall(b)),
        lambda b: SELF_CALL.findall(b),
    )
    return session_view, driver_view


def driver_hits(body: str, session_view: set[str], driver_view: set[str]) -> list[str]:
    hits = [f".driver.{c}()" for c in DRIVER_CALL.findall(body) if c in driver_view]
    hits += [
        f".driver.session.{c}()"
        for c in DRIVER_SESSION_CALL.findall(body)
        if c in session_view
    ]
    return hits


def main() -> int:
    root = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT
    global SLOT_FIELDS
    SLOT_FIELDS = slot_fields(root)
    files = [p for p in sorted(root.rglob("*.rs")) if not is_test(p)]
    texts = {p: p.read_text() for p in files}
    session_view, driver_view = view_methods(root.parent)
    bodies: dict[str, list[str]] = {}
    for text in texts.values():
        for name, body in methods(text, APP_IMPL):
            bodies.setdefault(name, []).append(without_scoped_calls(body))
    touching = closure_of(
        bodies,
        lambda b: bool(
            slot_field_hits(b, receivers(b))
            or SESSION_VIEW.search(b)
            or driver_hits(b, session_view, driver_view)
        ),
        lambda b: calls_on(b, receivers(b)),
    )
    fields = screen_fields(root)
    screen_bodies: dict[str, list[str]] = {}
    for text in texts.values():
        for name, body in methods(text, APP_IMPL):
            screen_bodies.setdefault(name, []).append(without_screen_calls(body))
    touching_screen = closure_of(
        screen_bodies,
        lambda b: bool(screen_field_hits(b, receivers(b), fields)),
        lambda b: calls_on(b, receivers(b)),
    )
    failures = []
    screen_failures = []
    for path, text in texts.items():
        lines = text.split("\n")
        for m in START.finditer(text):
            body = flat(without_scoped_calls(closure_part(call_args(text, m.end()))))
            recv = receivers(body)
            direct = sorted(
                set(slot_field_hits(body, recv))
                | {x.group(0) for x in SESSION_VIEW.finditer(body)}
                | set(driver_hits(body, session_view, driver_view))
            )
            via = sorted({c for c in calls_on(body, recv) if c in touching})
            if not direct and not via:
                continue
            line = text.count("\n", 0, m.start()) + 1
            above = lines[max(0, line - 4) : line]
            if any(MARKER.search(l) for l in above):
                continue
            rel = path.relative_to(root.parent.parent)
            what = ", ".join((direct + [f"{c}()" for c in via])[:4])
            failures.append(f"{rel}:{line}: async work touches window state ({what})")
        for kind, starts in (("continuation", CONTINUATION), ("app work", APP_WORK)):
            for m in starts.finditer(text):
                body = flat(without_screen_calls(closure_part(call_args(text, m.end()))))
                recv = receivers(body)
                direct = sorted(set(screen_field_hits(body, recv, fields)))
                via = sorted({c for c in calls_on(body, recv) if c in touching_screen})
                if not direct and not via:
                    continue
                line = text.count("\n", 0, m.start()) + 1
                above = lines[max(0, line - 4) : line]
                if any(SCREEN_MARKER.search(l) for l in above):
                    continue
                rel = path.relative_to(root.parent.parent)
                what = ", ".join((direct + [f"{c}()" for c in via])[:4])
                screen_failures.append(f"{rel}:{line}: {kind} touches the screen's state ({what})")
    if screen_failures:
        print("\n".join(screen_failures))
        print(
            f"\n{len(screen_failures)} async call(s) in src/ui touch the account on screen without a scope.\n"
            "Start them with spawn_scoped / spawn_screen / defer_screen (and their _in forms:\n"
            "the screen that started the work), do the screen work inside\n"
            "in_screen_scope(epoch, ...) in the closure, keep it out of spawn_app bodies,\n"
            "or, when the work acts for whichever account is on screen, add\n"
            "`// screen-scope: <why>` on one of the three lines above the call. See\n"
            "src/ui/chat_window/scoped.rs and docs/decisions/account-scoped-ui-state.md."
        )
    if failures:
        print("\n".join(failures))
        print(
            f"\n{len(failures)} async call(s) in src/ui touch per-window state without a scope.\n"
            "Start them with spawn_scoped / spawn_scoped_in (the window that started the\n"
            "work), QuillApp::spawn_screen / spawn_app (and _in; no chat window), do the\n"
            "window work inside in_window_scope(window, ...) in the closure, or, when no\n"
            "window state is involved, add `// window-scope: <why>` on one of the three\n"
            "lines above the call. See src/ui/chat_window/scoped.rs and\n"
            "docs/contributing/structure.md (Window scope)."
        )
        return 1
    if screen_failures:
        return 1
    print("window scope: OK")
    print("screen scope: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
