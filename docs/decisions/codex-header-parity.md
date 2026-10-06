# Chat header parity (tdesktop)

tdesktop's top bar (`history_view_top_bar_widget.cpp`) in a wide window:
- the chat name and a status line, with no userpic (the userpic `_info`
  button only appears in one-column mode);
- icon buttons, right to left: ⋮ menu, third-column (info) toggle, call /
  group call, search. Calls show for users other than yourself and bots.
- Everything else lives in the ⋮ menu: auto-delete, "View discussion",
  and closing a secret chat.

Quill had a 38px avatar, a "⋯" text button, no info toggle (the title
opened the panel), and text buttons in the bar for "Close secret chat",
the "⏱ timer" and "Discuss". Now:
- the avatar is gone, and title + status keep the click-to-open-info
  behavior;
- the buttons are Search, Call (users), Voice chat (groups), Info toggle
  (panel-right, selected while the panel is open), ⋮ (ellipsis-vertical)
  in tdesktop's order;
- "View discussion", "Auto-Delete" (secret chats that can post) and
  "Close secret chat" moved into the ⋮ menu;
- the ⋮ button lost its tooltip, which drew over its own menu.

Quill keeps its voice-chat button for every group. tdesktop hides it
while a call with participants is live and shows a join bar instead,
which Quill doesn't have yet.

Verified live in Saved Messages and a group: icons, info toggle open and
close, menu.
