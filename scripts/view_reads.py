"""Update paths in core must not read the view in place as "what is on screen".

The session keeps one window's view in its place (`open_chat`,
`threads.thread`, the Saved Messages sublist and tag filter): the main
window's during `ingest`, a chat window's while that window is drawn or
handles input. An update or redraw path that reads those fields sees the
main window alone, so a chat window misses its new messages, live thread
rows and tap effects (review round 11, area A). Those paths ask
`Session::views_shown` / `chat_is_shown` / `chat_drawn` instead, or edit
through `Session::in_every_view` (src/state/session_views.rs).

This check reads the update, answer and redraw files below and fails on
a read of `self.open_chat`, `session.open_chat`, `.threads.thread` or
`.threads.saved.sublist` / `.tag_search` (the method call
`open_chat(..)` is not a read). A read that is right because it means the
view in place (an answer applied in the view that asked, the main window's
own rows) says so in a comment on its line or one of the three above:

    // view-in-place: <why the view in place is the right one>

Run by scripts/check-window-scope.py.
"""

import pathlib
import re

# The update, answer and redraw paths of core (relative to src/state).
PATHS = (
    "redraw.rs",
    "session_apply*.rs",
    "session_updates.rs",
    "updates_sync.rs",
    "session_notifications.rs",
    "session_forward.rs",
    "domains/*/apply.rs",
    "domains/*/error.rs",
    "domains/*/interaction.rs",
)
# A read may span lines (`session\n    .open_chat`).
READ = re.compile(
    r"\b(?:self|session)\s*\.\s*(?:open_chat\b(?!\s*\()|threads\s*\.\s*thread\b"
    r"|threads\s*\.\s*saved\s*\.\s*(?:sublist|tag_search)\b)"
)
MARKER = re.compile(r"//\s*view-in-place:\s*\S")


def problems(repo: pathlib.Path) -> list[str]:
    state = repo / "src" / "state"
    files = sorted({p for pattern in PATHS for p in state.glob(pattern)})
    failures = []
    for path in files:
        lines = path.read_text().split("\n")
        code = "\n".join(line.split("//", 1)[0] for line in lines)
        for hit in READ.finditer(code):
            number = code.count("\n", 0, hit.end()) + 1
            context = lines[max(0, number - 4) : number]
            if any(MARKER.search(l) for l in context):
                continue
            rel = path.relative_to(repo)
            what = " ".join(hit.group(0).split())
            failures.append(f"{rel}:{number}: update path reads the view in place ({what})")
    return failures
