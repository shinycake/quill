# quill

Independent, keyboard-friendly **Telegram desktop client** written in Rust (**GPUI Kit + official TDLib**). Working name **Quill**. Not a ZapFast fork.

Phase 0–1: synthetic GPUI chat, ordered tdjson bridge, replay reducers, live connect (phone / code / 2FA), and after Ready a main chat list + text send. Live Telegram still needs owner credentials + tdjson.

## Build

See [docs/build.md](docs/build.md). Short version:

```bash
cargo test --no-default-features
cargo run --features ui          # synthetic chat; live connect if credentials + tdjson
cargo run --no-default-features -- --connect-smoke   # headless WaitPhoneNumber gate
```

Toolchain: Rust **1.98.1**. UI pin: **gpui-kit 0.6.1**. TDLib schema: **1.8.67** (`d1085f9cebc5a62379991ae1652673954f229c1f`).

## Status

This is the comprehensive Telegram-parity checklist: one checkbox per user-visible feature/behavior, grouped by area, each with a stable `parity:<area>-<slug>` anchor. `[x]` means the feature genuinely works in Quill today; partial implementations stay unchecked with a note. The parity percentage is computed from this section by `scripts/parity_pct.sh` — never estimated. Newly discovered gaps are added here, so the percentage may drop when audits find new gaps. Every merged feature PR checks its boxes in this list.

### Auth & accounts

- [x] Phone-number login: country code, invalid/banned-number errors, SMS hint <!-- parity:auth-phone-login --> (README:24; telegram/requests.rs:51)
- [x] Verification-code entry: expected digit count, invalid-code handling <!-- parity:auth-code-entry --> (auth.rs:44; telegram/requests.rs:71)
- [ ] Resend the login code (partial: code entry works; no resend UI — schema has resendPhoneNumberCode) <!-- parity:auth-code-resend -->
- [x] Two-step password entry on login <!-- parity:auth-2fa-password --> (auth.rs:52; telegram/requests.rs:81)
- [ ] Log in via QR code: request QR, display link, scan with phone (partial: WaitOtherDeviceConfirmation state text exists at auth.rs:62 but requestQrCodeAuthentication is never called) <!-- parity:auth-qr-login -->
- [ ] "Link desktop device": show QR so another device can log in as this account <!-- parity:auth-qr-authorize-other -->
- [ ] New-user registration: first/last name + terms (partial: explicit UnsupportedHalt — "finish registration in an official client", auth.rs:83) <!-- parity:auth-registration -->
- [ ] Email-based login flow (partial: explicit UnsupportedHalt, auth.rs:72) <!-- parity:auth-email-login -->
- [ ] Premium-purchase-gated login state (partial: explicit UnsupportedHalt, auth.rs:66) <!-- parity:auth-premium-login -->
- [ ] Enable / change / disable the two-step password (schema: setPassword) <!-- parity:auth-2fa-manage -->
- [ ] Set / change recovery email, pending-confirmation state, abort setup (schema: setRecoveryEmailAddress; TGX SetRecoveryEmail, PendingEmailText, AbortPasswordSetup) <!-- parity:auth-recovery-email -->
- [ ] Password recovery via 6-digit email code (partial: login password screen only hints "recovery email is available in the official client", auth.rs:54) <!-- parity:auth-password-recovery -->
- [ ] Active Sessions list: device/app/IP/location with current-device marker (schema: getActiveSessions) <!-- parity:auth-sessions-list -->
- [ ] Incomplete login attempts list with per-attempt terminate (TGX SessionsIncompleteTitle/Info) <!-- parity:auth-sessions-incomplete -->
- [ ] Terminate one session, with confirmation (schema: terminateSession; TGX TerminateSessionQuestion) <!-- parity:auth-session-terminate-one -->
- [ ] Terminate all other sessions, with confirmation (schema: terminateAllOtherSessions; TGX AreYouSureSessions) <!-- parity:auth-sessions-terminate-all -->
- [ ] Per-session toggles: accept secret chats / accept calls (schema: toggleSessionCanAcceptSecretChats, toggleSessionCanAcceptCalls; TGX SessionAccepts) <!-- parity:auth-session-toggles -->
- [ ] "Logged in with Telegram" websites list + disconnect all (TGX WebSessionsTitle, TerminateAllWebSessions) <!-- parity:auth-web-sessions -->
- [x] Log out (telegram/requests.rs:98; state.rs:6502 invalidates account) <!-- parity:auth-logout -->
- [ ] Logout warning text: secret chats die, downloaded media erased (TGX SignOutHint2) (partial: logout works, no warning copy) <!-- parity:auth-logout-warning -->
- [ ] Add another account / switch between accounts: single "accounts/primary" DB layout only, no account UI (settings.rs:39) <!-- parity:auth-multi-account -->
- [ ] Change phone number: move contacts/groups/messages/media to a new number (partial: nothing in Quill; schema has no changePhoneNumber constructor — it re-runs the auth flow on the new number) <!-- parity:auth-change-number -->
- [ ] Delete account with "Deleted Account" explainer (schema: deleteAccount) <!-- parity:auth-delete-account -->
- [ ] Self-destruct-if-away timer (schema: getAccountTtl/setAccountTtl; TGX DeleteAccountIfAwayFor*) <!-- parity:auth-account-ttl -->
- [x] Contacts list with empty state (ui/mod.rs:7729 contacts_list; telegram/requests.rs:364 getContacts) <!-- parity:auth-contacts-list -->
- [x] Add contact via dialog: phone (required), first/last name -> addContact (ui/mod.rs:9469; telegram/requests.rs:379) <!-- parity:auth-contact-add -->
- [ ] Delete contact (schema: removeContacts; TGX DeleteContactConfirm) <!-- parity:auth-contact-delete -->
- [ ] Import contacts from a file/vCard (schema: importContacts) <!-- parity:auth-contact-import -->
- [ ] Sync contacts toggle + delete synced contacts from servers (TGX SyncContacts*, SyncContactsDeleteInfo) <!-- parity:auth-contact-sync -->
- [ ] Block user with confirmation (TGX QBlockUser/BlockUserConfirm; no block request builders in Quill) <!-- parity:auth-block-user -->
- [ ] Edit name (schema: setName) <!-- parity:auth-edit-name -->
- [ ] Edit bio (schema: setBio; profile panel is read-only, ui/mod.rs:8150) <!-- parity:auth-edit-bio -->
- [ ] Username management: set, active-usernames list, reorder, activate/deactivate (schema: setUsername, reorderActiveUsernames, toggleUsernameIsActive) <!-- parity:auth-username -->
- [ ] Set / remove profile photo (schema: setProfilePhoto, deleteProfilePhoto) <!-- parity:auth-profile-photo -->
- [ ] Profile accent color (schema: setProfileAccentColor) <!-- parity:auth-profile-accent -->

### Messaging core

