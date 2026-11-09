"""Every start the passcode lock must reach asks its gate, or is listed.

Imported by scripts/check-screen-state.py (`problems()`).

The lock is a render state (docs/decisions/lock-is-a-render-state.md):
while locked, every window draws only the lock screen or nothing, so what
is drawn needs no check. What this guard checks is what runs without being
drawn. It claims exactly this, and no more:

1. Starts. In the production code of every file under src/ (test modules,
   `#[cfg(test)]` items and comments removed), each of these calls is a
   start:
   - an OS window (`.open_window(`)
   - a helper process (`Command::new(`, `media_tools::command(`,
     `quiet_command_for(`)
   - a microphone or camera capture (`.begin_capture_start(`,
     `CameraStream::start(`)
   - an app-wide key hook (`intercept_keystrokes(`, `observe_keystrokes(`)
   - an app-wide action handler (`cx.on_action(`)
   - an account switch that skips the lock (`switch_account_even_locked(`)
   The innermost function holding the call must test a gate in a
   condition (an `if`, a match guard or a `return`): `refused_while_locked()`,
   `paused_by_lock()` or `app_locked(`. Otherwise `path::fn` must be listed
   in scripts/lock-surfaces.txt as `path::fn | closer | reason`:
   - `closer` is a method `close_for_lock` calls (it stops the surface);
   - `via:fn` says the function runs only through `fn`, which must itself
     test a gate in a condition (the guard checks `fn`, not the call graph);
   - `-` says, in the reason, why the lock need not reach it.
   A listed entry must still hold a start; `refuses` pins a function that
   must keep testing a gate, for paths no pattern finds (links, launches).
2. One writer. `passcode.lock_state` (locked or not, with the count) is assigned only in
   `set_locked` (src/ui/passcode/lock_gate.rs).
3. The render gate is mounted: `lock_layer.rs` registers `LockLayer` as a
   root plugin, and `PasscodeUi::new` calls `lock_layer::register`.
4. What runs while locked. The functions in `UPKEEP` (the main window's
   frame upkeep and the poll pass) run every frame or pass, locked or not.
   Each must test `lock_waits()` in a condition, and every `self.method(`
   call in it outside the gated part (after an early return on the gate, or
   inside the `if` that tests `!self.lock_waits()`) must be listed in
   scripts/lock-surfaces.txt as `path::fn -> method | runs-locked | reason`.
   So a new step that would show or start something under the lock fails
   here until it goes behind the gate, or is listed with why it may run.
   A listed step that is gone fails too. The locked frames (`LOCKED_FRAMES`)
   must not run the per-window or main frame upkeep at all.

5. Agent effects after a wait. `withheld_if_locked` hides only an agent
   call's answer, so a step with an effect that runs after the call may
   have waited must ask the lock itself. In the production code under
   src/agent/: every function that takes a paced queue turn
   (`pacing::take_turn(`) must call `still_allowed()` after it (that
   check refuses once Quill locked, Agent access went off or the grants
   changed since the call began), so must every write tool that waits
   for the user's click (`prompts::wait(` under src/agent/tools/write/
   and in src/agent/tools/td_call.rs), and every
   function that copies a file to an agent's path (`copy_new(`, outside
   its own definition) must test `locked_since_call_began()` in a
   condition before it. In `agent/server.rs::answer`, the `resources/read`
   arm must test `paused_by_lock()` before `read_resource(`, so no resource
   but the guide answers while locked.

It does not prove that a listed `via` function is the only caller, nor that
a gate's answer is used correctly beyond being tested in a condition, nor
what a `runs-locked` step calls in turn: those are read in review.
"""

import os
import re

