# Right-to-left and mixed-direction text sweep (R10)

## What tdesktop does
`Ui::Text::String` takes each paragraph's direction from its first strong character.
Names, titles and previews in lists are one-line strings: they stay start-aligned,
are reordered visually, and elide at the end of reading order. Captions, bubble text,
poll and checklist text are blocks aligned by their own direction.

## What changed
Two helpers in `src/ui/bidi_line.rs`:
- `one_line_plain` (existing): single-line names/titles/previews, now used at the sites below.
- `aligned_block` / `aligns_end` (new): wrapping blocks of user text; right-to-left first-strong
  text fills the width and aligns right, everything else is unchanged. Unit-tested (`aligns_end`).

Single-line (`one_line_plain`, with `truncate()` where the row had none):
- `conversation.rs`: chat header title and status line
- `composer_ui.rs`: reply/edit bar title, reply quote, reply preview, edit preview, context-bar
  preview, mention suggestion names
- `share_box_ui.rs`: share destination titles, forward bar sender and preview, send-as titles
- `forward.rs`: forward success banner detail (destination title)
- `profile_panels.rs`: chats-in-common/groups rows
- `group_panels.rs`: contact picker names, user profile name, group and basic-group titles
- `group_members.rs`: member names
- `contacts.rs`: contact names
- `event_log.rs`: actor names
- `notification_settings.rs`: exception chat titles
- `saved_sublists.rs`: sublist titles and previews
- `player_bar.rs`: track title and subtitle
- `accounts.rs`: account names
- `media_viewer.rs`: sender name in the top bar
- `story_viewer.rs`: poster header, story viewers' names

Blocks (`aligned_block`):
- `message_poll.rs`: question, description, option labels, quiz explanation
- `message_checklist.rs`: title, task text, completion caption
- `profile_panels.rs`: profile detail rows (bio, note, links)

## Already correct / left alone
Media viewer and story captions go through `rich_text_line` (aligns by direction already).
Pinned bar already used the helper. Service pills, event-log description sentences (start with
English), story-strip labels (centered cells), kit `Button` labels (admin chips) and static strings
are unchanged.

## Verification
- `gate.sh`: GATE OK (core 2431, ui 169).
- Demo capture with a local-only mixed Arabic/Latin chat title: header reads in correct visual order.
  Captures and the temporary fixture are not committed.
- Not verified visually: the other sites (mechanical routing through the same helpers).
