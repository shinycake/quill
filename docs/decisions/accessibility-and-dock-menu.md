# Accessibility and OS menus: dock menu, window menu, screen-reader focus

Parity items: `parity:platform-dock-menu`, `parity:platform-screen-reader-announce`,
`parity:platform-touch-bar`, `parity:platform-voiceover`.

## What tdesktop does

### Dock menu (`menu/menu_dock.cpp`)

- With more than one account and the app not locked: a separator, a disabled
  "Profiles" header, one row per signed-in account (names cut at 30
  characters with an ellipsis; picking one switches to it), then a separator.
- "Disable notifications" or "Enable notifications", which flips the global
  desktop notification switch.
- While the player is not stopped: a separator, then Previous, Pause or
  Resume, and Next.

On Windows the jump list holds one task, "Quit Telegram", which relaunches the
executable with `-quit`. On Linux the `.desktop` file declares `Actions=quit;`
with a `[Desktop Action quit]` entry that runs the same argument.

### Window menu (`platform/mac/global_menu_mac.mm`, `buildWindowMenu`)

Toggle Full Screen (Ctrl+Cmd+F), separator, Contacts, Add Contact, separator,
New Group, New Channel, separator, "Show Telegram". Contacts, Add Contact, New
Group and New Channel are disabled with no session or while locked. Show is
disabled while the window is active.

### Touch Bar

tdesktop 7.2.10 still ships it (`platform/mac/touchbar/`, about 2900 lines of
Objective-C). It has a pinned-dialogs panel, a sticker and emoji scrubber
popover, a text formatting popover for the composer, an audio player and media
viewer controls. The experimental option `kOptionDisableTouchbar` turns it off.

### Screen-reader focus and announcements (`history_inner_widget.cpp`, `history_inner_widget_accessibility.cpp`)

The message list exposes its messages and the unread bar as accessible
children.

- On focus-in (`announceAccessibilityFocusedChild`) it keeps the child it had,
  or picks the one after the unread bar, else the last one.
- In screen-reader mode, Up and Down move between children. PageUp and
  PageDown move by a page, and Home and End go to the ends. Shift with Up or
  Down extends the selection, Ctrl+Space toggles it, and Space toggles the
  selection when one exists, else plays or pauses the focused voice message,
  song, audio file or video message.
- The only new-message announcement is `checkAnnounceFirstMessages`. When the
  list held focus while it was empty, the first messages to arrive take focus
  and are read. tdesktop does not announce every incoming message in an open
  chat, and Quill doesn't either.
- `MessageAccessibilityName` reads, one line each: Seen or Not seen
  (outgoing), the sender (or Me), via @bot, "In reply to {name}: {text}",
  "Forwarded from {name}", media parts joined by ", ", web page parts joined
  by ": ", the caption or text, the fact check, Pinned, the status line
  (Sending, Failed, Sent or Received, then "edited", "at HH:MM", "N Views" and
  the signature) and "Reactions: …".

## What changed

- `src/ui/os_menus/`: the dock menu, built exactly as above and rebuilt when
  accounts, the notification switch or the player change. The Window menu has
  tdesktop's rows, and disabled rows go to a no-op action. On Windows the
  jump list holds "Quit Quill". `--quit`, `-quit` and the jump list's
  `--dock-action 0` quit a running Quill (forwarded through the single-instance
  inbox), and with no Quill running they just exit.
- `src/ui/new_contact.rs` and `src/connect/contacts.rs`: Add Contact opens the
  New Contact box (name and phone) and imports the contact through TDLib.
- Linux: `assets/quill.desktop` declares the quit action, and the installer
  and packaging carry it.
- `src/message_a11y.rs`: `message_accessibility_name`, a port of
  `MessageAccessibilityName`.
- `src/ui/history_a11y.rs`: the history's screen-reader focus model. Focus-in
  picks the child, the navigation keys and Space work as in tdesktop, and the
  first messages of a focused empty list take focus. The focused row is the
  history's AccessKit active descendant, labelled with its full name. The
  ordinary keyboard behaviour stays unchanged while no screen reader reads
  the window.
- Labels: "Jump to bottom, N", the group admin switches, and the media
  viewer's Zoom In and Zoom Out.
- `scripts/macos-accessibility-smoke.sh flows` walks the chat list,
  conversation and composer, an animating chat, the media viewer, settings,
  group info and a dialog. It fails on any unnamed control and checks that
  focusing the history through AX lands on a message whose name ends in its
  delivery line.

A bug the smoke run caught: GPUI runs focus listeners at the end of a draw,
where `cx.notify()` does not schedule another frame, and the message list
keeps rendered rows until they are remeasured. The focus pick is now
deferred, and the rows losing and gaining focus are remeasured.

## How it was verified

- Unit tests: `message_a11y` (4 tests, including name lines, outgoing
  receipts, files and durations), the pure focus steps in `history_a11y`, and
  the New Contact driver tests.
- GPUI UI tests (`history_a11y_tests.rs`) on the `ready-chats` demo:
  - focus-in picks the default child, the arrows and Home move it, and the
    name ends in the delivery line
  - with no screen reader, the arrows keep their old meaning
  - in a focused empty chat, the first messages take focus
  - a rebuild leaves focus where it was
- macOS AX smoke, run locally twice: `scripts/macos-accessibility-smoke.sh
  target/debug/quill <dir> flows` passed all 7 checks, and the default
  `ready-text-entities` run passed as well.

## Not verified

- Spoken VoiceOver output. VoiceOver was not running on the test machine, so
  `macos-voiceover-smoke.sh` was not run, and `parity:platform-voiceover`
  stays open.
- The dock menu, the Windows jump list and the Linux desktop action were not
  exercised by hand on a desktop. The model and the `--quit` path are covered
  by unit tests.
- Touch Bar: not implemented. tdesktop still ships it, and it can't be
  checked without the hardware. Part of it, the formatting popover, belongs
  to the composer, which is out of scope for this change.
  `parity:platform-touch-bar` stays open. Suggested README wording: "Touch
  Bar (pinned chats, emoji and sticker scrubber, formatting, player and media
  viewer controls); window menu items done".