LIST = "scripts/lock-surfaces.txt"
GATE_FILE = "src/ui/passcode/lock_gate.rs"
FLAG_FILE = "src/ui/passcode/mod.rs"
FLAG_SETTER = "set_lock_flag"
LAYER_FILE = "src/ui/passcode/lock_layer.rs"
PASSCODE_FILE = "src/ui/passcode/mod.rs"
STARTS = re.compile(
    r"\.open_window\(|\bCommand::new\(|\bmedia_tools::command\(|\bquiet_command_for\("
    r"|\.begin_capture_start\(|\bCameraStream::start\(|\bintercept_keystrokes\("
    r"|\bobserve_keystrokes\(|\bcx\.on_action\(|\bswitch_account_even_locked\("
)
GATES = r"(?:refused_while_locked\(\)|lock_waits\(\)|paused_by_lock\(\)|app_locked\()"
# Bodies that run locked or not, and the frames that run only while locked.
UPKEEP = [
    "src/ui/app_render/frame_upkeep.rs::frame_upkeep",
    "src/ui/notifications/mod.rs::poll_live",
]
LOCKED_FRAMES = [
    "src/ui/app_render/locked_root.rs::locked_root",
    "src/ui/chat_window/view.rs::render_locked",
]
UPKEEP_CALL = re.compile(r"\bself\.([a-z_][a-z0-9_]*)\(")
EARLY_GATE = re.compile(r"\bif\s+self\.lock_waits\(\)\s*\{\s*return\b")
OPEN_GATE = re.compile(r"\bif\s+!\s*self\.lock_waits\(\)")
# A gate tested in a condition: `if ..gate..`, a match guard `.. if ..gate.. =>`,
# `return ..gate..`, or `let .. = ..gate..;` followed by `if`/`return` on it.
GATED = re.compile(
    r"\bif\b[^{;]*" + GATES + r"|\breturn\b[^;]*" + GATES + r"|=>[^,;]*" + GATES
)
GATE_BINDING = re.compile(r"\blet\s+(\w+)\s*=[^;]*" + GATES + r"[^;]*;")
TEST_MOD = re.compile(
    r"^\s*#\[cfg\(test\)\]\s*\n(?:\s*#\[[^\n]*\]\s*\n)*\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{",
    re.M,
)
TEST_ITEM = re.compile(r"#\[cfg\(test\)\]\s*\n(?:\s*#\[[^\n]*\]\s*\n)*\s*(?:pub(?:\([^)]*\))?\s+)?fn\s")
FN = re.compile(r"\bfn ([a-z_][a-z0-9_]*)\s*(?:<[^>(]*>)?\(")
LOCKED_WRITE = re.compile(r"\.(?:locked|lock_state)\s*=[^=]")