- [x] Send plain text message (`sendMessage`, Enter to send) <!-- parity:msg-send-text --> (telegram/requests.rs:1574; send-on-Enter gate composer.rs:20)
- [x] Bold formatting authoring (partial: incoming bold renders text.rs:203; composer sends entities: [] — requests.rs:1587) <!-- parity:msg-bold --> (M1: toolbar B / Ctrl-B inserts **bold**; send converts to textEntityTypeBold requests.rs)
- [x] Italic formatting authoring (partial: incoming renders; no composer entry) <!-- parity:msg-italic --> (M1: toolbar I / Ctrl-I inserts *italic*; send converts to textEntityTypeItalic)
- [x] Underline formatting authoring (partial: incoming renders; no composer entry) <!-- parity:msg-underline --> (M1: toolbar U / Ctrl-U inserts __underline__; send converts to textEntityTypeUnderline)
- [x] Strikethrough formatting authoring (partial: incoming renders; no composer entry) <!-- parity:msg-strikethrough --> (M1: toolbar S inserts ~~strike~~; send converts to textEntityTypeStrikethrough)
- [x] Inline code / monospace authoring (partial: incoming renders; no composer entry) <!-- parity:msg-inline-code --> (M1: toolbar </> inserts `code`; send converts to textEntityTypeCode)
- [x] Code block / pre (with language) authoring (partial: incoming renders; no composer entry) <!-- parity:msg-code-block --> (M1: toolbar pre inserts fenced block, optional language; send converts to textEntityTypePre/PreCode)
- [x] Spoiler authoring (partial: renders hidden-until-tapped on incoming, text.rs:134) <!-- parity:msg-spoiler --> (M1: toolbar spoiler inserts ||spoiler||; send converts to textEntityTypeSpoiler)
- [x] Quote/blockquote formatting authoring (partial: incoming renders; no composer entry) <!-- parity:msg-quote-block --> (M1: toolbar quote prefixes > lines; send converts to textEntityTypeBlockQuote)
- [x] Create text link (textUrl) authoring (partial: incoming textUrl rendered/opened text.rs:78; no composer entry) <!-- parity:msg-text-link --> (M1: toolbar link inserts [label](url); send converts to textEntityTypeTextUrl)
- [x] Clear formatting on selection (no formatting toolbar at all in composer) <!-- parity:msg-clear-formatting --> (M1: toolbar clear strips markup on selection or whole text, composer.rs)
- [x] Reply to a message <!-- parity:msg-reply --> (begin_reply_to ui/mod.rs:4269)
- [ ] Quote selected text in a reply (partial: incoming quotes render state.rs:6531; send always quote:null requests.rs:1552) <!-- parity:msg-reply-quote -->
- [x] Reply bar in composer with cancel <!-- parity:msg-reply-bar-cancel --> (cancel_reply_draft composer.rs:131)
- [x] Swipe-to-reply gesture <!-- parity:msg-swipe-reply --> (M1: leftward drag >24px on a message starts a reply, ui/mod.rs)
- [x] Forward with "Forwarded from" attribution <!-- parity:msg-forward-attribution --> (forward_messages, send_copy:false requests.rs:2172)
- [x] Forward as copy / hide sender (send_copy hardcoded false, requests.rs:2184) <!-- parity:msg-forward-hide-sender --> (M1: forward picker "Hide sender name" -> forwardMessages send_copy:true, requests.rs)
- [x] Remove caption on forward copy (remove_caption hardcoded false) <!-- parity:msg-forward-remove-caption --> (M1: forward picker "Remove caption" (with send-copy) -> remove_caption:true)
- [x] Multi-select forward (per-message Select) <!-- parity:msg-forward-multiselect --> (toggle_forward_select ui/mod.rs:4682; 100-msg cap connect.rs)
- [x] Forward destination picker <!-- parity:msg-forward-picker --> (forward_destinations state.rs:6585; begin_forward_one ui/mod.rs:4654)
- [x] Edit own message text <!-- parity:msg-edit-text --> (edit_message_text requests.rs:2097; begin_edit ui/mod.rs:21954)
- [x] Edit media caption <!-- parity:msg-edit-caption --> (edit_message_caption requests.rs:2125; wired connect.rs:4989)
- [x] Cancel edit restores prior draft <!-- parity:msg-edit-cancel-restores --> (cancel_edit_draft composer.rs:213)
- [x] Delete own message for everyone (confirm dialog) <!-- parity:msg-delete-everyone --> (delete_confirmed, revoke:true connect.rs:5043; confirm_delete ui/mod.rs:4371)
- [x] Delete for me / delete incoming messages (DeleteConfirm::own requires own-outgoing composer.rs:243; revoke always true) <!-- parity:msg-delete-for-me --> (M1: incoming messages deletable with revoke:false; banner toggles for-me/everyone, gated on can_revoke)
- [x] Pin message <!-- parity:msg-pin --> (toggle_pin_message ui/mod.rs:11464)
- [x] Unpin message (same toggle, Pin/Unpin row button ui/mod.rs:21992) <!-- parity:msg-unpin -->
- [x] Silent pin toggle (partial: pin_chat_message takes disable_notification requests.rs:2274; UI toggles without the option) <!-- parity:msg-pin-silent --> (M1: composer silent toggle feeds disable_notification to pinChatMessage)
- [x] Unpin all messages (unpinAllChatMessages unwired in UI) <!-- parity:msg-unpin-all --> (M1: "Unpin all" button in pinned banner when >1 pinned, unpinAllChatMessages)
- [x] Copy message text (only inline-button CopyText ui/mod.rs:3932 and invite-link copy exist) <!-- parity:msg-copy-text --> (M1: right-click context menu Copy on text/caption messages)
- [x] Share message / copy t.me link (getMessageLink unused) <!-- parity:msg-share-link --> (M1: right-click Share link -> getMessageLink -> clipboard)
- [x] Scheduled send (send at date; options:null, schema messageSchedulingStateSendAtDate td_api.tl:5902) <!-- parity:msg-scheduled-send --> (M1: schedule popup +1h/+8h/+24h via messageSchedulingStateSendAtDate)
- [x] Scheduled messages list / edit / delete <!-- parity:msg-scheduled-list --> (M1: getChatScheduledMessages dialog with list, edit (editMessageText), delete)
- [x] Send when online (schema messageSchedulingStateSendWhenOnline td_api.tl:5905) <!-- parity:msg-send-when-online --> (M1: schedule popup "when online" via messageSchedulingStateSendWhenOnline)
- [x] Silent send / disable notification (send options always null) <!-- parity:msg-silent-send --> (M1: silent toggle -> messageSendOptions.disable_notification; non-null options on every sendMessage)
- [x] Cloud drafts synced via setChatDraftMessage (debounced save) <!-- parity:msg-cloud-drafts --> (requests.rs:1537; DraftSaveClock composer.rs:333; ui/mod.rs:12226)
- [x] Draft preserves reply context <!-- parity:msg-draft-reply-context --> (begin_edit_draft composer.rs:221; updateChatDraftMessage handled)
- [x] Delivery status on outgoing (sending / sent / read) <!-- parity:msg-delivery-status --> (outgoing_status_label state.rs:893; outbox_receipt state.rs:1177)
- [x] Mark history read (viewMessages after openChat) <!-- parity:msg-mark-read --> (requests.rs:1470, 306)
- [x] Typing indicator from peer <!-- parity:msg-typing-receive --> (updateChatAction → typing_senders state.rs:930)
- [x] Send typing indicator <!-- parity:msg-typing-send --> (sendChatAction requests.rs:2392)
- [x] Retry/resend failed message (resendMessages unwired) <!-- parity:msg-retry-failed --> (M1: failed sends marked red with retry hint; right-click Retry -> resendMessages)
- [x] In-chat message search <!-- parity:msg-in-chat-search --> (search_chat_messages requests.rs:213; chat_search_input ui/mod.rs:575)
- [x] Per-message hover actions (Reply/Forward/Select/Edit/Delete/React/Pin) <!-- parity:msg-row-actions --> (ui/mod.rs:21917+)
- [x] Right-click context menu (no right-click handlers; hover buttons only) <!-- parity:msg-context-menu --> (M1: right-click menu Reply/Copy/Forward/Pin/Unpin/Share link/Retry/Delete)
- [x] Link preview options on send (partial: received previews render card ui/mod.rs:22404; send has link_preview_options:null, no toggle) <!-- parity:msg-link-preview-toggle --> (M1: previews toggle -> linkPreviewOptions.is_disabled; secret chats force off)

### Chat list

- [x] Chat rows with avatar (photo or colored initials), title, and last-message preview <!-- parity:chatlist-row -->
- [x] Unread count badge on rows, capped at 99+ (src/state.rs:883) <!-- parity:chatlist-unread-badge -->
- [x] Muted-chat speaker icon on rows (src/ui/mod.rs:21139) <!-- parity:chatlist-muted-icon -->
- [x] Row preview shows typing…, Draft:, last message, and secret-chat label (src/state.rs:1148) <!-- parity:chatlist-row-preview -->
- [x] Folder tag chips on rows when folder tags are enabled (src/ui/mod.rs:20893) <!-- parity:chatlist-folder-tags -->
- [x] Pinned chats sort first by TDLib order in the model (src/state.rs:923) <!-- parity:chatlist-pinned-order -->
- [x] Archived section listed under the main chat list (src/ui/mod.rs:18248) <!-- parity:chatlist-archive-section -->
- [x] Archive / Unarchive action in the open-chat header bar (src/ui/mod.rs:11671) <!-- parity:chatlist-archive-toggle -->
- [x] Chats | Contacts sidebar tabs (src/ui/mod.rs:7510) <!-- parity:chatlist-chats-contacts-tabs -->
- [x] Folder tabs (Main + folder names) with always-present manage entry; selecting a folder loads its chats (src/ui/mod.rs:7641) <!-- parity:chatlist-folder-tabs -->
- [x] Create/edit chat folders with include/exclude chat-type and mute/read/archive filters (src/folders.rs:21) <!-- parity:chatlist-folder-editor -->
- [x] Delete folder with confirmation (src/ui/mod.rs:11854) <!-- parity:chatlist-folder-delete -->
- [x] Per-chat add-to-folder picker backed by getChatListsToAddChat (src/ui/mod.rs:12676) <!-- parity:chatlist-folder-picker -->
- [x] Global search field with Clear button (src/ui/mod.rs:16360) <!-- parity:chatlist-search-field -->
- [x] Search sections: Recent, Chats, Messages, and Global public chats via searchPublicChats (src/ui/mod.rs:16438; src/state.rs:63) <!-- parity:chatlist-search-sections -->
- [x] Empty states: "Loading chats…", "No chats in this folder yet.", "No chats in the main list." (src/ui/mod.rs:18218) <!-- parity:chatlist-empty-states -->
- [ ] Right-click context menu on chat rows (missing entirely; mute/archive exist only in the open-chat header) <!-- parity:chatlist-row-context-menu -->
- [ ] Pin / Unpin chat (schema toggleChatIsPinned exists at schema/td_api.tl:13678, never wired to UI) <!-- parity:chatlist-pin-unpin -->
- [ ] Drag-to-reorder pinned chats <!-- parity:chatlist-pin-drag-reorder -->
- [ ] Pin-limit error handling (TGX: "you can pin up to N chats and N secret chats at once") <!-- parity:chatlist-pin-limit -->
- [ ] Mark all chats as read (schema readChatList exists, unused in src) <!-- parity:chatlist-mark-all-read -->
- [ ] Per-chat mark as read / unread (schema toggleChatIsMarkedAsUnread exists, unused in src) <!-- parity:chatlist-mark-read-unread -->
- [ ] Per-chat mute/unmute from the list (partial: open-chat header bar only) <!-- parity:chatlist-list-mute -->
- [ ] Delete chat from the list (partial: driver has a deleteChat request builder, no UI) <!-- parity:chatlist-delete-chat -->
- [ ] Clear chat history <!-- parity:chatlist-clear-history -->
- [ ] Saved Messages entry row in the list <!-- parity:chatlist-saved-messages -->
- [ ] Chat preview on long-press / hover <!-- parity:chatlist-chat-preview -->
- [ ] Clear recent searches (recents load but are not clearable) <!-- parity:chatlist-clear-recent-searches -->
- [ ] No-results state in search <!-- parity:chatlist-search-no-results -->
- [ ] Collapsible / hideable archive section (TGX: archiveCollapsed setting) <!-- parity:chatlist-archive-collapse -->
- [ ] Archive auto-settings: archive+mute chats from unknown users, keep muted/folder chats archived (schema setArchiveChatListSettings exists, unused in src) <!-- parity:chatlist-archive-auto-settings -->
- [ ] Mention/reaction counts on the unread badge (partial: plain unread count only) <!-- parity:chatlist-mention-badge -->
- [ ] Multi-select mode: Select…, Select unread <!-- parity:chatlist-multi-select -->
- [ ] Report / Block contact from the list <!-- parity:chatlist-report-block -->
- [ ] App badge counter settings: include muted chats, include archived chats, count messages vs chats <!-- parity:chatlist-badge-settings -->
- [ ] Chat list style settings: two/three lines, media icons, text formatting <!-- parity:chatlist-list-style -->
- [ ] Unread / Archived filter category chips <!-- parity:chatlist-category-filters -->

