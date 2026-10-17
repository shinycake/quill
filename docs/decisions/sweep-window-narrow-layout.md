# Main window layout on narrow windows

Round 2 UI/UX audit, findings 1, 12 and 13. The main window had no minimum
size, and its columns (chat list, forum topics, conversation, info /
downloads / shared media panels) always sat side by side at fixed widths. A
half-screen window with a wide chat list and the info panel open left the
conversation 0 to 100 px wide.

## tdesktop

- `window.style`: `windowMinWidth: 380`, `windowMinHeight: 480`;
  `columnMinimalWidthLeft: 260`, `columnMinimalWidthMain: 380`,
  `columnMinimalWidthThird: 292`.
- `SessionController::computeColumnLayout`: below left minimum + main
  minimum the window is one column (the list or the chat, never both); the
  third column stays only while the chat keeps its minimum beside it,
  otherwise the info section is shown in the chat's place; the list width is
  capped so the chat keeps `columnMinimalWidthMain`.

## Quill

One pure function, `ui/shell_layout::shell_layout(ShellInput) ->
ShellLayout`. Its inputs are the window's content width (interface pixels,
so interface zoom is already applied), the folder rail width, the dragged
chat list width, whether a chat and a forum topic are open, the forum
column placement (`Session::forum_column`) and the summed preferred width of
the open side panels. `QuillApp::shell_layout()` reads them from state on
every call. The shell row, the chat list's width (and the folder tabs,
stories strip and header fit checks that depend on it), the drag clamp and
the chat header's back button all derive from it. Nothing is mirrored: the
only stored input is the window width as last drawn
(`FrameUi::main_width`), the same fact `window.viewport_size()` gives.

Thresholds (interface pixels):

| Rule | Value | Source |
| --- | --- | --- |
| Window minimum | 380 x 480, scaled by interface zoom when the window opens | tdesktop `windowMin*` |
| Conversation minimum beside other columns | 380 | `columnMinimalWidthMain` |
| One column below | rail + 240 (list minimum) + 380 = 620 without the folder rail | `computeColumnLayout` |
| Side panel minimum beside the chat | 280 (panels are laid out for 300 / 320) | `columnMinimalWidthThird` is 292 |
| Panel replaces the chat below | rail + list + 380 + 280 (940 with the default 300 list, no rail) | same rule |

- One column: an open side panel fills the window; else the open chat
  (with a back button in its header that closes the chat, like Escape);
  else, for a forum with no topic open, the topic list fills the window
  with a back button; else the chat list fills the window.
- Two columns: the list keeps its dragged width but never leaves the chat
  under 380; dragging is clamped the same way. A panel is beside the chat
  while the chat keeps 380 and the panel 280; it narrows from its preferred
  width down to 280 first, then takes the chat's place. Its close button
  brings the chat back.
- The info, downloads, shared media and playlist panels no longer set their
  own width; the shell does.
- Chat header: the action buttons are `flex_none` and the title side is
  `flex_1 min_w_0`, so the title truncates and the buttons keep their size.
  No button is hidden (the design guides ask for another path to any hidden
  action, and at 380 px all of them fit).

The interface-zoom wrapper already scales `window_min_size` when a window
opens; a zoom change while the window is open does not update the
platform's minimum (GPUI has no setter). The layout itself follows the zoom
live, since it works in interface pixels.

Forum topics: while the topic column replaces the list (`Replacing`, below
1100) it counts as a column, so the window is one column below
`FORUM_COLUMN_WIDTH + CHAT_MIN_WIDTH` (660) and the chat never gets under
380. In one column, Escape and the header's back button share
`QuillApp::step_back`: the topic list first, then the chat list. Both and
the layout read one `ShellInput` (`QuillApp::shell_input`, forum peek
included).

## Not covered

- History bubbles do not shrink to the pane. Their widths come from
  `msgMaxWidth` (430) and fixed-width parts (link preview cards, reply
  lines, media), so in a conversation narrower than about 480 px a wide
  bubble is cut at the pane edge. tdesktop recomputes bubble geometry for
  the available width (`Message::countGeometry`). That is a bubble width
  policy change of its own (`history/bubble_width.rs`, `quill::bubble_layout`).
- No screenshot demo opens a forum, so the forum paths are covered by the
  `shell_layout` unit tests only.
- Tracked follow-up: the bubble clipping above shows at 640 and 380 px in the
  showcase captures (outgoing bubbles lose their left edge). It needs the
  bubble width policy change, to be done in its own PR.
