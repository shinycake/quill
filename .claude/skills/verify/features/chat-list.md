# Chat list

The left column: folder tabs, search field, dialog rows (avatar, name, last
message, time, unread badge, pins).

## Sub-features

- Folder tabs and unread badges.
- Search field and results.
- Row hover, selection and keyboard navigation.

## How to get to it (user POV)

It is the first thing a signed-in user sees. Click a row to open that chat;
`escape` then arrow keys move the selection.

## Driving it with --screenshot-demo

| Kind | State on screen |
| --- | --- |
| `ready-chats` | signed in over the standard chat-list fixture |
| `ready-chat-list-search` | search field focused with results |
| `ready-chat-list-3` | a short three-row list |

Click a row with `QUILL_DEMO_CLICK="200,300"` and check the history pane
opened for that chat. End state: rows aligned, time and badge on the right,
selected row highlighted, no clipped text in light and dark.

## Gotchas

- Fixture rows are synthetic (`DC` initials, no photos) unless the demo
  seeds avatars.
- Some fixture messages have no `date`, so they show no time.
