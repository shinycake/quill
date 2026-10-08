# Avatar click opens the sender's profile (codex/avatar-profile)

## tdesktop behavior (read-only reference, Telegram/SourceFiles)

- Left click on a message userpic: `Element::fromLink`
  (history/view/history_view_element.cpp:2350-2367) calls
  `window->showPeerInfo(from)` for `displayFrom()` (left button only).
  `displayFrom()` is the sender peer: a user or bot, or the chat itself for
  anonymous admins / channels posting into a discussion group.
- `SessionNavigation::showPeerInfo` (window/window_session_controller.cpp:1391)
  -> `showSection(Info::Memento)`. `MainWidget::showNewSection`
  (mainwidget.cpp:2011-2030) asks `Memento::createLayer`
  (info/info_memento.cpp:311) first: when the window is wide enough
  (`LayerWidget::MinimalSupportedWidth`) the profile is an
  `Info::LayerWidget`, a dimmed modal layer. Only narrow windows get a
  main-column section, and the third column needs `params.thirdColumn`,
  which userpic clicks do not set. So the default presentation is a layer.
- Right click on the userpic: `fillSenderUserpicMenu`
  (history/history_widget.cpp:10465) -> `FillSenderUserpicMenu`
  (window/window_peer_menu.cpp:4407): View profile, Send message, Mention,
  Search messages from; not offered in private chats. Not implemented here
  (follow-up).
- Channels show no userpic next to posts, so signed channel posts have no
  avatar click. Middle/ctrl click: no handler (`context.button != Left`
  returns).

## Quill decision

- Presentation: modal layer (scrim + centered card, fade and 14px rise,
  `Glide` + `request_animation_tick`, snaps when the window is inactive).
  Escape (`cancel_search` chain), the close button or a click outside close it.
- Content: the existing info panel (`info_panel_parts`) is shared by the
  right column and the modal; the right column is suppressed while the modal
  presents the same target (`profile_modal_active`).
- Routing: `Session::avatar_profile_target` (unit-tested): user/bot -> user
  profile, `messageSenderChat` -> that chat's info (anonymous admin = the
  group, channel sender = channel), unknown chat -> nothing.
- Hit area: only the avatar wrapper in `history.rs` (`AvatarLink`), pointer
  cursor; spacer avatars and outgoing rows are not clickable.

## Content parity (tdesktop info_profile_*)

| Item | Status |
| --- | --- |
| Avatar, name, status / "bot" | yes (photo from userFullInfo) |
| Message, Call, Video, Secret chat, Add contact | yes (Message added; hidden inside the peer's own chat) |
| Mute | only in the peer's own private chat (previously muted the group from a member panel) |
| Bio, username, phone, birthday | yes |
| Groups in common / shared media counts | counts only for the open chat's own peer; groups in common always |
| Block / Delete contact | yes |
| Verified / premium badge, emoji status, photo carousel, business hours | gap |
| Sender name click, right-click userpic menu | gap |
