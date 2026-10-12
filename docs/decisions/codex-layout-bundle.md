# Wide chat layout and the pull to the next unread channel

Two Telegram Desktop chat settings: "Adaptive layout for wide screens" and "Swipe to the next unread channel".

## Adaptive layout for wide screens

**What tdesktop does.** `HistoryWidget::recountChatWidth` sets the chat layout to wide once the history is at least `adaptiveChatWideWidth` (880 px) wide. With `Core::Settings::adaptiveForWide` on, the history is drawn in `ElementChatMode::Wide`:

- `Element::hasRightLayout` is false, so outgoing messages sit on the left with their tail on the left. They keep their outgoing colors.
- `Message::hasFromPhoto` is true for every message, and `canHaveFromUserpics` allows photos in private chats. Every message in a private chat or group gets the sender's photo, your own included. Channels keep `signatureProfiles`.
- Service messages, date pills (inline and floating) and the "Unread messages" label are centred in `WideChatWidth()` = `msgMaxWidth + 2 * msgPhotoSkip + 2 * msgMargin.left()` = 542 px from the history's left edge. The column is not centred in the window. The unread bar's background stays full width.
- The selection check sits right of the message column (`msgSelectionOffset` and friends) instead of at the edge, and outgoing bubbles do not slide aside for it.
- The composer does not change. The README title says "(centered column)", but tdesktop has no centred column. Quill follows tdesktop.
- The checkbox is in Settings → Chat under "Tile background". It is shown only while the chat layout is wide.

**Quill.** `quill::chat_wide` holds the rules (threshold, column width, `has_right_layout`, `shows_sender_photo`, `selection_right`). `ui/chat_wide_ui.rs` measures the history width at paint and renders again when it crosses 880 px. It also provides the column wrapper and the checkbox, which sits under "Tile background" in Appearance → Telegram wallpapers. `BubbleLook::wide` carries the mode into bubble alignment, corners, hover actions, the reaction fly, the selection slide, the Premium sticker mirror and its effect side. Service rows, community and group-call rows, day pills, the floating date and the unread label use the column. The row cache key includes the mode.

**Default off.** tdesktop turns it on by default. That would move every outgoing message to the left on wide windows, which changes Quill's default look, so `AppearancePrefs::adaptive_for_wide` defaults to false.

## Swipe to the next unread channel

**What tdesktop does** (`history_view_pull_to_next_channel.cpp`, lib_ui `ElasticScroll`). In a broadcast channel scrolled to its newest loaded message, a trackpad scroll past the bottom stretches the edge. Within the pull distance (`historyPullNextThreshold * 2` = 96 px) the stretch follows the finger at 1/2.5, then at 1/20. The indicator grows: a capsule with an arrow, and the next channel's photo growing at its bottom. Held past the threshold for 120 ms, the pull becomes ready: a haptic tap, the card expands to 110 px, and the photo with its unread badge and the name pill take over with a bounce. Pulling back past a peak-relative point un-readies it. A note covers the composer: "Pull up to go to the next unread channel", then "Release to go to the next unread channel". Releasing a ready pull preloads the channel (50 messages around the read position plus its pinned messages). It waits up to 1.5 s for the history and 0.6 s for the pins, then opens the channel with a slide from the bottom. Anything else springs back.

`FindNextUnreadChannel` checks the open folder's list, then every other folder, then all chats, then the archive. It takes the first other broadcast channel with unread messages that is not the promoted proxy or PSA chat and has no restriction reason. The pull is off while an ad that will follow the last post has not been shown yet (`hasUnshownFor`). In a forum topic the same gesture goes to the next unread topic ("No more unread topics in {group}" when there is none). The topic list must be loaded first. A plain mouse wheel never pulls; only phased scrolls do. The setting defaults to on.

**Quill.** `quill::pull_next` is the edge and the controller as one deterministic state machine (time in ms), plus the indicator geometry and the finders. `ui/pull_next_ui.rs` adds:

- a capture-phase wheel listener over the history (precise deltas only, as `chat_swipe`);
- the indicator: divs for the capsule, photo, badge and name, plus a canvas for the round-capped strokes;
- the note over the composer, drawn by the main window's conversation slot and the chat window body;
- the dwell and spring on the frame clock;
- the preload-then-jump task, and the slide-in.

The history rises by the indicator's height through the same viewport offset the new-row reveal uses. The core changes are:

- `chatPosition.source` → `ChatListState::promoted_chats`;
- `supergroup.restriction_info` → `GroupsState::supergroup_restricted`;
- a `PreloadHistory` request purpose whose answer only marks the chat ready (`ConnectDriver::preload_next_channel`).

The setting is `MediaPrefs::pull_to_next_channel` (default on, as in tdesktop). It does not change the default look. The switch is in Settings → Media settings, after "Reaction button on messages", where tdesktop puts it after the corner buttons.

## Differences left, and why

- **The chat being left is not drawn sliding out.** tdesktop slides a picture of the old section up while the new one comes in from the bottom. GPUI cannot take a picture of what it drew, so only the incoming conversation slides up (480 ms, ease-out quint) over the empty background.
- **The badge's ring** is a 2 px border in the window background. tdesktop punches a transparent ring out of the photo layer; GPUI has no compositing mode for that.
- **The release spring** starts at rest. tdesktop seeds it with the finger's last velocity, which Quill's trackpad events do not report reliably. At a ready pull the finger is still anyway.
- **The wide checkbox** appears and disappears without the SlideWrap animation.
- **Forum topics.** Quill loads the first page of a forum's topics, so the next unread topic is looked for in that page. tdesktop pages the whole list.
- **No plain bounce.** Quill does not bounce at the edges when the pull is not available. tdesktop's ElasticScroll always does, but that is the scroller, not this setting.

## Verification

- Unit tests: `chat_wide` and `pull_next` (curves, dwell, flick, un-ready, hint, geometry, finders, promotion and restriction parsing).
- Driver test: `connect/tests/pull_next.rs` (preload request JSON and dedupe, readiness, chat not opened).
- GPUI tests: `ui/pull_next_ui_tests.rs`. A full pull in a channel opens the next unread one; a flick and a private chat do nothing; the setting turns it off; the wide layout gives own messages a photo; narrow chats stay normal and hide the checkbox.
- Demo captures, checked by eye and not committed: the wide layout at 1600 px, an early pull, and a ready pull.