def balanced(text, start):
    depth = 1
    for i in range(start, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return start, i
    return None


def strip_comments(text):
    """Line comments out, string literals kept: a `//` inside a string stays."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append(text[i : j + 1])
            i = j + 1
        elif text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j == -1 else j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            i = n if j == -1 else j + 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


def production(text):
    text = strip_comments(text)
    # A test-only function: its body goes.
    while True:
        match = TEST_ITEM.search(text)
        if not match:
            break
        brace = text.find("{", match.end())
        span = balanced(text, brace + 1) if brace != -1 else None
        end = span[1] + 1 if span else match.end()
        text = text[: match.start()] + text[end:]
    match = TEST_MOD.search(text)
    return text[: match.start()] if match else text


def functions(text):
    """(name, start, end, body) of every function with a body."""
    out = []
    for match in FN.finditer(text):
        brace = text.find("{", match.end())
        semi = text.find(";", match.end())
        if brace == -1 or (semi != -1 and semi < brace):
            continue
        span = balanced(text, brace + 1)
        if span:
            out.append((match.group(1), span[0], span[1], text[span[0] : span[1]]))
    return out


def gated(body):
    if GATED.search(body):
        return True
    for binding in GATE_BINDING.finditer(body):
        name = re.escape(binding.group(1))
        rest = body[binding.end() :]
        if re.search(rf"\bif\b[^{{;]*\b{name}\b|\breturn\b[^;]*\b{name}\b", rest):
            return True
    return False


def sources():
    for root, _, names in os.walk("src"):
        if os.sep + "tests" in root or root.endswith("tests"):
            continue
        for name in sorted(names):
            if name.endswith(".rs") and not name.endswith("tests.rs") and "_tests_" not in name:
                path = os.path.join(root, name)
                with open(path, encoding="utf-8") as f:
                    yield path, production(f.read())


def gated_spans(body):
    """The parts of an upkeep body that run only while unlocked."""
    spans = []
    early = EARLY_GATE.search(body)
    if early:
        spans.append((early.start(), len(body)))
    for match in OPEN_GATE.finditer(body):
        brace = body.find("{", match.end())
        span = balanced(body, brace + 1) if brace != -1 else None
        if span:
            spans.append((match.start(), span[1]))
    return spans


def upkeep_problems(bodies, steps):
    out = []
    for key in UPKEEP:
        found = bodies.get(key)
        if not found:
            out.append(f"{key}: missing (UPKEEP in scripts/lock_surfaces.py)")
            continue
        body = found[0]
        spans = gated_spans(body)
        if not spans:
            out.append(f"{key}: runs locked or not, so it must test `lock_waits()` in a condition")
        seen = set()
        for call in UPKEEP_CALL.finditer(body):
            if any(a <= call.start() < b for a, b in spans):
                continue
            name = call.group(1)
            if name in ("lock_waits", "refused_while_locked"):
                continue
            seen.add(name)
            if (key, name) not in steps:
                out.append(
                    f"{key}: `self.{name}(` runs while locked. Put it behind `lock_waits()` "
                    f"(its state then waits for the unlock), or list it in {LIST} as "
                    f"`{key} -> {name} | runs-locked | <why it shows and starts nothing>`."
                )
        for (step_key, name), n in sorted(steps.items()):
            if step_key == key and name not in seen:
                out.append(f"{LIST}:{n}: `{key} -> {name}` no longer runs while locked")
    for (step_key, _), n in sorted(steps.items()):
        if step_key not in UPKEEP:
            out.append(f"{LIST}:{n}: `{step_key}` is not in UPKEEP")
    for key in LOCKED_FRAMES:
        found = bodies.get(key)
        if not found:
            out.append(f"{key}: missing (LOCKED_FRAMES in scripts/lock_surfaces.py)")
        elif re.search(r"\b(?:per_window_upkeep|frame_upkeep)\(", found[0]):
            out.append(f"{key}: a locked frame runs no upkeep")
    return out


AGENT_SERVER = "src/agent/server.rs"
# Writes that can wait for the user's click (request_access in ask.rs is
# the grant change itself, not a write).
WRITE_PROMPTS = ("src/agent/tools/write/", "src/agent/tools/td_call.rs")


def agent_problems(files):
    out = []
    for path, text in files.items():
        if not path.startswith("src/agent/"):
            continue
        for name, start, end, body in functions(text):
            turn = body.find("pacing::take_turn(")
            if turn != -1 and "still_allowed()" not in body[turn:]:
                out.append(
                    f"{path}::{name}: a write that waited its queue turn must call "
                    f"`still_allowed()` after `pacing::take_turn(`, which refuses once Quill locked"
                )
            if path.startswith(WRITE_PROMPTS):
                wait = body.find("prompts::wait(")
                if wait != -1 and "still_allowed()" not in body[wait:]:
                    out.append(
                        f"{path}::{name}: a write the user allowed after a wait must call "
                        f"`still_allowed()` after `prompts::wait(`, which refuses once Quill "
                        f"locked or the grants changed"
                    )
            if name == "copy_new":
                continue
            copy = body.find("copy_new(")
            if copy != -1 and not gated_by(body[:copy], r"locked_since_call_began\(\)"):
                out.append(
                    f"{path}::{name}: `copy_new(` writes a chat's file to the agent's path; test "
                    f"`locked_since_call_began()` in a condition first"
                )
    answer = next(
        (body for name, _, _, body in functions(files.get(AGENT_SERVER, "")) if name == "answer"),
        None,
    )
    arm = answer.find('"resources/read"') if answer else -1
    read = answer.find("read_resource(", arm) if arm != -1 else -1
    if read == -1 or not gated_by(answer[arm:read], r"paused_by_lock\(\)"):
        out.append(
            f"{AGENT_SERVER}::answer: the `resources/read` arm must test `paused_by_lock()` "
            f"before `read_resource(`"
        )
    return out


def gated_by(text, gate):
    return re.search(r"\bif\b[^{;]*" + gate + r"|\breturn\b[^;]*" + gate, text) is not None


def listed(path=LIST):
    out, problems = {}, []
    steps = {}
    if not os.path.exists(path):
        return out, [f"{path}: missing"]
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = [p.strip() for p in line.split("|", 2)]
            if " -> " in parts[0]:
                key, _, name = parts[0].partition(" -> ")
                if len(parts) != 3 or parts[1] != "runs-locked" or not parts[2]:
                    problems.append(f"{path}:{n}: want `path::fn -> method | runs-locked | reason`")
                elif (key.strip(), name.strip()) in steps:
                    problems.append(f"{path}:{n}: `{parts[0]}` is listed twice")
                else:
                    steps[(key.strip(), name.strip())] = n
                continue
            if len(parts) != 3 or not parts[2] or "::" not in parts[0]:
                problems.append(f"{path}:{n}: want `path::fn | closer | reason`")
                continue
            if parts[0] in out:
                problems.append(f"{path}:{n}: `{parts[0]}` is listed twice")
            out[parts[0]] = (parts[1], n)
    return out, problems, steps


def problems():
    entries, out, steps = listed()
    starters, bodies, by_name, files = {}, {}, {}, {}
    for path, text in sources():
        files[path] = text
        fns = functions(text)
        for name, start, end, body in fns:
            bodies.setdefault(f"{path}::{name}", []).append(body)
            by_name.setdefault(name, []).append(body)
        for match in STARTS.finditer(text):
            inner = [f for f in fns if f[1] <= match.start() < f[2]]
            if not inner:
                continue
            # The innermost function holding the call.
            name = min(inner, key=lambda f: f[2] - f[1])[0]
            starters.setdefault(f"{path}::{name}", match.group(0))
    gate_body = next(iter(bodies.get(f"{GATE_FILE}::close_for_lock", [])), None)
    if gate_body is None:
        return out + [f"{GATE_FILE}: no `close_for_lock`"], len(starters)
    for key, what in sorted(starters.items()):
        if any(gated(body) for body in bodies.get(key, [])):
            continue
        if key not in entries:
            out.append(
                f"{key}: starts `{what}` without asking the lock. Test "
                f"`refused_while_locked()` in a condition, stop it in `close_for_lock`, "
                f"or list it in {LIST} as `{key} | <closer, via:fn or -> | <reason>`."
            )
    for key, (closer, n) in sorted(entries.items()):
        if key not in bodies:
            out.append(f"{LIST}:{n}: `{key}` is not a function")
            continue
        if closer == "refuses":
            if not any(gated(body) for body in bodies[key]):
                out.append(f"{LIST}:{n}: `{key}` must test a lock gate in a condition")
            continue
        if key not in starters:
            out.append(f"{LIST}:{n}: `{key}` starts nothing this guard looks for")
        if closer.startswith("via:"):
            via = closer[4:]
            target = bodies.get(via) if "::" in via else by_name.get(via)
            if not target:
                out.append(f"{LIST}:{n}: `{via}` is not a function")
            elif not any(gated(body) for body in target):
                out.append(f"{LIST}:{n}: `{via}` must test a lock gate in a condition")
        elif closer != "-" and not re.search(rf"\bself\.{re.escape(closer)}\(", gate_body):
            out.append(f"{LIST}:{n}: `{key}` names `{closer}`, which `close_for_lock` does not call")
    # One writer of the flag. `set_locked` reaches it through
    # `PasscodeUi::set_lock_flag`, which moves the lock count with it
    # (round 15), and nothing else calls that.
    for path, text in files.items():
        if not path.startswith("src/ui/"):
            continue
        for name, start, end, body in functions(text):
            if LOCKED_WRITE.search(body) and "passcode" in path and not (
                (path == GATE_FILE and name == "set_locked")
                or (path == FLAG_FILE and name == FLAG_SETTER)
            ):
                out.append(f"{path}::{name}: writes the lock flag; only `set_locked` may")
            elif re.search(rf"\.{FLAG_SETTER}\(", body) and not (
                path == GATE_FILE and name == "set_locked"
            ):
                out.append(f"{path}::{name}: calls `{FLAG_SETTER}`; only `set_locked` may")
            elif re.search(r"passcode\.(?:locked|lock_state)\s*=[^=]", body) and path != GATE_FILE:
                out.append(f"{path}::{name}: writes `passcode.locked`; only `set_locked` may")
    # What runs while locked.
    out += upkeep_problems(bodies, steps)
    # Agent effects after a wait.
    out += agent_problems(files)
    # The render gate is mounted.
    if "register_plugin::<LockLayer>" not in files.get(LAYER_FILE, ""):
        out.append(f"{LAYER_FILE}: `LockLayer` is not registered as a root plugin")
    if "lock_layer::register(" not in files.get(PASSCODE_FILE, ""):
        out.append(f"{PASSCODE_FILE}: `PasscodeUi::new` must call `lock_layer::register`")
    return out, len(starters)


if __name__ == "__main__":
    found, count = problems()
    for line in found:
        print(line)
    print(f"lock surfaces: {'FAIL' if found else 'OK'} ({count} starting functions)")
