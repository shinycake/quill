# Chat row menu parity (tdesktop)

tdesktop's chat-list context menu (`Window::FillDialogsEntryMenu`) has
left-aligned rows with icons: Archive, Pin, Mute notifications, Mark as
read/unread, Clear history, then Delete chat in the danger color.

Quill's menu had centered text buttons in its own order (Pin, Mark as
unread, Mute, Archive…). It now uses the message menu's row style
(#385), sorted into tdesktop's order with tdesktop's labels ("Mute
notifications"). Quill's extras slot in before Delete: Select, Clear
history for everyone, Report, Block user. The panel occludes, so clicks
inside never reach the backdrop.

Not done: tdesktop's "Open in new window" (Quill has a single window)
and its Mute submenu of durations. Quill mutes forever, as before.

Verified live (menu opened and dismissed without acting).
