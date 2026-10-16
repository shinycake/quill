# Per-chat composer send state

Quality sweep, reactive-state audit findings 2 and 7.

## The bug

The composer kept the next send's choices in one global `ComposerUi`: the
attachment tray, the self-destruct timer, caption above media, High quality,
the paid media price, the schedule and its repeat, the link preview choice and
"send silently". Opening another chat did not touch them, so a file attached in
chat A, or a timer or schedule picked there, went out with the next message in
chat B. The chat switch cleanup was a hand-kept list, copied in
`dismiss_cross_chat_state` (`src/ui/drafts.rs`) and inline in
`select_chat_with` (`src/ui/chat_list/mod.rs`); the two copies had already
drifted apart, neither cleared the send choices, and the story viewer, group
upgrade, Settings > Ask a Question and every chat the driver opens after a
TDLib answer (deep links, new private chats, comment threads) ran neither.

Reproduced on main 790f952c with a UI test (attach a photo, pick a timer and a
schedule, turn on silent in chat 11, open chat 12): it failed on the first
assert, "attachment leaked".

## What tdesktop does

- The text draft, its reply and its link preview choice belong to the chat or
  topic they were typed in: `Data::Draft` holds `textWithTags`, `reply` and
  `webpage` (`WebPageDraft`: removed, invert, large/small media, url), keyed by
  `Data::DraftKey`. Opening a chat applies its draft, including
  `_preview->apply(draft->webpage)` (`HistoryWidget::applyDraft`).
- After a send the field is cleared and the preview draft reset
  (`_preview->apply({ .removed = true })` in `sendTextWithTags` and
  `sendRichMessage`; with no links left `WebpageProcessor::apply` drops
  `removed` too), so the next message starts from defaults.
- Attachments, the timer, caption above media, High quality, price and the
  schedule are chosen in `SendFilesBox` or at send time from the send menu.
  None of them sit in the compose area. The box is modal
  (`setCloseByOutsideClick(false)`) and `MainWidget::showPeerHistory` calls
  `hideSettingsAndLayer()`, so opening another chat closes it and nothing chosen
  there reaches another chat.
- The silent toggle (`Ui::SilentToggle`, broadcast channels only) shows the
  channel's own `silentPosts` notify setting and writes it back on click: it is
  a per-chat setting, never global.

## What changed

- New `src/ui/composer_sends.rs`: `ComposerSends` holds a `PendingSend` per
  `DraftKey` (chat plus topic, the key the text draft uses) and a per-chat silent
  override. The composer reads the entry for the open chat on every render
  (`pending_send()`, `pending_attachments()`), so whichever path opens a chat,
  it shows that chat's entry. No path needs to clear anything.
- The store is a `QuillApp` field outside the per-window slot that
  `chat_window::slot::swap_slot` exchanges, so a chat's choices are keyed by
  chat, never by window.
- Quill's tray and option chips sit in the compose area, so they follow
  tdesktop's rule for the compose area: per chat and topic, like the draft.
  Going back to chat A shows its tray and choices as they were left (tdesktop
  has no compose-area tray; its modal box can't be left open while you go to
  another chat).
- A successful send drops the slot's entry (`finish_pending_send`): tray,
  timer, caption above, quality, price, schedule and preview choice all start
  over, matching tdesktop's cleared draft and reset preview. Before this the
  preview choice stayed for later sends; it now resets like tdesktop's.
- Silent: `composer_effective_silent` is the chat's override if the user set
  one, else the chat's `default_disable_notification`. Toggling stores the
  override only when it differs from the default. The old global `silent` flag
  and `loud_chat` mirror are gone, as is `quill::state::effective_silent`.
- The two cleanup lists are now one function, `dismiss_cross_chat_state`, which
  only closes what was open for another chat (menus, confirmations, the forward
  bar, the TTL picker, a recording). The story viewer's message link and the
  group upgrade now run it as well; the story path also saves the leaving
  draft and restores the new chat's.
- The tray's file checks (`SendBoxUi`: photo sizes, sound probes, thumbnails)
  are keyed by path. A send or clear now forgets only its own files
  (`forget`), so another chat's tray keeps its checks, and a check that
  finishes after the user moved on updates every tray holding that file
  (`attachments_at_mut`) instead of whatever chat is open.

## Not changed, and why

- `send_silent_once` and `send_effect_once` stay in `ComposerUi`. They are set
  and cleared around one synchronous `submit_from_send_button` call, so they
  can't outlive a chat switch; the finding overstated them.
- `group_media` (the album grouping override) stays in `ComposerUi`: its only
  reader and writer are in `src/ui/media_viewer/playback.rs`, which another
  thread owns right now. It is reset after every send and clear, so at worst a
  grouping choice made with a tray in A applies to the next tray in B.
  Follow-up: move `composer_group_media_effective` and
  `toggle_composer_group_media` next to the store and add `group_media` to
  `PendingSend`.
- tdesktop's channel silent toggle writes the setting to the server
  (`toggleChatDefaultDisableNotification`); Quill's toggle lives in the send
  menu for every chat and stays local, as before.
- Cloud drafts still save `link_preview_options: null`, so the preview choice
  is per chat for this session only (tdesktop also saves it with the cloud
  draft). That gap predates this change.
- The similar-channels strip already opened chats through
  `select_listed_chat`; the finding listed it wrongly.

## Verification

- `src/ui/composer_sends_tests.rs` (GPUI UI tests on the demo app):
  - switching chats shows B with nothing chosen, and A's tray, timer,
    schedule, preview choice and silent come back on return;
  - five open paths (chat list, search result, search message, story viewer
    message link, a chat opened by the session with no UI code involved, which
    is how deep links and Ask a Question open) each show B with nothing chosen;
  - a send resets schedule and preview choice and keeps the chat's silent
    choice;
  - silent follows each chat's default and its own override, and an override
    equal to the default is dropped;
  - a chat window's choices are stored under its chat: B's window sees only
    B's tray, the main window only A's.
- Existing composer, send box and image editor tests pass with the store.
- Captures (demo `ready-send-media`, 1200x800): chat A with `demo-notes.txt`
  in the tray, chat B after clicking it in the list (empty composer), and A
  again (tray back), light and dark.

No perf or memory work here, so no measurements.
