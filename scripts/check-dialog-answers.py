#!/usr/bin/env python3
"""Every request a box waits on has a box that takes its answer.

`RequestPurpose::answered_to_a_box` (src/state/domains/chat_list/
dialog_answers.rs) lists the requests whose end is kept for a folder or
contact box, by request id. A purpose on that list with nothing in src/ui
that takes it (`dialog_answers.take(`) only fills the queue with answers no
one reads, and its box waits for ever. Each purpose therefore must be named
in a file that also takes from `dialog_answers`, or in the request-sending
code of a box that does (`ui/folders`, `ui/folder_share`, `ui/contacts`).

The box side is checked by tests (`ui/folders/box_stage_tests.rs`); this
keeps a new entry on the list from being added without one.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent / "src"
LIST = ROOT / "state/domains/chat_list/dialog_answers.rs"

# The box that waits on each purpose, by the driver call it sends.
SENDERS = {
    "CreateChatFolder": "create_chat_folder",
    "EditChatFolder": "edit_chat_folder",
    "CreateChatFolderInviteLink": "create_folder_invite_link",
    "EditChatFolderInviteLink": "edit_folder_invite_link",
    "DeleteChatFolderInviteLink": "delete_folder_invite_link",
    "CheckChatFolderInviteLink": "check_folder_invite_link",
    "AddChatFolderByInviteLink": "add_folder_by_invite_link",
    "AddContact": "add_contact",
}


def listed():
    text = LIST.read_text()
    body = re.search(r"fn answered_to_a_box.*?matches!\((.*?)\)\s*\}", text, re.S)
    if not body:
        sys.exit("check-dialog-answers: answered_to_a_box not found")
    return re.findall(r"RequestPurpose::(\w+)", body.group(1))


def main():
    ui = ROOT / "ui"
    takers = [
        p for p in ui.rglob("*.rs") if "dialog_answers.take(" in p.read_text()
    ]
    if not takers:
        sys.exit("check-dialog-answers: nothing in src/ui takes a dialog answer")
    taker_text = "\n".join(p.read_text() for p in takers)
    folder_ui = "\n".join(
        p.read_text()
        for d in ("folders", "folder_share")
        for p in (ui / d).rglob("*.rs")
    ) + (ui / "contacts.rs").read_text()
    problems = []
    for purpose in listed():
        call = SENDERS.get(purpose)
        if call is None:
            problems.append(f"{purpose}: add it to SENDERS in this script, with the box that waits on it")
        elif f".{call}(" not in folder_ui and f".{call}(" not in taker_text:
            problems.append(f"{purpose}: no box in src/ui sends `{call}` and takes its answer")
    for message in problems:
        print(message)
    if problems:
        sys.exit(f"check-dialog-answers: {len(problems)} problem(s)")
    print(f"check-dialog-answers: ok ({len(listed())} purposes)")


if __name__ == "__main__":
    main()