### Media

- [x] Fullscreen photo/video viewer overlay with prev/next navigation (arrow keys) and "N of M" counter (src/media_viewer.rs:74-124) <!-- parity:media-viewer -->
- [x] Zoom viewer image via `=`/`-` keys and scroll, drag-pan when zoomed, `0` resets to fit (src/media_viewer.rs:172-250, ui/mod.rs:146-150) <!-- parity:media-viewer-zoom -->
- [ ] Rotate photo in the viewer <!-- parity:media-viewer-rotate -->
- [ ] Picture-in-picture for video playback <!-- parity:media-video-pip -->
- [x] Viewer auto-downloads the current item when not local and resumes a parked play once `downloadFile` lands (ui/mod.rs:4926, 5179) <!-- parity:media-viewer-autodownload -->
- [ ] Share photo/video from the viewer <!-- parity:media-viewer-share -->
- [ ] Save viewer media to gallery / downloads folder <!-- parity:media-viewer-save -->
- [ ] "Show in chat" jump from the viewer to the source message <!-- parity:media-viewer-show-in-chat -->
- [x] Play/pause video in the viewer with elapsed/total overlay (PlaybackClock, src/playback.rs:18; ui/mod.rs:4951) <!-- parity:media-video-play -->
- [ ] Seek scrubber in the fullscreen video player (partial: scrub seek bars exist only on history rows, ui/mod.rs:1063) <!-- parity:media-video-seek -->
- [ ] Playback speed control for voice/audio/video, incl. 0.5x–2x long-press dial (TGX PlaybackSpeed*) <!-- parity:media-playback-speed -->
- [ ] Volume control / mute toggle in the video player <!-- parity:media-video-volume -->
- [ ] Playback error states for unsupported video/audio/GIF/round-video formats (TGX *PlaybackError/*PlaybackUnsupported) <!-- parity:media-playback-errors -->
- [x] Record voice note from the mic (ffmpeg OGG capture) with `chatActionRecordingVoiceNote` shown while recording (src/voice.rs:86-99, connect.rs:4796) <!-- parity:media-voice-record -->
- [ ] Lock-to-record (swipe up) and slide-to-cancel while recording (partial: click-to-record, Esc/Cancel discards, ui/mod.rs:650) <!-- parity:media-voice-lock -->
- [x] Waveform bars on voice messages decoded from TDLib 5-bit waveform (src/voice.rs:29-67) <!-- parity:media-voice-waveform -->
- [x] Voice/audio history rows with play/pause, draggable seek bar, and remembered position (ui/mod.rs:1063, 6966) <!-- parity:media-audio-player -->
- [ ] Voice note transcription display (partial: `speech_recognition_result` parsed, no UI) <!-- parity:media-voice-transcription -->
- [ ] Hold-to-record audio vs tap-to-switch video recording mode toggle (TGX HoldToAudio/HoldToVideo) <!-- parity:media-record-mode-toggle -->
- [ ] "Record HQ round videos" quality setting (TGX UseHqRoundVideos) <!-- parity:media-video-note-hq -->
- [ ] Discard-recording confirmation dialog <!-- parity:media-record-discard-confirm -->
- [x] Round video-note player with play/pause in history (ui/mod.rs:842) <!-- parity:media-video-note-player -->
- [x] Send video notes from video files with probed duration and generated square thumbnail (requests.rs:1885, src/video.rs:107-156) <!-- parity:media-video-note-send -->
- [ ] Record video note from the camera (partial: video notes attach from file only) <!-- parity:media-video-note-record -->
- [x] Music rows with title/performer/album-cover art and play/pause (ui/mod.rs:844) <!-- parity:media-music-row -->
- [x] GIF/animation frame playback in history (ui/mod.rs:688-690, 5838) <!-- parity:media-gif-playback -->
- [x] Document rows with file name and mime type, click-to-download (envelope.rs:4025, ui/mod.rs:16933) <!-- parity:media-document-row -->
- [ ] Open downloaded document with the system app / reveal in file manager <!-- parity:media-document-open -->
- [x] On-demand `downloadFile` with priority and auto-download of thumbnails in the open chat (requests.rs:1491, connect.rs:2820) <!-- parity:media-download -->
- [ ] Download progress display on history rows (partial: in-flight download state tracked, no progress bar/percent, ui/mod.rs:866) <!-- parity:media-download-progress -->
- [ ] Pause / cancel an in-flight download (schema has `cancelDownloadFile`; Quill never calls it) <!-- parity:media-download-cancel -->
- [ ] Retry a failed download <!-- parity:media-download-retry -->
- [ ] Downloads manager screen listing active/completed downloads (TGX Downloads, NoDownloadFilesFound) <!-- parity:media-downloads-manager -->
- [ ] Automatic media download settings incl. data-saver pause-all mode (partial: thumb auto-download is hardcoded, connect.rs:2820) <!-- parity:media-auto-download-settings -->
- [x] Link preview cards render on messages with site, title, description, and thumbnail (ui/mod.rs:22426) <!-- parity:media-link-preview -->
- [ ] Send-time link preview controls: disable, force small/large media, show above text (partial: `link_preview_options` always null on send, requests.rs:1529) <!-- parity:media-link-preview-send-options -->
- [ ] Instant View reader with auto-open setting (None / Telegram / All links) (partial: `instant_view_version` only passed through) <!-- parity:media-instant-view -->
- [ ] Embedded media players inside link previews (video/audio embeds) <!-- parity:media-link-preview-embedded -->
- [ ] Album-type link previews with multiple photo/video thumbnails <!-- parity:media-link-preview-album -->
- [x] Send photo/video albums of 2–10 items via `sendMessageAlbum` with composer text as caption (requests.rs:1943, connect.rs:4605) <!-- parity:media-album-send -->
- [x] Received albums grouped by `media_album_id` into grid tiles (ui/mod.rs:17592, 21223) <!-- parity:media-album-grid -->
- [ ] Open an album item in the fullscreen viewer (partial: album tiles do not open the viewer, ui/mod.rs:22718) <!-- parity:media-album-viewer -->
- [ ] Pin / unpin an album (TGX MessagePinAlbum/MessageUnpinAlbum) <!-- parity:media-album-pin -->
- [ ] "Remember media grouping" setting (TGX RememberAlbumSetting) <!-- parity:media-album-grouping-setting -->
- [ ] Per-chat shared media gallery with Media / Files / Music / Links / Voice / GIFs tabs (partial: `searchMessagesFilter*` schema constructors exist, no UI) <!-- parity:media-shared-gallery -->
- [ ] Empty states per shared-media tab (TGX NoPhotosToShowInChat etc.) <!-- parity:media-shared-gallery-empty -->
- [x] Captions render on photos, videos, animations, audio, and documents <!-- parity:media-caption-render -->
- [x] Edit a sent media caption via `editMessageCaption` (requests.rs:2123, connect.rs:4941) <!-- parity:media-caption-edit -->
- [ ] Caption position toggle: show above vs below media (partial: `show_caption_above_media` parsed, no UI) <!-- parity:media-caption-position -->
- [ ] "Add a caption…" affordance when attaching media (partial: composer text field doubles as caption) <!-- parity:media-caption-prompt -->
- [ ] Remove captions when forwarding copies (TGX RemoveCaptions) <!-- parity:media-caption-remove-on-forward -->
- [ ] Caption-too-long validation on caption edits (TGX EditMessageCaptionTooLong) <!-- parity:media-caption-length-limit -->

### Groups, supergroups & channels

- [x] Group/channel info panel with description and member/subscriber count (src/ui/mod.rs:8280) <!-- parity:groups-info-panel -->
- [ ] Create new group — no `createNewBasicGroupChat` request in Quill <!-- parity:groups-create-group -->
- [ ] Create new channel — no `createNewSupergroupChat` request in Quill <!-- parity:groups-create-channel -->
- [ ] Convert group to broadcast group (`toggleSupergroupIsBroadcastGroup`) <!-- parity:groups-convert-broadcast -->
- [ ] Add members via contact picker — no `addChatMember`/`addChatMembers` request <!-- parity:groups-add-members -->
- [ ] Browse/search member list (non-admin view) (partial: member list exists only inside the promote picker, src/ui/mod.rs:14430) <!-- parity:groups-member-list -->
- [ ] Restricted-members and banned-members lists (member-status filters) <!-- parity:groups-restricted-banned-lists -->
- [x] Administrator list with refresh, owner shown, custom titles displayed (src/ui/mod.rs:8748) <!-- parity:groups-admin-list -->
- [x] Promote member via searchable picker with 18 granular rights checkboxes (src/ui/mod.rs:266-415) <!-- parity:groups-promote -->
- [x] Edit existing admin rights, pre-filled from `getChatMember` (src/state.rs:209, src/ui/mod.rs:363) <!-- parity:groups-edit-rights -->
- [x] Demote admin with confirmation dialog (src/ui/mod.rs:363) <!-- parity:groups-demote -->
- [ ] Set/edit admin custom title (partial: titles are parsed and shown in the admin list) <!-- parity:groups-admin-title -->
- [ ] Ban/restrict member with duration, mute-until, and unban (partial: only promote/edit/demote via `setChatMemberStatus` exist, src/state.rs:29) <!-- parity:groups-restrict-ban -->
- [ ] Chat permissions editor — default "what members can do" toggles (send, stickers, polls, embed links, reactions) — no `setChatPermissions` request <!-- parity:groups-chat-permissions -->
- [x] Slow-mode delay picker (Off/5s/10s/30s/1m/5m/15m/1h, admin-gated) (src/ui/mod.rs:8309) <!-- parity:groups-slow-mode -->
- [x] Slow-mode send gate with countdown, applies to sends/forwards/voice (src/ui/mod.rs:3099) <!-- parity:groups-slow-mode-enforcement -->
- [x] Slow-mode bypass when viewer boosts meet the unrestrict threshold (src/state.rs:3034) <!-- parity:groups-slow-mode-boost-bypass -->
- [x] Invite-link list with expiry/humanised "Never expires"/"Expired" labels and pending-join-request counts (src/ui/mod.rs:8397) <!-- parity:groups-invite-link-list -->
- [x] Create invite link dialog — name, expiration, member limit, join-request toggle (src/ui/mod.rs:222) <!-- parity:groups-invite-link-create -->
- [x] Edit, copy and revoke invite links (src/ui/mod.rs:6341) <!-- parity:groups-invite-link-edit-revoke -->
- [ ] Replace primary invite link — no `replacePrimaryChatInviteLink` request <!-- parity:groups-invite-link-primary -->
- [x] Join-request list with approve/decline buttons and pending-count badge (src/ui/mod.rs:8596) <!-- parity:groups-join-requests -->
- [ ] "Approve new members" join-by-request toggle (`toggleSupergroupJoinByRequest`) <!-- parity:groups-join-by-request-toggle -->
- [x] Recent-actions event log with refresh and load-more (src/ui/mod.rs:8914) <!-- parity:groups-event-log -->
- [ ] Event-log filter picker (per-event-type / per-admin) and in-log text search (partial: `getChatEventLog` accepts filters in the request builder, src/telegram/requests.rs:725, but no UI picker) <!-- parity:groups-event-log-filters -->
- [x] Channel/group statistics panel with graphs and top senders/administrators/inviters, gated on `can_get_statistics` (src/ui/mod.rs:9195) <!-- parity:groups-statistics -->
- [x] Author signatures rendered on channel posts (src/ui/mod.rs:22014) <!-- parity:groups-author-signatures-display -->
- [ ] Author-signatures toggle for channel (`toggleSupergroupSignMessages`) <!-- parity:groups-author-signatures-toggle -->
- [x] Forum topic list with per-topic history and posting to topics (src/connect.rs:1443) <!-- parity:groups-forum-browse -->
- [ ] Create/edit/close/pin/hide forum topics — no `createForumTopic` family requests <!-- parity:groups-forum-manage -->
- [x] "Discuss" jump to the linked discussion group (src/ui/mod.rs:12638) <!-- parity:groups-discussion-jump -->
- [ ] Channel comments viewer ("view comments" in the discussion group) (partial: only the jump to the discussion group exists) <!-- parity:groups-channel-comments -->
- [ ] Aggressive anti-spam toggle (`toggleSupergroupHasAggressiveAntiSpamEnabled`) <!-- parity:groups-anti-spam -->
- [ ] Boost status/level display and boost action (partial: boost counts are parsed only for the slow-mode bypass) <!-- parity:groups-boost -->
- [ ] Public username management for group/channel (`setSupergroupUsername`) <!-- parity:groups-public-username -->
- [x] Leave channel (src/ui/mod.rs:17302) <!-- parity:groups-leave -->
- [ ] Delete group/channel for everyone <!-- parity:groups-delete -->

### Secret chats

- [x] Start secret chat from a contact's profile (non-bot, non-self) via createNewSecretChat (src/telegram/requests.rs:751, src/ui/mod.rs:4432) <!-- parity:secret-start -->
- [x] "🔒 New secret chat" sidebar entry with an eligible-contact picker (non-bot, non-self) reusing createNewSecretChat (src/ui/mod.rs) <!-- parity:secret-new-from-menu -->
- [x] Secret-chat state lifecycle: pending → ready → closed rendered from updateSecretChat / getSecretChat (src/telegram/envelope.rs:767, src/state.rs:3587) <!-- parity:secret-state-lifecycle -->
- [x] 🔒 secret-chat badge in chat list and header state rows ("Secret chat closed", "Loading secret chat…") (src/ui/mod.rs:17394, src/ui/mod.rs:2339) <!-- parity:secret-badge -->
- [x] "Close this secret chat?" confirm dialog (closing is permanent, no new messages) wired to closeSecretChat (src/ui/mod.rs:14295) <!-- parity:secret-close -->
- [x] Distinct close-confirm texts per secret-chat state (pending: "cancel the secret chat"; closed: "delete … cannot be undone"; ready: "delete … history deleted forever") with the peer's name — TGX DeleteSecretChat{Pending,Closed,}Confirm verbatim (src/ui/mod.rs) <!-- parity:secret-close-variants -->
- [x] "Waiting for {name} to get online…" subtitle in the conversation header for pending secret chats (TGX AwaitingEncryption) plus the existing composer note (src/ui/mod.rs) <!-- parity:secret-pending-banner -->
- [x] Encryption key fingerprint: 12×12 pixel grid from secretChat.key_hash rendered in the chat info panel with a still-loading state (src/key_fingerprint.rs:56, src/ui/mod.rs:7963) <!-- parity:secret-key-grid -->
- [x] Encryption-key guarantee text with the peer's name next to the fingerprint (TGX EncryptionKeyDescription verbatim: "If they look the same on {name}'s device, end-to-end encryption is guaranteed") (src/ui/mod.rs) <!-- parity:secret-key-description -->
- [x] End-to-end-encryption explainer ("🔒 Secret chats" + the four TGX EncryptedDescription bullets) rendered as real app UI for empty secret chats — no fake injected messages (src/ui/mod.rs) <!-- parity:secret-e2e-notice -->
- [x] Per-media self-destruct timer/view-once picker in the composer (self_destruct_type on photo/video/voice/video-note inputs, live countdown + view-once in demo) (src/telegram/requests.rs:1614, src/ui/mod.rs:608) <!-- parity:secret-selfdestruct-media -->
- [x] Chat-level self-destruct timer for secret chats (incl. text messages): ⏱ picker (Off/5s/30s/1m/1h/1d/1w), timer status line, "X set timer …" service rows, per-message auto_delete_in countdown chips (setChatMessageAutoDeleteTime; connect.rs:2513, ui/mod.rs:12882/11611, envelope.rs:3137) <!-- parity:secret-ttl-timer -->
- [x] Incoming/outgoing "X took a screenshot" rendered as a chat service message (messageScreenshotTaken parsed in envelope.rs; "You took a screenshot" for own, "{name} took a screenshot" for peer) (src/telegram/envelope.rs, src/ui/mod.rs) <!-- parity:secret-screenshot-notify -->
- [ ] Send screenshot-taken notification when the user screenshots (blocked: Linux desktop has no OS-level screenshot-detection API to trigger it; the TDLib mechanism exists — viewMessages with messageSourceScreenshot, td_api.tl:13230/:3234, as Telegram X's sendScreenshotMessage does — but nothing on Linux can tell us a screenshot happened) <!-- parity:secret-screenshot-send -->
- [ ] Screenshot capture prevention for secret chats on the desktop (blocked: no OS-level screen-capture prevention API on Linux — no FLAG_SECURE equivalent on X11/Wayland; verified 2026-09-27) <!-- parity:secret-screenshot-block -->
- [x] Forwarding to/from secret chats: destination gate in submit_forward_to shows TGX's verbatim "This message cannot be forwarded to secret chats." (picker stays open); source gate hides the Forward/Select buttons on secret-chat messages, mirroring TGX only offering Forward when messageProperties.canBeForwarded (MessagesController.java:5287) (src/ui/mod.rs) <!-- parity:secret-no-forward -->
- [x] Inline-bot warning alert in secret chats (Telegram X's `SecretChatContextBotAlert`: gates the `SwitchInline` button path in secret chats — Quill's only inline-bot invocation point; first press in a session shows the verbatim TGX warning above the composer with Confirm-only behavior, then inserts the query) <!-- parity:secret-bot-alert -->
- [x] Link previews off in secret chats: sendMessage emits explicit linkPreviewOptions{is_disabled:true} for secret chats (previews are generated on Telegram servers, which can't see E2E content); the TGX opt-in alert to enable server-side previews is not yet surfaced (src/telegram/requests.rs, src/connect.rs) <!-- parity:secret-link-preview -->
- [x] "{name}'s Telegram client doesn't support this feature. They need to install an update first." notice when sending a video note in a secret chat whose layer < 66 (TGX SecretChatFeatureUnsupported / chatSupportsRoundVideos gate) (src/ui/mod.rs) <!-- parity:secret-feature-unsupported -->
- [ ] "Toggle whether a session can accept incoming secret chats" privacy toggle (partial: toggleSessionCanAcceptSecretChats request builder exists in requests.rs; no sessions UI surface to expose it — TGX's per-session "Session Accepts → Secret Chats" toggle in EditSessionController) <!-- parity:secret-session-accept -->
- [x] Secret-chat notification settings: "Custom notification settings for the Secret Chat with {name}." label (TGX NotificationChannelSecretChat) in the existing per-chat notification panel, which fully applies to secret chats; lock-screen hide behavior (TGX HideSecret/ShowSecretOn) is out of slice — Linux desktop exposes no lock-screen notification API (src/ui/mod.rs) <!-- parity:secret-notif-settings -->
- [x] Per-secret-chat passcode hint in the chat info panel (TGX SecretPasscodeInfo verbatim with the peer's name); actually setting an additional per-chat passcode needs a full app passcode feature — out of slice (src/ui/mod.rs) <!-- parity:secret-passcode -->
- [x] "Secret media and files" category in storage usage (new "💾 Storage usage" sidebar entry backed by getStorageStatistics; overlay lists TGX-ordered categories with counts and sizes, including the Secret media and files row for fileTypeSecret) <!-- parity:secret-storage-category -->
- [ ] Secret-chat count shown when terminating a session that would cancel secret chats (out of slice: Quill has no sessions UI at all — no getActiveSessions surface — so TGX's ClosingXSecretChats/SessionSecretChats strings have no parent screen) <!-- parity:secret-session-terminate -->

### Stories

#### View stories
- [x] Active-story tray above the chat list, unread ring vs muted read ring (ui/mod.rs:7566 story_tray; DECISIONS.md Phase 9.1) <!-- parity:stories-tray -->
- [x] Tap a tray entry opens fullscreen viewer on the latest story; missing details prefetched via getStory (ui/mod.rs:5427 open_story_viewer) <!-- parity:stories-viewer-open -->
- [x] Viewer renders photo stories at full size and video stories (video shows thumbnail only; full clip not renderable by the img element) (src/story_viewer.rs; DECISIONS.md Phase 9.1) <!-- parity:stories-viewer-photo-video -->
- [x] Viewer shows poster name + "Story N of M" + caption with formatted entities (src/story_viewer.rs:55-56 caption fields) <!-- parity:stories-viewer-caption -->
- [x] Viewer Prev / Next / Close; Escape closes viewer before other overlays (DECISIONS.md Phase 9.1) <!-- parity:stories-viewer-nav -->
- [ ] Segmented progress bar with auto-advance to next story <!-- parity:stories-progress-bar -->
- [x] Live/unsupported story content degrades to a placeholder in the item list (src/story_viewer.rs:11-13) <!-- parity:stories-live-placeholder -->
- [ ] Join or play live stories (storyContentLive / startLiveStory in schema) <!-- parity:stories-live-play -->
- [x] openStory/closeStory mark stories viewed; read state from max_read_story_id (telegram/requests.rs:2519,2531) <!-- parity:stories-read-state -->
- [x] Quick-react ❤️ toggle on viewer, chosen state shown (ui/mod.rs:5505-5566) <!-- parity:stories-quick-react -->
- [x] Reaction picker fed by getStoryAvailableReactions (ui/mod.rs:5532 toggle_story_reaction_picker) <!-- parity:stories-reaction-picker -->
- [ ] Chosen custom-emoji or paid reactions (parser drops them: chosen_reaction_emoji emoji-only, telegram/envelope.rs Phase 9.2) <!-- parity:stories-custom-reactions -->
- [x] Reaction removal (setStoryReaction with null; request asserts, telegram/requests.rs:2556) <!-- parity:stories-reaction-remove -->
- [x] Interaction counters (views / hearts / reposts, non-zero, when can_get_interactions) (ui/mod.rs Phase 9.2) <!-- parity:stories-interaction-counters -->
- [ ] Detailed viewers list (getStoryInteractions — explicit schema surface kept out of Phase 9.2; not called anywhere) <!-- parity:stories-viewers-list -->
- [x] Text reply to a story via sendMessage + inputMessageReplyToStory, gated on can_be_replied (telegram/requests.rs Phase 9.2; ui/mod.rs Reply row) <!-- parity:stories-reply -->
- [x] Delete own story, gated on can_be_deleted; viewer closes when the story leaves the cache; updateStoryDeleted handled (telegram/requests.rs:2584, telegram/envelope.rs:4660) <!-- parity:stories-delete -->
- [ ] Report story (reportStory in schema, unused) <!-- parity:stories-report -->
- [ ] Stealth mode / hide view from poster (activateStoryStealthMode, updateStoryStealthMode in schema; TGX consumes them, Quill does not) <!-- parity:stories-stealth-mode -->
- [ ] Clickable story areas (location, venue, suggested reaction, message, link, weather, gift) — parser drops areas (telegram/envelope.rs:3328) <!-- parity:stories-areas-view -->
- [ ] Story albums: view albums, add to album (schema getChatStoryAlbums / createStoryAlbum / ... in 1.8.67, unused) <!-- parity:stories-albums -->
- [ ] Archive story list (storyListArchive trays remove the row; no archive UI) (DECISIONS.md Phase 9.1 "Out of this slice") <!-- parity:stories-archive -->
- [ ] Pinned stories on chat page (getChatPostedToChatPageStories, setChatPinnedStories unused) <!-- parity:stories-pinned -->
- [ ] Story notification settings (mute stories per chat, story sound, show story poster) — parsed into fields only (telegram/envelope.rs:1859-1860) <!-- parity:stories-notify-settings -->
- [ ] "Only admins can send stories in this group" and story-restriction notices (TGX strings ChatDisabledStory / ChatRestrictedStory / ChatRestrictedStoryUntil; no Quill handling) <!-- parity:stories-restriction-notice -->

#### Post stories
- [ ] Story composer: photo picker (partial: post-success/fail updates parsed — telegram/envelope.rs:4774,4786) <!-- parity:stories-post-photo-composer -->
- [ ] Video story upload (partial: inputStoryContentVideo exists in schema, unused) <!-- parity:stories-post-video -->
- [ ] canPostStory eligibility check before posting (schema td_api.tl:13702, unused) <!-- parity:stories-can-post-check -->
- [ ] postStory with caption + privacy selector (partial: schema constructor verified td_api.tl:13715; post-succeeded/failed reducer upserts story + queues tray refresh, DECISIONS.md 9.2) <!-- parity:stories-post-call -->
- [ ] Privacy selector: Everyone / Contacts / Close friends / Selected users (schema storyPrivacySettings*, td_api.tl:8928-8937; unused) <!-- parity:stories-post-privacy -->
- [ ] Formatted caption with entities on post <!-- parity:stories-post-caption -->
- [ ] Add story areas (stickers, links, reactions) on own post <!-- parity:stories-post-areas -->
- [ ] Active period / expiry selection (postStory active_period param unused) <!-- parity:stories-post-expiry -->
- [ ] "Post to chat page" toggle (postStory is_posted_to_chat_page / toggleStoryIsPostedToChatPage unused) <!-- parity:stories-post-to-chat-page -->
- [ ] "Protect content" (no forwarding) toggle (postStory protect_content param unused) <!-- parity:stories-post-protect -->
- [ ] Honest pending / succeeded / failed states (partial: updateStoryPostSucceeded/PostFailed parsed and applied — telegram/envelope.rs:10966,10985; no UI states) <!-- parity:stories-post-status -->
- [ ] Edit own story content / caption / areas (editStory, td_api.tl:13732, unused) <!-- parity:stories-edit -->
- [ ] Edit story cover frame (editStoryCover, td_api.tl:13738, unused) <!-- parity:stories-edit-cover -->
- [ ] Change a posted story's privacy settings (setStoryPrivacySettings, td_api.tl:13743, unused) <!-- parity:stories-post-change-privacy -->
- [ ] Post stories on behalf of a channel/supergroup (getChatsToPostStories, canPostStory chat_id unused) <!-- parity:stories-post-as-channel -->
- [ ] Admin story rights management (post / edit / delete others' stories; TGX strings RightStories*; not in Quill) <!-- parity:stories-admin-rights -->
- [ ] Repost / re-share a story (storyRepostInfo / storyInteractionTypeRepost in schema; parser drops repost info) <!-- parity:stories-repost -->

### Calls

- [x] Start voice call from user profile <!-- parity:calls-start-voice --> (ui/mod.rs:8195 → start_call_for_user:4459; createCall requests.rs:809)
- [x] Accept incoming call <!-- parity:calls-accept --> (accept_incoming_call ui/mod.rs:4547; acceptCall requests.rs:843)
- [x] Decline incoming call <!-- parity:calls-decline --> (call-decline button ui/mod.rs:9963; discardCall requests.rs:881)
- [x] End active call (hang up) <!-- parity:calls-hangup --> (hang_up_call ui/mod.rs:4562)
- [x] Call state indicators (calling / connecting / exchanging keys) <!-- parity:calls-states --> (ui/mod.rs:9768)
- [x] Mute / unmute microphone during call <!-- parity:calls-mute --> (toggle_call_mute ui/mod.rs:4483; set_call_muted connect.rs:1871)
- [x] Microphone and speaker output selection during call <!-- parity:calls-devices --> (select_call_devices connect.rs:1891; picker ui/mod.rs:4523)
- [ ] Incoming-call-while-busy swap prompt (partial: no Telegram-level hold/swap API; incoming call is auto-declined with an explanatory banner, state.rs) <!-- parity:calls-swap-prompt -->
- [x] Call failed / offline / microphone-missing error states <!-- parity:calls-errors --> ("Call failed" card ui/mod.rs:9668)
- [x] Reconnect indicator when audio transport drops <!-- parity:calls-reconnect --> (TransportState::Reconnecting calls/engine.rs:27; shown ui/mod.rs:9787)
- [x] Call end summary screen with duration <!-- parity:calls-summary --> (CallSummary state.rs:2006)
- [x] Rate call quality after call <!-- parity:calls-rating --> (open_rating_detail ui/mod.rs:5009; submit_call_rating ui/mod.rs:5030; sendCallRating requests.rs:899)
- [x] Rating problems and comment (stars + problem chips + optional comment sent via sendCallRating) <!-- parity:calls-rating-detail -->
- [x] Send call debug information to Telegram <!-- parity:calls-debug --> (send_call_debug_information connect.rs:1984)
- [x] Call log file upload (sendCallLog with inputFileLocal; button on end screen when need_log) <!-- parity:calls-log-upload -->
- [x] Recent calls list (Calls tab: searchCallMessages with Load more) <!-- parity:calls-history -->
- [x] Missed / declined / canceled call entries in chat (messageCall service rows) <!-- parity:calls-chat-messages -->
- [x] Call again from summary or chat entry (end screen + service rows + recent calls) <!-- parity:calls-again -->
- [x] Confirm before calling setting (persisted per-account call_prefs.json; gates startCall) <!-- parity:calls-confirm -->
- [x] "Who can call me" privacy setting (getUserPrivacySettingRules / setUserPrivacySettingRules) <!-- parity:calls-privacy -->
- [x] Peer-to-peer call relay toggle (getUserPrivacySettingRules / setUserPrivacySettingRules) <!-- parity:calls-p2p -->
- [ ] Less data for calls setting (partial: persisted in call_prefs.json, but the native call engine exposes no data-saving API) <!-- parity:calls-less-data -->
- [ ] Use proxy for calls setting <!-- parity:calls-proxy -->
- [ ] Echo cancellation / noise suppression toggles (partial: ntgcalls defaults only, no settings UI) <!-- parity:calls-audio-fx -->
- [x] Start video call with working video (video negotiated when a camera exists via `video_wanted`; peer frames flow through the engine callbacks to the video stage — verified in code + screenshot; real camera/peer still unverified) <!-- parity:calls-start-video -->
- [x] Camera preview (local video tile) in video call (renders latest driver frame as a 160x120 PiP; "Starting camera…" / "Camera off" states when no frame) <!-- parity:calls-camera-preview -->
- [x] Remote video frames in video call (peer camera as the main tile; Connecting/paused/off state text from `RemoteVideoState`) <!-- parity:calls-remote-video -->
- [x] Switch camera during video call (camera on/off toggle drives `set_camera_enabled`; camera picker re-applies the selected device on the active call) <!-- parity:calls-camera-switch -->
- [x] Camera device selection (Camera picker row with radio selection; "No camera found." when the engine reports none) <!-- parity:calls-camera-select -->
- [ ] 1:1 call verification emojis (partial: parsed and shown only for group calls) <!-- parity:calls-verify-emoji -->
- [ ] Share screen in a call (partial: screen-sharing participants are detected and flagged, ui/mod.rs:10150; 1:1 share UI not started — see group screen-share item below) <!-- parity:calls-screen-share -->
- [x] Join group voice chat <!-- parity:calls-join --> (group-call-join button ui/mod.rs:10817; join_group_call requests.rs:1101)
- [x] Leave group voice chat <!-- parity:calls-leave --> ("Leave" button ui/mod.rs:10407; leave_group_call connect.rs:2161)
- [x] Participant list with live speaking indicators <!-- parity:calls-participants --> (ui/mod.rs:10144)
- [x] Mute / unmute self in group call <!-- parity:calls-group-mute-self --> (ui/mod.rs:10375)
- [x] Mute / unmute a participant (admin) <!-- parity:calls-mute-participant --> (ui/mod.rs:10203; toggle_group_call_participant_is_muted requests.rs:1233)
- [x] Mute new participants by default <!-- parity:calls-mute-new --> (toggle ui/mod.rs:10740; requests.rs:1278)
- [x] Raise / lower hand <!-- parity:calls-hand --> (group-call-hand button ui/mod.rs:10383; toggle_group_call_self_hand:10627; requests.rs:1255)
- [x] Hand-raised badge on participants <!-- parity:calls-hand-badge --> (ui/mod.rs:10144)
- [x] Per-participant volume control (`setGroupCallParticipantVolumeLevel`, 1-20000, per-row stepper) <!-- parity:calls-participant-volume -->
- [x] Invite participants to group call (`inviteGroupCallParticipant` + contact picker) <!-- parity:calls-invite -->
- [x] Group call invitation UI: incoming `messageGroupCall` row with Accept (`joinGroupCall`) / Decline (`declineGroupCallInvitation`) <!-- parity:calls-decline-invite -->
- [x] Ban participant from group call (`banGroupCallParticipants`, owner-gated, per schema) <!-- parity:calls-ban -->
- [x] Group call verification emojis <!-- parity:calls-group-verify-emoji --> (ui/mod.rs:10325; state.rs:2043)
- [x] Toggle my video in group video chat (drives the native group camera via `set_group_camera`, not just signaling; live camera unverified) <!-- parity:calls-group-video-toggle --> (toggle_group_call_video ui/mod.rs:10667)
- [ ] Group video tiles show live video (partial: engine→tile path implemented + mock-tested + demo screenshot; live group call never exercised — no ntgcalls runtime on this VM) <!-- parity:calls-video-tiles -->
- [ ] Share screen in a group call (partial: startGroupCallScreenSharing/endGroupCallScreenSharing + native presentation handshake implemented; screen-source availability gate added; native screen source unverified — no ntgcalls runtime on this VM) <!-- parity:calls-group-screen-share -->
- [ ] Group video paused indicator (partial: `is_paused` parsed from video_info, not shown on tiles) <!-- parity:calls-group-video-pause -->
- [ ] Local camera preview tile in group calls (partial: remote tiles render; local capture frames are dropped at the native callback, so the self tile stays an avatar) <!-- parity:calls-group-video-self -->
- [x] Auto-rejoin group call after network loss (auto-rejoin on `need_rejoin`, max 3 attempts, manual retry resets) <!-- parity:calls-rejoin -->
- [x] Record group call (`startGroupCallRecording` video / `endGroupCallRecording`, `can_be_managed`-gated; REC indicator with duration; code + demo only, live recording unverified) <!-- parity:calls-recording -->
- [x] RTMP stream key for video chat (`getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl`, fetch admin-gated, regenerate owner-gated; code + demo only, live unverified) <!-- parity:calls-rtmp -->
- [x] Set / rename video chat title (`setVideoChatTitle`, 1-64 chars, `can_be_managed`-gated; code + demo only, live unverified) <!-- parity:calls-title -->
- [x] Schedule video chat for later (`createVideoChat` start_date with 10s–8d validation; scheduled card shows start time + admin-only **Start now** (`startScheduledVideoChat`), Join appears once TDLib activates the call; code + demo only, live activation unverified) <!-- parity:calls-schedule -->
- [ ] Notify me when a scheduled video chat starts (`toggleVideoChatEnabledStartNotification`, scheduled-only; not implemented) <!-- parity:calls-schedule-notify -->
- [x] Video chat invite link (`getVideoChatInviteLink` with Copy, `revokeGroupCallInviteLink`; code + demo only, live unverified) <!-- parity:calls-invite-link -->
- [x] In-call chat messages for group calls (`sendGroupCallMessage` + live `updateNewGroupCallMessage` feed with composer, gated on `can_send_messages`/`are_messages_allowed`; no history getter exists in the schema, so live feed only; code + demo only, live unverified) <!-- parity:calls-group-messages -->

### Stickers, emoji & GIFs

- [x] Sticker picker with installed sets + grid, tap to send <!-- parity:stickers-picker -->
- [x] Send sticker via inputMessageSticker <!-- parity:stickers-send -->
- [x] Load installed regular sticker sets (getInstalledStickerSets + getStickerSet per set) <!-- parity:stickers-installed -->
- [x] Sticker picker loading / error states (state.rs:4844-4849) <!-- parity:stickers-picker-states -->
- [x] GIF picker with saved GIFs (getSavedAnimations), tap to send via inputMessageAnimation <!-- parity:gifs-saved-picker -->
- [x] GIF picker refreshes when updateSavedAnimations arrives (state.rs:4397-4398) <!-- parity:gifs-saved-refresh -->
- [x] Emoji reactions picker (emoji-only) + reaction chips on messages (ui/mod.rs:631, state.rs:1298-1308) <!-- parity:emoji-reactions -->
- [x] Sticker thumbnails shown in picker and history (state.rs:5215, ui/mod.rs:14067) <!-- parity:stickers-thumbnails -->
- [ ] Trending sticker sets tab (partial: picker opens only; no getTrendingStickerSets/viewTrendingStickerSets) <!-- parity:stickers-trending -->
- [ ] Search sticker sets / stickers (searchStickerSets/searchStickers) <!-- parity:stickers-search -->
- [ ] Favorites sticker tab (getFavoriteStickers/addFavoriteSticker/removeFavoriteSticker) <!-- parity:stickers-favorites -->
- [ ] Recent stickers tab + clear recent stickers (getRecentStickers/clearRecentStickers) <!-- parity:stickers-recent -->
- [ ] Install sticker set (changeStickerSet install) <!-- parity:stickers-install -->
- [ ] Remove sticker set with confirm dialog <!-- parity:stickers-remove -->
- [ ] Archive sticker set + Archived view in settings (restore path) <!-- parity:stickers-archive -->
- [ ] Reorder installed sticker sets <!-- parity:stickers-reorder -->
- [ ] Dynamic set order (auto-place recently used sets above others) <!-- parity:stickers-dynamic-order -->
- [ ] Open sticker set preview screen (title, stickers grid, install/remove from preview) <!-- parity:stickers-set-preview -->
- [ ] "No sticker sets installed" empty state <!-- parity:stickers-empty-state -->
- [ ] "X sets installed" counts and batch install/remove feedback <!-- parity:stickers-install-counts -->
- [ ] Sticker suggestions by emoji in composer (Installed + recommended / Only installed / None) <!-- parity:stickers-suggest-by-emoji -->
- [ ] Animated sticker (TGS) playback in picker and history (partial: format parsed, only thumbnails rendered) <!-- parity:stickers-animated-playback -->
- [ ] Video sticker (WebM) playback (partial: format parsed, static thumb only) <!-- parity:stickers-video-playback -->
- [ ] "Loop Animated Stickers" setting <!-- parity:stickers-loop-setting -->
- [ ] Premium sticker gating ("Sending this sticker requires Telegram Premium") <!-- parity:stickers-premium-gate -->
- [ ] Show "choosing a sticker" chat action of others <!-- parity:stickers-typing-action -->
- [ ] GIF search + trending GIFs (inline bot path searchInlineBots/getInlineQueryResults) <!-- parity:gifs-search-trending -->
- [ ] Save GIF to media keyboard (addSavedAnimation) <!-- parity:gifs-save -->
- [ ] Delete saved GIF (removeSavedAnimation + confirm) <!-- parity:gifs-delete -->
- [ ] "No GIFs" empty state <!-- parity:gifs-empty-state -->
- [ ] "Autoplay GIFs" setting <!-- parity:gifs-autoplay-setting -->
- [ ] GIF loop playback in history (partial: static frame cache only, ui/mod.rs:3015) <!-- parity:gifs-history-playback -->
- [ ] Emoji picker in composer with categories (Smileys & People, etc.) and search <!-- parity:emoji-picker -->
- [ ] Insert emoji at cursor in composer text <!-- parity:emoji-insert -->
- [ ] Big emoji rendering for emoji-only messages (Big Emoji setting) <!-- parity:emoji-big -->
- [ ] Custom emoji packs (browse/install/remove, "Emoji Sets" settings screen) <!-- parity:emoji-custom-packs -->
- [ ] Render custom emoji inside message text <!-- parity:emoji-custom-render -->
- [ ] Suggest animated emoji in composer <!-- parity:emoji-suggest-animated -->
- [ ] Emoji status: select/set status, timed status (1h/2h/8h/2d/custom), trending statuses <!-- parity:emoji-status -->
- [ ] Clear recent emoji statuses <!-- parity:emoji-status-clear-recent -->
- [ ] Clear recent emoji <!-- parity:emoji-clear-recent -->
- [ ] Dynamic emoji pack order setting <!-- parity:emoji-dynamic-pack-order -->
- [ ] Emoji pack download states (Downloading…/Downloaded/Update Needed/Installing…) <!-- parity:emoji-pack-states -->
- [ ] "Send Stickers & GIFs" permission denied messaging in groups <!-- parity:stickers-permission-messaging -->
- [ ] Group sticker set management (setSupergroupStickerSet / setSupergroupCustomEmojiStickerSet) <!-- parity:stickers-group-set -->

### Bots, polls & payments

- [x] Inline keyboard rows rendered under messages with per-button styles (ui/mod.rs:21636) <!-- parity:bots-inline-keyboard-render -->
- [x] URL buttons open the link (ui/mod.rs:21663) <!-- parity:bots-inline-url -->
- [x] Callback buttons answered via getCallbackQueryAnswer; bot answer shown as toast / URL opened (connect.rs:4786, ui/mod.rs:3941) <!-- parity:bots-inline-callback -->
- [x] Switch-inline buttons insert "@bot query" into the composer (ui/mod.rs:21678) <!-- parity:bots-inline-switch -->
- [x] Copy-text buttons copy the text (ui/mod.rs:21681) <!-- parity:bots-inline-copy -->
- [ ] Login URL buttons (partial: renders with "Login buttons are not supported yet" tooltip) <!-- parity:bots-inline-login -->
- [ ] Web app buttons (partial: renders with "Web App buttons are not supported yet" tooltip; no getWebAppUrl code) <!-- parity:bots-inline-webapp -->
- [ ] Password-protected callback buttons (partial: tooltip only) <!-- parity:bots-inline-callback-password -->
- [ ] Game buttons (partial: "Game buttons are not supported yet" tooltip; no sendGame/score code) <!-- parity:bots-inline-game -->
- [ ] Buy buttons (partial: "Payment buttons are not supported yet" tooltip; invoice messages fall back to Unsupported) <!-- parity:bots-inline-buy -->
- [ ] User buttons (partial: "User buttons are not supported yet" tooltip) <!-- parity:bots-inline-user -->
- [ ] Custom reply keyboards (keyboardButton types parsed in envelope.rs:8548 but never rendered) <!-- parity:bots-custom-keyboard -->
- [ ] Force-reply markup (no UI) <!-- parity:bots-force-reply -->
- [x] Bot info panel with description and tappable /command buttons inserting into the composer (ui/mod.rs:13726) <!-- parity:bots-info-panel -->
- [ ] Bot START button / start_parameter deep links <!-- parity:bots-start -->
- [ ] Restart bot <!-- parity:bots-restart -->
- [ ] Share bot <!-- parity:bots-share -->
- [ ] Block / unblock bot <!-- parity:bots-block -->
- [ ] Bot menu button / main web app launch <!-- parity:bots-menu-button -->
- [ ] Bot privacy settings <!-- parity:bots-privacy -->
- [ ] Similar bots tab in profile <!-- parity:bots-similar -->
- [x] `/` command menu merging chat-specific bot commands and global getCommands (state.rs:3100) <!-- parity:bots-command-menu -->
- [ ] Inline mode: type @bot in composer, inline query results list, send an inline result (no getInlineQueryResults code in src) <!-- parity:bots-inline-mode -->
- [ ] Games: send / play, high scores (no game code at all) <!-- parity:bots-games -->
- [x] Poll creation dialog: question, add/remove options (2–10), validation errors, anonymous + multiple-answer toggles (ui/mod.rs:14860, poll.rs:112) <!-- parity:bots-poll-create -->
- [ ] Create quiz polls: mark correct option, write quiz explanation (dialog explicitly excludes quiz; quiz answers only render in results) <!-- parity:bots-poll-create-quiz -->
- [ ] Poll description field (shown above the poll title) <!-- parity:bots-poll-description -->
- [ ] Poll duration setting <!-- parity:bots-poll-duration -->
- [ ] Revoting toggle in creation dialog (partial: revoting honored when the server allows it) <!-- parity:bots-poll-revoting -->
- [ ] Shuffle options toggle <!-- parity:bots-poll-shuffle -->
- [ ] Show voters toggle <!-- parity:bots-poll-show-voters -->
- [ ] Country restriction setting <!-- parity:bots-poll-countries -->
- [ ] Poll discard-confirmation prompt <!-- parity:bots-poll-discard -->
- [x] Voting: single, multiple, retract-when-revoting, quiz answering, closed-poll blocked, optimistic UI with rollback on failure (poll.rs:51, connect.rs:5287) <!-- parity:bots-poll-vote -->
- [x] Live poll updates applied in place via updatePoll (state.rs:3962) <!-- parity:bots-poll-live-update -->
- [x] Results: percentage bars, voter counts, chosen marks, correct-answer mark on closed quizzes (ui/mod.rs:21764) <!-- parity:bots-poll-results -->
- [ ] View voter list (getPollVoters) <!-- parity:bots-poll-voters -->
- [ ] Stop poll / stop quiz with confirmation warning (zero stopPoll usage in src) <!-- parity:bots-poll-stop -->
- [ ] Quiz explanation shown after answering <!-- parity:bots-poll-quiz-explanation -->
- [ ] Vote restriction reasons display (closed / country / membership) <!-- parity:bots-poll-restrictions -->
- [ ] Poll entry gated on can_send_polls; restricted-poll notices <!-- parity:bots-poll-permissions -->
- [ ] Poll stopped service message <!-- parity:bots-poll-service-message -->
- [ ] Invoice message rendering (falls back to Unsupported) <!-- parity:bots-payment-invoice -->
- [ ] Payment checkout flow (order info, shipping, card credentials) <!-- parity:bots-payment-checkout -->
- [ ] Payment receipts <!-- parity:bots-payment-receipt -->
- [ ] Recurring payments <!-- parity:bots-payment-recurring -->
- [ ] Clear payment/shipping info (privacy) <!-- parity:bots-payment-clear -->

### Settings

- [ ] Light/dark theme switcher <!-- parity:settings-theme-switch -->
- [ ] Auto-night mode (system/scheduled) <!-- parity:settings-auto-night -->
- [ ] Accent color picker <!-- parity:settings-accent-color -->
- [ ] Chat background / wallpaper <!-- parity:settings-chat-wallpaper -->
- [ ] Message font size <!-- parity:settings-font-size -->
- [ ] Bubble vs plain chat style <!-- parity:settings-bubble-style -->
- [x] Per-chat mute presets (1h / 8h / 2d / forever), live via chat panel (src/ui/mod.rs:11564; connect.rs:5424 `set_chat_mute_for` → `setChatNotificationSettings`) <!-- parity:settings-chat-mute -->
- [x] Per-chat message preview toggle, live (src/ui/mod.rs:13028 `apply_chat_preview`) <!-- parity:settings-chat-preview -->
- [x] Per-chat custom notification sound picker, live (src/ui/mod.rs:12844; `getSavedNotificationSounds`) <!-- parity:settings-chat-sound -->
- [x] Default mute per scope (private / groups / channels), live "Notification defaults" dialog (src/ui/mod.rs:13274 `apply_scope_mute`, `setScopeNotificationSettings`) <!-- parity:settings-scope-mute -->
- [x] Default message preview per scope, live (src/ui/mod.rs:13318 `apply_scope_preview`) <!-- parity:settings-scope-preview -->
- [x] Default notification sound per scope, live (src/ui/mod.rs:13229 `apply_scope_sound`) <!-- parity:settings-scope-sound -->
- [ ] Mentions/replies and pinned-message notification overrides <!-- parity:settings-mentions-pinned -->
- [ ] Reaction and story notification settings <!-- parity:settings-reaction-notif -->
- [ ] List of chats with custom notification exceptions (partial: per-chat settings can be set, but no exceptions list view) <!-- parity:settings-notif-exceptions -->
- [ ] Reset all notification settings <!-- parity:settings-reset-notif -->
- [ ] In-app notification sounds toggle <!-- parity:settings-inapp-sound -->
- [ ] Privacy: Last Seen & Online <!-- parity:settings-privacy-lastseen -->
- [ ] Privacy: Phone Number visibility <!-- parity:settings-privacy-phone -->
- [ ] Privacy: Profile Photos visibility <!-- parity:settings-privacy-photo -->
- [ ] Privacy: Forward My Messages (link in forwarded messages) <!-- parity:settings-privacy-forwards -->
- [ ] Privacy: Call Me, incl. peer-to-peer calls <!-- parity:settings-privacy-calls -->
- [ ] Privacy: Add Me to Groups and Channels <!-- parity:settings-privacy-invites -->
- [ ] Privacy: See My Read Date <!-- parity:settings-privacy-readreceipts -->
- [ ] Blocked users list <!-- parity:settings-blocked-users -->
- [ ] Privacy exceptions per rule (always allow / never allow user lists) <!-- parity:settings-privacy-exceptions -->
- [ ] Auto-download per network (mobile / Wi-Fi / roaming) and media type <!-- parity:settings-auto-download -->
- [ ] Use less data for calls <!-- parity:settings-less-data-calls -->
- [ ] Storage usage view with per-chat / file-type breakdown <!-- parity:settings-storage-usage -->
- [ ] Clear cache <!-- parity:settings-clear-cache -->
- [ ] App language selector (partial: `system_language_code:"en"` hardcoded in src/connect.rs:284) <!-- parity:settings-language -->
- [ ] Enter-to-send toggle (partial: Enter always sends, hardcoded in src/composer.rs:19-20; no toggle) <!-- parity:settings-enter-send -->
- [ ] Send by Cmd/Ctrl+Enter option <!-- parity:settings-ctrlenter-send -->

### Platform & edge cases

- [x] Global keyboard shortcuts: 22 bindings wired in `bind_keys` (Quit, focus sidebar/composer, chat search, media viewer nav/zoom) (src/ui/mod.rs:125) <!-- parity:platform-keyboard-shortcuts -->
- [ ] Keyboard shortcuts reference/help overlay listing all bindings <!-- parity:platform-shortcuts-reference -->
- [ ] Customizable key bindings <!-- parity:platform-custom-keybindings -->
- [ ] Screen-reader accessible labels/roles on UI elements (no accessibility API usage in src) <!-- parity:platform-screen-reader-labels -->
- [ ] VoiceOver support (blocked: Linux desktop has no VoiceOver; no accessibility tree backend in the UI layer) <!-- parity:platform-voiceover -->
- [ ] High-contrast theme/mode <!-- parity:platform-high-contrast -->
- [ ] System tray icon with unread count <!-- parity:platform-tray-icon -->
- [ ] Minimize/close-to-tray behavior <!-- parity:platform-minimize-to-tray -->
- [ ] Tray context menu (open window, quit) <!-- parity:platform-tray-menu -->
- [ ] Start minimized to tray <!-- parity:platform-start-minimized -->
- [ ] Autostart on login (OS-level; schema `autostart` is bot-start-only, no TDLib involvement) <!-- parity:platform-autostart -->
- [ ] Spellcheck in composer <!-- parity:platform-spellcheck -->
- [ ] Chat history export to file (partial: getChatHistory fetching exists; no export-to-file; implementable client-side — no exportHistory constructor in schema, not schema-blocked) <!-- parity:platform-history-export -->
- [ ] Full account data export (Telegram Desktop "Export Telegram data") <!-- parity:platform-data-export -->
- [ ] In-app update check/download/install <!-- parity:platform-app-updates -->
- [ ] Update changelog display after updates <!-- parity:platform-update-changelog -->
- [ ] Offline connection indicator in UI (partial: updateConnectionState parsed at telegram/envelope.rs:5506 and stored in state.rs:2199, but never rendered) <!-- parity:platform-offline-indicator -->
- [ ] Reconnect state labels ("Connecting…", "Waiting for network…", "Updating…", "Connecting to proxy…") <!-- parity:platform-reconnect-states -->
- [ ] "You're offline" error messaging when sending/calling while offline <!-- parity:platform-offline-errors -->
- [x] TDLib request errors surfaced on the originating surface (e.g. failed createCall → error line on call overlay) (src/state.rs:4547) <!-- parity:platform-error-surfacing -->
- [ ] Flood/rate-limit errors with retry countdown (e.g. "Try again in N seconds") <!-- parity:platform-flood-errors -->
- [ ] Unread badge on the app/taskbar icon <!-- parity:platform-app-icon-badge -->
- [ ] OS desktop notifications (partial: in-app toast queue with burst coalescing exists in src/notify.rs; no OS dispatch) <!-- parity:platform-os-notifications -->
- [ ] Drag-and-drop files into the composer <!-- parity:platform-drag-drop-files -->
- [x] Copy text to clipboard (inline keyboard copy-text button src/ui/mod.rs:3932; invite link src/ui/mod.rs:6357) <!-- parity:platform-copy-clipboard -->
- [ ] Paste image from clipboard into composer (partial: clipboard write exists, no read_from_clipboard usage) <!-- parity:platform-paste-image -->
- [ ] t.me/tg: deep-link handling via getDeepLinkInfo (schema support exists; no usage in Quill) <!-- parity:platform-deep-links -->

Decisions, pins, and blockers: [DECISIONS.md](DECISIONS.md).  
What credentials are needed next: [docs/credentials.md](docs/credentials.md).  
Phase 0 UI proof (real window, not a generated still): [docs/screenshots](docs/screenshots).
