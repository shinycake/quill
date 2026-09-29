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

A weekly `telegram-update-watch` scheduled job keeps this checklist current with official Telegram releases: new release features are verified against the pinned TDLib schema and added here as unchecked items with `parity:` anchors. Items blocked on missing TDLib APIs are marked `(blocked:)` with the reason.

### Auth & accounts

- [x] Phone-number login: country code, invalid/banned-number errors, SMS hint <!-- parity:auth-phone-login --> (README:24; telegram/requests.rs:51)
- [x] Verification-code entry: expected digit count, invalid-code handling <!-- parity:auth-code-entry --> (auth.rs:44; telegram/requests.rs:71)
- [x] Resend the login code: "Resend code" on the code screen sends resendAuthenticationCode (reason resendCodeReasonUserRequest); no invented local cooldown — a too-early resend fails server-side (429) and surfaces via the auth-error line <!-- parity:auth-code-resend -->
- [x] Two-step password entry on login <!-- parity:auth-2fa-password --> (auth.rs:52; telegram/requests.rs:81)
- [x] Log in via QR code: "Sign in with QR code" on the phone screen sends requestQrCodeAuthentication; the link from authorizationStateWaitOtherDeviceConfirmation renders as a real QR bitmap (never logged) <!-- parity:auth-qr-login -->
- [ ] "Link desktop device": show QR so another device can log in as this account <!-- parity:auth-qr-authorize-other -->
- [ ] New-user registration: first/last name + terms (partial: explicit UnsupportedHalt — "finish registration in an official client", auth.rs:83) <!-- parity:auth-registration -->
- [ ] Email-based login flow (partial: explicit UnsupportedHalt, auth.rs:72) <!-- parity:auth-email-login -->
- [ ] Premium-purchase-gated login state (partial: explicit UnsupportedHalt, auth.rs:66) <!-- parity:auth-premium-login -->
- [x] Enable / change / disable the two-step password: "Two-Step Verification" overlay (TGX wording) shows the authoritative getPasswordState; setPassword enable (empty old, optional recovery email in the same call), change, and disable (empty new); no optimistic mutations, one op in flight, passwords zeroized and never logged <!-- parity:auth-2fa-manage -->
- [x] Set / change recovery email, pending-confirmation state, abort setup: setRecoveryEmailAddress (current password required), pending pattern card (TGX PendingEmailText), resend (resendRecoveryEmailAddressCode, no invented cooldown) and "Abort recovery email setup" (TGX AbortRecoveryEmail verbatim) <!-- parity:auth-recovery-email -->
- [x] Password recovery via emailed code: "Forgot password?" on the 2FA screen sends requestAuthenticationPasswordRecovery (resend re-issues it — no invented cooldown, 429 surfaces), recovery-code entry sends recoverAuthenticationPassword (code zeroized, never stored — the A2 rule; new password empty, re-enable 2FA in Settings); recovery removes 2FA and TDLib continues auth <!-- parity:auth-password-recovery --> (A10: requests.rs; state.rs:RequestPurpose; connect.rs:request_password_recovery/submit_recovery_code; ui/mod.rs)
- [x] Active Sessions list: device/app/IP/location with current-device marker (schema: getActiveSessions) <!-- parity:auth-sessions-list --> (telegram/requests.rs:57; telegram/envelope.rs:8110; ui/mod.rs:24263)
- [x] Incomplete login attempts list with per-attempt terminate (TGX SessionsIncompleteTitle/Info) <!-- parity:auth-sessions-incomplete --> (ui/mod.rs:24263 — is_password_pending section)
- [x] Terminate one session, with confirmation (schema: terminateSession; TGX TerminateSessionQuestion) <!-- parity:auth-session-terminate-one --> (telegram/requests.rs:67; connect.rs: terminate_session; ui/mod.rs: confirm banner)
- [x] Terminate all other sessions, with confirmation (schema: terminateAllOtherSessions; TGX AreYouSureSessions) <!-- parity:auth-sessions-terminate-all --> (telegram/requests.rs:79; connect.rs: terminate_all_other_sessions; ui/mod.rs: confirm banner)
- [x] Per-session toggles: accept secret chats / accept calls (schema: toggleSessionCanAcceptSecretChats, toggleSessionCanAcceptCalls; TGX SessionAccepts) <!-- parity:auth-session-toggles -->
- [x] "Logged in with Telegram" websites list + disconnect all (TGX WebSessionsTitle, TerminateAllWebSessions) <!-- parity:auth-web-sessions -->
- [x] Log out (telegram/requests.rs:98; state.rs:6502 invalidates account) <!-- parity:auth-logout -->
- [ ] Logout warning text: secret chats die, downloaded media erased (TGX SignOutHint2) (partial: logout works, no warning copy) <!-- parity:auth-logout-warning -->
- [ ] Add another account / switch between accounts: single "accounts/primary" DB layout only, no account UI (settings.rs:39) <!-- parity:auth-multi-account -->
- [ ] Change phone number: move contacts/groups/messages/media to a new number (partial: A8 backend shipped — sendPhoneNumberCode/checkPhoneNumberCode/resendPhoneNumberCode with phoneNumberCodeTypeChange while authorized (NOT re-auth), + connect + reducer; UI blocked on server-acceptance verification: schema marks phoneNumberCodeTypeChange "for official Android and iOS applications only") <!-- parity:auth-change-number -->
- [x] Delete account with "Deleted Account" explainer: "🗑️ Account" sidebar entry → kit Dialog with the TGX-verbatim explainer (chats see you as Deleted Account), optional reason field, 2FA password field only when `passwordState.has_password`, inline "Alright, delete account." final confirm; password zeroized and never logged (A7: deleteAccount builder + connect + reducer; A9: UI) <!-- parity:auth-delete-account -->
- [x] Self-destruct-if-away timer: kit RadioGroup picker with the TGX-verified options (1/3/6/12/18/24 months → 31/91/181/366/546/730 days); current value rendered with TGX's display rule; confirmed days land from the authoritative `ok`, never optimistic (A7: getAccountTtl/setAccountTtl builders + connect + cached days; A9: UI) <!-- parity:auth-account-ttl -->
- [x] Contacts list with empty state (ui/mod.rs:7729 contacts_list; telegram/requests.rs:364 getContacts) <!-- parity:auth-contacts-list -->
- [x] Add contact via dialog: phone (required), first/last name -> addContact (ui/mod.rs:9469; telegram/requests.rs:379) <!-- parity:auth-contact-add -->
- [x] Delete contact (schema: removeContacts; TGX DeleteContactConfirm) <!-- parity:auth-contact-delete -->
- [x] Import contacts from a file/vCard (schema: importContacts) <!-- parity:auth-contact-import -->
- [x] Sync contacts toggle + delete synced contacts from servers (TGX SyncContacts*, SyncContactsDeleteInfo) <!-- parity:auth-contact-sync -->
- [x] Block user with confirmation (TGX QBlockUser/BlockUserConfirm; no block request builders in Quill) <!-- parity:auth-block-user -->
- [x] Edit name: "Edit profile" dialog on the own info panel sends setName (telegram/requests.rs:4213; connect.rs:8910; ui/mod.rs:18963) <!-- parity:auth-edit-name -->
- [x] Edit bio: "Edit profile" dialog sends setBio; the info panel now also renders the bio from getUserFullInfo (telegram/requests.rs:4224; connect.rs:8924; ui/mod.rs:18963) <!-- parity:auth-edit-bio -->
- [x] Username management: Check availability via checkChatUsername (private chat with self; all six checkChatUsernameResult verdicts parsed, envelope.rs:2386), set/clear editable username (empty clears per schema :14830), up/down reorder of active usernames, activate/deactivate (editable username can't be deactivated per schema :14835); schema: setUsername, reorderActiveUsernames, toggleUsernameIsActive (telegram/requests.rs:4235; connect.rs:8935; ui/mod.rs:18963) <!-- parity:auth-username -->
- [x] Set / remove profile photo: setProfilePhoto via inputChatPhotoStatic + inputFileLocal with is_public hard-coded false for the main (non-public) photo (schema :14803; TGX AvatarPickerManager), remove via deleteProfilePhoto with the retained chatPhoto.id (state.rs:3771) (telegram/requests.rs:4283; connect.rs:9006; ui/mod.rs:18963) <!-- parity:auth-profile-photo -->
- [ ] Profile accent color (schema: setProfileAccentColor) <!-- parity:auth-profile-accent -->

### Messaging core

- [x] Send plain text message (`sendMessage`, Enter to send) <!-- parity:msg-send-text --> (telegram/requests.rs:1574; send-on-Enter gate composer.rs:20)
- [x] Bold formatting authoring (entities apply to message text and captions) <!-- parity:msg-bold --> (M1: toolbar B / Ctrl-B inserts **bold**; send converts to textEntityTypeBold requests.rs)
- [x] Italic formatting authoring (entities apply to message text and captions) <!-- parity:msg-italic --> (M1: toolbar I / Ctrl-I inserts *italic*; send converts to textEntityTypeItalic)
- [x] Underline formatting authoring (entities apply to message text and captions) <!-- parity:msg-underline --> (M1: toolbar U / Ctrl-U inserts __underline__; send converts to textEntityTypeUnderline)
- [x] Strikethrough formatting authoring (entities apply to message text and captions) <!-- parity:msg-strikethrough --> (M1: toolbar S inserts ~~strike~~; send converts to textEntityTypeStrikethrough)
- [x] Inline code / monospace authoring (entities apply to message text and captions) <!-- parity:msg-inline-code --> (M1: toolbar </> inserts `code`; send converts to textEntityTypeCode)
- [x] Code block / pre (with language) authoring (entities apply to message text and captions) <!-- parity:msg-code-block --> (M1: toolbar pre inserts fenced block, optional language; send converts to textEntityTypePre/PreCode)
- [x] Spoiler authoring (entities apply to message text and captions) <!-- parity:msg-spoiler --> (M1: toolbar spoiler inserts ||spoiler||; send converts to textEntityTypeSpoiler)
- [x] Quote/blockquote formatting authoring (entities apply to message text and captions) <!-- parity:msg-quote-block --> (M1: toolbar quote prefixes > lines; send converts to textEntityTypeBlockQuote)
- [x] Create text link (textUrl) authoring (entities apply to message text and captions) <!-- parity:msg-text-link --> (M1: toolbar link inserts [label](url); send converts to textEntityTypeTextUrl)
- [x] Clear formatting on selection (applies to message text and captions) <!-- parity:msg-clear-formatting --> (M1: toolbar clear strips markup on selection or whole text, composer.rs)
- [x] Reply to a message <!-- parity:msg-reply --> (begin_reply_to ui/mod.rs:4269)
- [x] Quote selected text in a reply — "Quote reply" menu item → quote picker → validated UTF-16 offset → `inputTextQuote` on every send path (text, media, albums, voice notes, GIFs/stickers, polls); ❝quote❞ shown in the reply banner (src/composer.rs, src/telegram/requests.rs, src/ui/mod.rs) <!-- parity:msg-reply-quote -->
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
- [x] Unpin all messages (unpinAllChatMessages unwired in UI) <!-- parity:msg-unpin-all --> (M1: "Unpin all" button in pinned banner when >1 *loaded* row is pinned; the count reflects loaded history rows, while unpinAllChatMessages acts server-side on all pins)
- [x] Copy message text (only inline-button CopyText ui/mod.rs:3932 and invite-link copy exist) <!-- parity:msg-copy-text --> (M1: right-click context menu Copy on text/caption messages)
- [x] Share message / copy t.me link (getMessageLink unused) <!-- parity:msg-share-link --> (M1: right-click Share link -> getMessageLink -> clipboard)
- [x] Scheduled send (send at date; options:null, schema messageSchedulingStateSendAtDate td_api.tl:5902) <!-- parity:msg-scheduled-send --> (M1: schedule popup +1h/+8h/+24h via messageSchedulingStateSendAtDate)
- [x] Scheduled messages list / edit / delete <!-- parity:msg-scheduled-list --> (M1: getChatScheduledMessages dialog with list, edit (editMessageText), delete)
- [x] Send when online (schema messageSchedulingStateSendWhenOnline td_api.tl:5905; private chats only) <!-- parity:msg-send-when-online --> (M1: schedule popup "when online" via messageSchedulingStateSendWhenOnline, offered only in private chats)
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
- [x] Rich text editor: expand icon after 3+ lines, produces inputRichMessage <!-- parity:msg-richtext-editor -->
- [x] Inline documents/files/music inside text blocks (pageBlockDocument / inputPageBlockDocument) <!-- parity:msg-richtext-inline-doc -->
- [x] Send rich messages (inputMessageRichMessage) <!-- parity:msg-richmessage-send -->
- [x] Render richMessage PageBlocks in message bubbles (messageRichMessage, getFullRichMessage) <!-- parity:msg-richmessage-render -->
- [x] In-message buttons: render pageBlockButtonRow + richTextButton, taps fire bot callbacks <!-- parity:msg-richmessage-buttons -->
- [x] Ephemeral messages: render message.ephemeral_content instead of regular content <!-- parity:msg-ephemeral-render -->
- [ ] Apply updateMessageEphemeralContent (ephemeral content refreshes over time; initial render covered by parity:msg-ephemeral-render) <!-- parity:msg-ephemeral-updates -->
- [ ] Compact tables in rich messages <!-- parity:msg-richtext-tables -->
- [ ] Expandable block quotes (long block quotes collapse with an expand affordance; authoring covered by parity:msg-quote-block) <!-- parity:msg-blockquote-expandable -->
- [ ] Inline photos/videos in the rich-text composer (partial: inline documents/files/music done — parity:msg-richtext-inline-doc) <!-- parity:msg-richtext-inline-media -->
- [ ] AI tools in the rich-text composer (composeTextWithAi, composeRichMessageWithAi, createRichMessageWithAi, fixTextWithAi, fixRichMessageWithAi) <!-- parity:msg-richtext-ai-tools -->
- [ ] Rich-text composer max length (32,768 chars) <!-- parity:msg-richtext-max-length -->
- [ ] Premium gating of the rich-text editor <!-- parity:msg-richtext-premium-gate -->

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
- [x] Right-click context menu on chat rows (src/ui/mod.rs:5580) <!-- parity:chatlist-row-context-menu -->
- [x] Pin / Unpin chat via toggleChatIsPinned with optimistic rollback (src/connect.rs) <!-- parity:chatlist-pin-unpin -->
- [x] Drag-to-reorder pinned chats (main + archive lists): pin-drag ghost, drop moves to slot, full pinned-id list via setPinnedChats with optimistic reorder + rollback on refusal (src/connect.rs: set_pinned_chat_order, src/state.rs: reorder_pinned_chats, src/ui/mod.rs: PinnedChatDrag) <!-- parity:chatlist-pin-drag-reorder -->
- [x] Pin-limit handling: client-side pre-check from pinned_chat_count_max / pinned_archived_chat_count_max options plus server error surfacing (src/connect.rs, src/state.rs) <!-- parity:chatlist-pin-limit -->
- [x] Mark all chats as read: readChatList for main + archive lists, "✓ Mark all read" on the main list and the archive header, no-op when nothing is unread (src/connect.rs: mark_all_chats_as_read, src/ui/mod.rs) <!-- parity:chatlist-mark-all-read -->
- [x] Per-chat mark as read / unread: viewMessages with messageSourceChatList + force_read (TGX semantics) and toggleChatIsMarkedAsUnread; marked-unread dot badge (src/connect.rs, src/ui/mod.rs) <!-- parity:chatlist-mark-read-unread -->
- [x] Per-chat mute/unmute from the list via the row menu (src/ui/mod.rs:5611) <!-- parity:chatlist-list-mute -->
- [x] Delete chat from the list via deleteChatHistory(remove_from_chat_list:true) — not the destructive deleteChat (src/connect.rs, src/ui/mod.rs) <!-- parity:chatlist-delete-chat -->
- [x] Clear chat history via deleteChatHistory(remove_from_chat_list:false), gated on delete capabilities (src/connect.rs, src/ui/mod.rs) <!-- parity:chatlist-clear-history -->
- [x] Saved Messages entry row in the list: opens the existing self chat directly, otherwise createPrivateChat with getOption("my_id") and opens the returned chat (schema td_api.tl:9590; src/connect.rs: create_private_chat_with_self, src/ui/mod.rs: open_saved_messages) <!-- parity:chatlist-saved-messages -->
- [x] Chat preview on long-press (press-and-hold; hover was deliberately rejected — see DECISIONS.md) <!-- parity:chatlist-chat-preview -->
- [x] Clear recent searches: Clear button on the Recent heading, clearRecentlyFoundChats with optimistic local clear (TGX SearchManager parity), refusal surfaced as a status note (src/connect.rs: clear_recently_found_chats, src/ui/mod.rs: clear_search_recents) <!-- parity:chatlist-clear-recent-searches -->
- [x] No-results state in search: "No chats or messages match “…”" (src/ui/mod.rs: search_results; demo quill --screenshot-demo ready-chat-list-search) <!-- parity:chatlist-search-no-results -->
- [x] Collapsible archive section: clickable ▸/▾ header with count, per-session collapsed state (TGX archiveCollapsed) (src/state.rs: archive_collapsed, src/ui/mod.rs: toggle_archive_collapsed) <!-- parity:chatlist-archive-collapse -->
- [x] Archive auto-settings: get/setArchiveChatListSettings with a dialog for the three schema-backed toggles (archive+mute unknown users, keep unmuted archived, keep folder chats archived), fetch-on-open, optimistic toggle with rollback, TGX SettingsArchiveChatListController labels (src/connect.rs, src/state.rs, src/ui/mod.rs: archive_settings_overlay) <!-- parity:chatlist-archive-auto-settings -->
- [x] Mention/reaction badges on rows: @ badge (unread_mention_count, schema 1.8.67:3611) and ♥ badge (unread_reaction_count, :3612; dimmed when muted), right-to-left like TGX ChatView; the main counter hides when mentions exist and unread_count == 1 (TGX TGChat.setCounter) (src/state.rs, src/ui/mod.rs: mention_badge/reaction_badge; demo quill --screenshot-demo ready-chat-list-3) <!-- parity:chatlist-mention-badge -->
- [x] Multi-select mode: row-menu Select… enters with the chat checked, row clicks toggle (last uncheck exits), select bar with count + Pin / Read / Mute / Archive / Select unread / Delete (bulk delete confirms first); all bulk actions reuse the existing per-chat drivers (src/ui/mod.rs: selected_chats, src/connect.rs) <!-- parity:chatlist-multi-select -->
- [x] Report / Block contact from the list: row-menu Report gated on chat.can_be_reported sends the simple spam reportChat (schema 1.8.67:15693, :3667); Block/Unblock user for private/secret peers via setMessageSenderBlockList (schema 1.8.67:14492, null unblocks); refusals and non-OK report results surface honestly, never as success (src/connect.rs: report_chat/set_chat_user_blocked, src/state.rs, src/ui/mod.rs) <!-- parity:chatlist-report-block -->
- [ ] App badge counter settings: include muted chats, include archived chats, count messages vs chats <!-- parity:chatlist-badge-settings -->
- [ ] Chat list style settings: two/three lines, media icons, text formatting <!-- parity:chatlist-list-style -->
- [x] Unread / Archived filter category chips beside the folder tabs (the folder/Main selection itself is the All view); Archived forces the archive section open (src/ui/mod.rs: ChatListFilter) <!-- parity:chatlist-category-filters -->

### Media

- [x] Fullscreen photo/video viewer overlay with prev/next navigation (arrow keys) and "N of M" counter (src/media_viewer.rs:74-124) <!-- parity:media-viewer -->
- [x] Zoom viewer image via `=`/`-` keys and scroll, drag-pan when zoomed, `0` resets to fit (src/media_viewer.rs:172-250, ui/mod.rs:146-150) <!-- parity:media-viewer-zoom -->
- [x] Rotate photo in the viewer — Rotate button cycles 0/90/180/270, per-item, cached (media_viewer.rs `rotate_rgba_quarter_turns`) <!-- parity:media-viewer-rotate -->
- [ ] Picture-in-picture for video playback — BLOCKED: GPUI 0.3.5 `WindowOptions` has no always-on-top field on Linux and the repo has no multi-window plumbing; a second window would not be an honest PiP <!-- parity:media-video-pip -->
- [x] Viewer auto-downloads the current item when not local and resumes a parked play once `downloadFile` lands (ui/mod.rs:4926, 5179) <!-- parity:media-viewer-autodownload -->
- [x] Share photo/video from the viewer — closes the viewer and opens the existing forward picker with the message selected <!-- parity:media-viewer-share -->
- [x] Save viewer media to downloads folder — largest local photo size / full video clip (honest "download the media first" when not local; video never saves its thumbnail), `(n)` de-dup <!-- parity:media-viewer-save -->
- [x] "Show in chat" jump from the viewer to the source message — closes the viewer, jumps via the existing reply-jump flow <!-- parity:media-viewer-show-in-chat -->
- [x] Play/pause video in the viewer with elapsed/total overlay (PlaybackClock, src/playback.rs:18; ui/mod.rs:4951) <!-- parity:media-video-play -->
- [x] Seek scrubber in the fullscreen video player — drag scrubs (position preview in the elapsed label), release seeks the clock and restarts ffplay at `-ss` when playing <!-- parity:media-video-seek -->
- [x] Playback speed control for voice/audio/video — speed button cycles the TGX set 0.5x–2.0x (single-tap cycle, not TGX's long-press dial); clock rate + ffplay `-af atempo=` stay in sync <!-- parity:media-playback-speed -->
- [x] Volume control / mute toggle in the video player — volume slider applies on release (ffplay `-volume`), mute remembers and restores the previous level; same mute on voice/audio rows <!-- parity:media-video-volume -->
- [x] Playback error states for unsupported video/audio/GIF/round-video formats — unsupported-format vs generic playback errors surfaced as red error lines in the viewer transport (TGX *PlaybackError/*PlaybackUnsupported); voice/audio rows surface ffplay-spawn failures, not silent stalls <!-- parity:media-playback-errors -->
- [x] Record voice note from the mic (ffmpeg OGG capture) with `chatActionRecordingVoiceNote` shown while recording (src/voice.rs:86-99, connect.rs:4796) <!-- parity:media-voice-record -->
- [x] Lock-to-record and discard confirmation while recording — Lock button (TGX `RecordLockView`, desktop-mapped); locked recordings ignore Esc; Cancel/Esc on an unlocked recording opens a "Discard this recording?" confirm row instead of discarding silently (ui/mod.rs) <!-- parity:media-voice-lock -->
- [x] Waveform bars on voice messages decoded from TDLib 5-bit waveform (src/voice.rs:29-67) <!-- parity:media-voice-waveform -->
- [x] Voice/audio history rows with play/pause, draggable seek bar, and remembered position (ui/mod.rs:1063, 6966) <!-- parity:media-audio-player -->
- [x] Voice note transcription display — `speech_recognition_result` parsed (pending/text/error) for voice and video notes; Transcribe button sends a real `recognizeSpeech` request, transcript arrives via `updateMessageContent` (src/telegram/envelope.rs, requests.rs, ui/mod.rs) <!-- parity:media-voice-transcription -->
- [x] Audio/video recording mode toggle — right-click the record button flips mode (TGX tap-to-switch `HoldToAudio`/`HoldToVideo`, desktop-mapped), persisted in `MediaPrefs` (src/settings.rs, ui/mod.rs) <!-- parity:media-record-mode-toggle -->
- [x] "Record HQ round videos" quality setting (TGX `UseHqRoundVideos`) in Media settings — 480px captures when on, 280px otherwise (src/settings.rs, src/video.rs) <!-- parity:media-video-note-hq -->
- [x] Discard-recording confirmation dialog — Cancel/Esc opens a confirm row ("Discard this recording?" / Keep recording); Esc with the row open dismisses it and keeps recording (ui/mod.rs) <!-- parity:media-record-discard-confirm -->
- [x] Round video-note player with play/pause in history (ui/mod.rs:842) <!-- parity:media-video-note-player -->
- [x] Send video notes from video files with probed duration and generated square thumbnail (requests.rs:1885, src/video.rs:107-156) <!-- parity:media-video-note-send -->
- [x] Record video note from the camera — ffmpeg V4L2 capture, center-crop square + scale to the HQ size, validated duration/square MP4 sent via `inputMessageVideoNote`; `chatActionRecordingVideoNote` while recording (src/video.rs, connect.rs). Live camera path untested — no `/dev/video*` on this VM; synthetic transcode and the no-camera error path are covered by tests <!-- parity:media-video-note-record -->
- [x] Music rows with title/performer/album-cover art and play/pause (ui/mod.rs:844) <!-- parity:media-music-row -->
- [x] GIF/animation frame playback in history (ui/mod.rs:688-690, 5838) <!-- parity:media-gif-playback -->
- [x] Document rows with file name and mime type, click-to-download (envelope.rs:4025, ui/mod.rs:16933) <!-- parity:media-document-row -->
- [x] Open downloaded document with the system app / reveal in file manager (MED3: open via existing OS-open path; "Show in folder" = `open -R` / `explorer /select,` / parent `xdg-open`) <!-- parity:media-document-open -->
- [x] On-demand `downloadFile` with priority and auto-download of thumbnails in the open chat (requests.rs:1491, connect.rs:2820) <!-- parity:media-download -->
- [x] Download progress display on history rows (MED3: real percent from `localFile.downloaded_size` + determinate progress bar; media viewer shows percent too) <!-- parity:media-download-progress -->
- [x] Cancel an in-flight download (MED3: `cancelDownloadFile(only_if_pending:false)`; works from history chip and manager) <!-- parity:media-download-cancel -->
- [ ] Pause / resume a download (schema: pause exists only for the persistent file-download list — `toggleDownloadIsPaused`; MED3 uses one-shot `downloadFile`, so pause is out of slice, see DECISIONS.md) <!-- parity:media-downloads-pause -->
- [x] Retry a failed download (MED3: failed state from `downloadFile` errors + stalled active→idle transitions; "Retry" re-issues the request) <!-- parity:media-download-retry -->
- [x] Downloads manager screen listing active/completed downloads (MED3: right-side panel — active with progress/cancel, recent completed with open/reveal, failed with retry) <!-- parity:media-downloads-manager -->
- [x] Automatic media download settings incl. data-saver pause-all mode (MED3: TGX-compatible per-chat-kind × media-type bitmask + data saver; `media_prefs.json`; desktop collapses TGX's mobile/wifi/roaming grids into one; enabled types auto-download full media in the open chat, secret/spoiler excluded) <!-- parity:media-auto-download-settings -->
- [x] Link preview cards render on messages with site, title, description, and thumbnail (ui/mod.rs:22426) <!-- parity:media-link-preview -->
- [x] Send-time link preview controls: disable, force small/large media, show above text (chip: Preview on/off + Media small/large + Above/Below text; `getLinkPreview` prefetch with 500ms debounce, loading/404 states) <!-- parity:media-link-preview-send-options -->
- [x] Instant View reader with tap-gating setting (Off / Telegram / All links): card tap attempts `getWebPageInstantView` when `instant_view_version > 0` and the mode allows the URL, page blocks render in a reader overlay, TDLib error falls back to the browser (TGX behavior) <!-- parity:media-instant-view -->
- [x] Embedded media player previews inside link previews (video/audio/animation): play/duration badge on the card, tap opens the embed URL (inline playback out of slice) <!-- parity:media-link-preview-embedded -->
- [x] Album-type link previews with multiple photo/video thumbnails (up to 4 in a strip) <!-- parity:media-link-preview-album -->
- [x] Send photo/video albums of 2–10 items via `sendMessageAlbum` with composer text as caption (requests.rs:1943, connect.rs:4605) <!-- parity:media-album-send -->
- [x] Received albums grouped by `media_album_id` into grid tiles (ui/mod.rs:17592, 21223) <!-- parity:media-album-grid -->
- [x] Open an album item in the fullscreen viewer — album tiles open the shared viewer (the old note claiming they didn't was stale); viewer header shows Pin/Unpin album when the item is in an album <!-- parity:media-album-viewer -->
- [x] Pin / unpin an album — one `pinChatMessage` per member when none are pinned, unpins pinned members when any are (TGX MessagePinAlbum semantics; no album-level constructor in TDLib 1.8.67), rights-gated on `ChatSummary::can_pin_messages` <!-- parity:media-album-pin -->
- [x] "Remember media grouping" setting — composer Grouped/Ungrouped toggle for 2+ photos/videos + remember on/off; persisted in `media_prefs.json` (settings::MediaPrefs), ungrouped sends go as separate messages <!-- parity:media-album-grouping-setting -->
- [x] Per-chat shared media gallery with Media / Files / Music / Links / Voice / GIFs tabs — gallery side panel ("Media" header button), each tab fetches one `searchChatMessages` page with its `searchMessagesFilter*` filter (schema 1.8.67:11864), tabs cache their first page; limits: no pagination yet, rows are glyph+label (no thumbnails) <!-- parity:media-shared-gallery -->
- [x] Empty states per shared-media tab — one shared renderer (glyph + TGX `No*ToShow` title + `No*ToShowInChat`/`InChannel` description, strings.xml:1601-1627), distinct Loading / Empty / Failed states per tab <!-- parity:media-shared-gallery-empty -->
- [x] Captions render on photos, videos, animations, audio, and documents <!-- parity:media-caption-render -->
- [x] Edit a sent media caption via `editMessageCaption` (requests.rs:2123, connect.rs:4941) <!-- parity:media-caption-edit -->
- [x] Caption position toggle: show above vs below media (composer toggle for photo/video, preserved on caption edits, rendered per `show_caption_above_media`) <!-- parity:media-caption-position -->
- [x] "Add a caption…" affordance when attaching media (caption bar above the composer with hint, above/below toggle, live n / max counter) <!-- parity:media-caption-prompt -->
- [x] Remove captions when forwarding copies (TGX RemoveCaptions; checkbox gated on send-copy) <!-- parity:media-caption-remove-on-forward -->
- [x] Caption-too-long validation on sends and caption edits (runtime `message_caption_length_max`, live counter, refusal names the limit) <!-- parity:media-caption-length-limit -->

### Groups, supergroups & channels

- [x] Group/channel info panel with description and member/subscriber count (src/ui/mod.rs:8280) <!-- parity:groups-info-panel -->
- [x] Create new group via `createNewBasicGroupChat` (title + member picker, sidebar "New group") (src/ui/mod.rs) <!-- parity:groups-create-group -->
- [x] Create new supergroup/channel via `createNewSupergroupChat` (title + description only — no forum toggle, no member picker; `is_forum` is sent false and the schema takes no member IDs, so members are added after creation from the member dialog; sidebar "New supergroup" / "New channel") (src/ui/mod.rs) <!-- parity:groups-create-channel -->
- [x] Convert supergroup to broadcast group — one-way (`toggleSupergroupIsBroadcastGroup`, no reverse in schema :15221 or Telegram X; owner-only, destructive confirm) (src/ui/mod.rs) <!-- parity:groups-convert-broadcast -->
- [x] Add members via contact picker — `addChatMember` (basic groups) / `addChatMembers` (supergroups/channels), privacy failures surfaced from `failedToAddMembers` (src/connect.rs, src/ui/mod.rs) <!-- parity:groups-add-members -->
- [x] Browse/search real member lists — `getBasicGroupFullInfo` (basic groups) and `getSupergroupMembers` with All/Admins/Restricted/Banned tabs + server-side search; first 200 members only (offset 0 / limit 200, no load-more) (src/ui/mod.rs) <!-- parity:groups-member-list -->
- [x] Restricted-members and banned-members lists via `getSupergroupMembers` status filters (:2571/:2574), first 200 only, with edit/unrestrict/unban actions (src/ui/mod.rs) <!-- parity:groups-restricted-banned-lists -->
- [x] Administrator list with refresh, owner shown, custom titles displayed (src/ui/mod.rs:8748) <!-- parity:groups-admin-list -->
- [x] Promote member via searchable picker with 18 granular rights checkboxes (src/ui/mod.rs:266-415) <!-- parity:groups-promote -->
- [x] Edit existing admin rights, pre-filled from `getChatMember` (src/state.rs:209, src/ui/mod.rs:363) <!-- parity:groups-edit-rights -->
- [x] Demote admin with confirmation dialog (src/ui/mod.rs:363) <!-- parity:groups-demote -->
- [x] Set/edit admin custom title via `setChatMemberTag` (schema :13598 — the setter Telegram X uses; 0-16 chars, no emoji, basic groups + supergroups only; titles shown next to admin rows) (src/ui/mod.rs) <!-- parity:groups-admin-title -->
- [x] Ban/restrict member with duration (forever/1d/7d/30d) and unban via `setChatMemberStatus` (src/ui/mod.rs) <!-- parity:groups-restrict-ban -->
- [x] Chat permissions editor — all 16 `chatPermissions` toggles via `setChatPermissions`, admin-gated (src/ui/mod.rs) <!-- parity:groups-chat-permissions -->
- [x] Slow-mode delay picker (Off/5s/10s/30s/1m/5m/15m/1h, admin-gated) (src/ui/mod.rs:8309) <!-- parity:groups-slow-mode -->
- [x] Slow-mode send gate with countdown, applies to sends/forwards/voice (src/ui/mod.rs:3099) <!-- parity:groups-slow-mode-enforcement -->
- [x] Slow-mode bypass when viewer boosts meet the unrestrict threshold (src/state.rs:3034) <!-- parity:groups-slow-mode-boost-bypass -->
- [x] Invite-link list with expiry/humanised "Never expires"/"Expired" labels and pending-join-request counts (src/ui/mod.rs:8397) <!-- parity:groups-invite-link-list -->
- [x] Create invite link dialog — name, expiration, member limit, join-request toggle (src/ui/mod.rs:222) <!-- parity:groups-invite-link-create -->
- [x] Edit, copy and revoke invite links (src/ui/mod.rs:6341) <!-- parity:groups-invite-link-edit-revoke -->
- [x] Primary invite link displayed with replace via `replacePrimaryChatInviteLink` (src/ui/mod.rs) <!-- parity:groups-invite-link-primary -->
- [x] Join-request list with approve/decline buttons and pending-count badge (src/ui/mod.rs:8596) <!-- parity:groups-join-requests -->
- [x] "Approve new members" join-by-request toggle (`toggleSupergroupJoinByRequest`) (src/ui/mod.rs) <!-- parity:groups-join-by-request-toggle -->
- [x] Recent-actions event log with refresh and load-more (src/ui/mod.rs:8914) <!-- parity:groups-event-log -->
- [x] Event-log filter picker (per-event-type chips sent in `getChatEventLog`, per-admin chips filtering the loaded page client-side) and in-log text search (src/ui/mod.rs) <!-- parity:groups-event-log-filters -->
- [x] Channel/group statistics panel with graphs and top senders/administrators/inviters, gated on `can_get_statistics` (src/ui/mod.rs:9195) <!-- parity:groups-statistics -->
- [x] Author signatures rendered on channel posts (src/ui/mod.rs:22014) <!-- parity:groups-author-signatures-display -->
- [x] Author-signatures toggle for channel (`toggleSupergroupSignMessages`, plus show-authors flag forced to `sign && show` per Telegram X, gated on `can_change_info`) (src/ui/mod.rs) <!-- parity:groups-author-signatures-toggle -->
- [x] Forum topic list with per-topic history and posting to topics (src/connect.rs:1443) <!-- parity:groups-forum-browse -->
- [x] Create/rename/close/reopen/pin/unpin/delete forum topics and hide/show the General topic (`createForumTopic` family requests exist in src/telegram/requests.rs; forum dialog in src/ui/mod.rs, gated on `can_manage_topics`; custom topic icons out of scope) <!-- parity:groups-forum-manage -->
- [x] "Discuss" jump to the linked discussion group (src/ui/mod.rs:12638) <!-- parity:groups-discussion-jump -->
- [x] Channel comments viewer ("View comments" message-menu action opens the thread dialog via `getMessageThreadHistory`, src/ui/mod.rs) <!-- parity:groups-channel-comments -->
- [x] Aggressive anti-spam toggle (`toggleSupergroupHasAggressiveAntiSpamEnabled`, supergroups only, gated on `supergroupFullInfo.can_toggle_aggressive_anti_spam`) (src/ui/mod.rs) <!-- parity:groups-anti-spam -->
- [x] Boost status/level display (`getChatBoostStatus`) and boost action (`boostChat` → `chatBoostSlots` response handling, status cache invalidated and refetched after the confirmed boost) in the channel info panel (src/ui/mod.rs, src/state.rs, src/connect.rs) <!-- parity:groups-boost -->
- [x] Public username management (`setSupergroupUsername`, owner-only, empty clears) (src/ui/mod.rs) <!-- parity:groups-public-username -->
- [x] Group/channel title editing (`setChatTitle`, 1–128 chars, `can_change_info`): drivers (`set_group_title` + `group_info_edit_allowed` gate — basic groups democratic, supergroups/channels need `can_change_info`) plus the edit UI — "Edit title" row in the group/channel info panel (same gate) opens a prefilled text prompt, 1–128 chars enforced client-side, refusals surface honestly; the new title arrives via `updateChatTitle` (src/ui/mod.rs, src/ui/dialogs/username.rs; demo `quill --screenshot-demo ready-group-info-edit`) <!-- parity:groups-set-title -->
- [x] Group/channel description editing (`setChatDescription`, 0–255 chars, empty clears, `can_change_info`): same driver/UI pattern as the title — "Edit description" row, prefilled from the supergroup full info (basic groups keep no description in state, field starts empty); empty clears; the new description arrives on the next full-info pull, TDLib has no `updateChatDescription` broadcast (src/ui/mod.rs, src/ui/dialogs/username.rs) <!-- parity:groups-set-description -->
- [x] Group/channel photo editing (`setChatPhoto`, null deletes, `can_change_info`): same driver/UI pattern — "Change photo" row opens a path prompt (no native file picker in the app), empty removes the photo, the path must be a real file; the new photo arrives via `updateChatPhoto` (src/ui/mod.rs, src/ui/dialogs/username.rs) <!-- parity:groups-set-photo -->
- [x] Leave channel / leave group (src/ui/mod.rs) <!-- parity:groups-leave -->
- [x] Delete group/channel for everyone (`deleteChat`, gated by `can_be_deleted_for_all_users`) with confirmation (src/ui/mod.rs) <!-- parity:groups-delete -->
- [x] Welcome messages: joiner-side rendering — welcome content reaches a new joiner as regular `updateNewMessage` messages (server pushes `updateNewEphemeralMessage` with `welcome_template=false`; TDLib converts it to a normal message) and renders through the existing message pipeline; `updateChatWelcomeMessages` is pack sync for admins only (requires `can_send_welcome_messages`; TGX leaves it unhandled) and is never delivered to plain joiners — mechanism verified against TDLib 1.8.67 source, see DECISIONS.md <!-- parity:groups-welcome-view -->
- [x] Welcome messages: add/edit/delete via addChatWelcomeMessage, editChatWelcomeMessage, deleteChatWelcomeMessage, loadChatWelcomeMessages (gated on `can_send_welcome_messages`; pack refetched after each confirmed mutation) (src/ui/mod.rs, src/connect.rs) <!-- parity:groups-welcome-manage -->
- [x] Welcome message setup: Welcome-message row in the group/channel info panel opens the pack editor dialog (src/ui/mod.rs) <!-- parity:groups-welcome-setup -->
- [ ] Communities: create a community (createCommunity exists in TDLib 1.8.67; Quill backend landed — builder + driver + state sync; no Quill UI) <!-- parity:communities-create -->
- [ ] Communities: browse and manage owned communities (partial: Quill backend landed — builders + drivers + state sync for create/loadFullInfo/setName; no Quill UI) <!-- parity:communities-hub -->
- [ ] Communities: toggle community chat visibility (blocked: no TDLib 1.8.67 method to toggle hidden state) <!-- parity:communities-chat-visibility -->
- [ ] Communities: community chat-list mode (view a community's chats as a filtered chat list) <!-- parity:communities-chatlist-mode -->
- [ ] Communities: add a chat to a community (blocked: no TDLib 1.8.67 method) <!-- parity:communities-add-chat -->
- [ ] Communities: admin-rights management (blocked: no TDLib 1.8.67 method) <!-- parity:communities-admin-rights -->
- [ ] Communities: info panel (partial: Quill backend landed — loadFullInfo/setName builders + drivers + state sync; no Quill UI) <!-- parity:communities-info -->
- [x] Communities: "chat added to community" service message (`messageChatAddedToCommunity`; TGX `ActionChatAddedToCommunity`/`ActionChatAddedToCommunityUnknown` verbatim — `This chat was added to community "NAME"` with the name from the session `updateCommunity` cache, nameless fallback when unknown) (src/telegram/envelope.rs, src/ui/mod.rs) <!-- parity:groups-added-to-community -->
- [x] Communities: "chat removed from community" service message (`messageChatRemovedFromCommunity`; TGX `ActionChatRemovedFromCommunity` verbatim — `This chat was removed from community`) (src/telegram/envelope.rs, src/ui/mod.rs) <!-- parity:groups-removed-from-community -->
- [ ] Communities: community search filter (searchMessagesChatTypeFilterCommunity) <!-- parity:communities-search-filter -->
- [x] Communities: community join service message (`messageChatJoinFromCommunity`; TGX `group_user_join_from_community*` verbatim — `{name} joined the group from the community "NAME"` / `You joined the group from the community "NAME"`, nameless fallbacks; sender kept uncollapsed so incoming rows attribute the join) (src/telegram/envelope.rs, src/ui/mod.rs) <!-- parity:communities-join-service-message -->

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
- [x] "Toggle whether a session can accept incoming secret chats" privacy toggle (verified: sessions dialog renders a per-session kit "Secret Chats" Switch (ui/mod.rs:30680) checked from `can_accept_secret_chats` → `toggle_session_secret_chats` (30480) → driver `toggle_session_can_accept_secret_chats` (connect.rs:11202) → `toggleSessionCanAcceptSecretChats` builder (requests.rs:2081); TGX's EditSessionController "Session Accepts → Secret Chats" toggle) <!-- parity:secret-session-accept -->
- [x] Secret-chat notification settings: "Custom notification settings for the Secret Chat with {name}." label (TGX NotificationChannelSecretChat) in the existing per-chat notification panel, which fully applies to secret chats; lock-screen hide behavior (TGX HideSecret/ShowSecretOn) is out of slice — Linux desktop exposes no lock-screen notification API (src/ui/mod.rs) <!-- parity:secret-notif-settings -->
- [x] Per-secret-chat passcode hint in the chat info panel (TGX SecretPasscodeInfo verbatim with the peer's name); actually setting an additional per-chat passcode needs a full app passcode feature — out of slice (src/ui/mod.rs) <!-- parity:secret-passcode -->
- [x] "Secret media and files" category in storage usage (new "💾 Storage usage" sidebar entry backed by getStorageStatistics; overlay lists TGX-ordered categories with counts and sizes, including the Secret media and files row for fileTypeSecret) <!-- parity:secret-storage-category -->
- [ ] Secret-chat count shown when terminating a session that would cancel secret chats (impossible via TDLib 1.8.67: the `session` type (td_api.tl:9144) has no secret-chat linkage — `secretChat` (:2816) carries no session id, and the only secret-related session field is the capability flag `can_accept_secret_chats`; which auth-key a secret chat is bound to is server-side state. Telegram X shows no such count on session termination either — its `ClosingXSecretChats` string belongs to bulk chat deletion (ChatsController.java:1650), not sessions; Quill's terminate confirmations match TGX verbatim) <!-- parity:secret-session-terminate -->

### Stories

#### View stories
- [x] Active-story tray above the chat list, unread ring vs muted read ring (ui/mod.rs:7566 story_tray; DECISIONS.md Phase 9.1) <!-- parity:stories-tray -->
- [x] Tap a tray entry opens fullscreen viewer on the latest story; missing details prefetched via getStory (ui/mod.rs:5427 open_story_viewer) <!-- parity:stories-viewer-open -->
- [x] Viewer renders photo stories at full size and video stories (video shows thumbnail only; full clip not renderable by the img element) (src/story_viewer.rs; DECISIONS.md Phase 9.1) <!-- parity:stories-viewer-photo-video -->
- [x] Viewer shows poster name + "Story N of M" + caption with formatted entities (src/story_viewer.rs:55-56 caption fields) <!-- parity:stories-viewer-caption -->
- [x] Viewer Prev / Next / Close; Escape closes viewer before other overlays (DECISIONS.md Phase 9.1) <!-- parity:stories-viewer-nav -->
- [x] Segmented progress bar with auto-advance to next story (src/story_viewer.rs StoryPlayback: 5s photos per Telegram Desktop kPhotoDuration, video uses its own storyVideo.duration; src/ui/mod.rs story_progress_bar + 100ms tick; DECISIONS.md Phase 9.8) <!-- parity:stories-progress-bar -->
- [x] Live/unsupported story content degrades to a placeholder in the item list (src/story_viewer.rs:11-13) <!-- parity:stories-live-placeholder -->
- [ ] Join or play live stories (storyContentLive / startLiveStory in schema) <!-- parity:stories-live-play -->
- [x] openStory/closeStory mark stories viewed; read state from max_read_story_id (telegram/requests.rs:2519,2531) <!-- parity:stories-read-state -->
- [x] Quick-react ❤️ toggle on viewer, chosen state shown (ui/mod.rs:5505-5566) <!-- parity:stories-quick-react -->
- [x] Reaction picker fed by getStoryAvailableReactions (ui/mod.rs:5532 toggle_story_reaction_picker) <!-- parity:stories-reaction-picker -->
- [ ] Chosen custom-emoji or paid reactions (partial: envelope parses reactionTypeCustomEmoji/reactionTypePaid into story state + viewers list (S13), setStoryReaction custom-emoji builder + driver; viewer render + picker offer pending post-Phase-9) <!-- parity:stories-custom-reactions -->
- [x] Reaction removal (setStoryReaction with null; request asserts, telegram/requests.rs:2556) <!-- parity:stories-reaction-remove -->
- [x] Interaction counters (views / hearts / reposts, non-zero, when can_get_interactions) (ui/mod.rs Phase 9.2) <!-- parity:stories-interaction-counters -->
- [x] Detailed viewers list (getStoryInteractions, gated on can_get_interactions; paginated panel with reactions, forwards, Load more; ui/mod.rs Phase 9.5) <!-- parity:stories-viewers-list -->
- [x] Text reply to a story via sendMessage + inputMessageReplyToStory, gated on can_be_replied (telegram/requests.rs Phase 9.2; ui/mod.rs Reply row) <!-- parity:stories-reply -->
- [x] Delete own story, gated on can_be_deleted; viewer closes when the story leaves the cache; updateStoryDeleted handled (telegram/requests.rs:2584, telegram/envelope.rs:4660) <!-- parity:stories-delete -->
- [x] Report story (reportStory multi-step flow: option picker → optional/required text → Reported/Failed; state.rs + connect.rs + ui/mod.rs Phase 9.5) <!-- parity:stories-report -->
- [x] Stealth mode / hide view from poster (activateStoryStealthMode + updateStoryStealthMode state; viewer action-row button reflects active/cooldown; ui/mod.rs Phase 9.5) <!-- parity:stories-stealth-mode -->
- [x] Story albums: story page lists/opens albums; create/rename/delete; add/remove/reorder stories; reorder albums (album covers deferred) <!-- parity:stories-albums -->
- [x] Archive story list via getChatArchivedStories with load-more pagination (DECISIONS.md Phase 9.1 "Out of this slice") <!-- parity:stories-archive -->
- [x] Pinned stories on chat page (getChatPostedToChatPageStories + setChatPinnedStories full-list semantics) <!-- parity:stories-pinned -->
- [x] Clickable story areas (location, venue, suggested reaction, message, link, weather, gift) — parsed from `story.areas` (telegram/envelope.rs), rendered as clickable chips on the viewer, taps perform each area's action (ui/mod.rs Phase 9.8) <!-- parity:stories-areas-view -->
- [ ] Story notification settings (mute stories per chat, story sound, show story poster) — parsed into fields only (telegram/envelope.rs:1859-1860) <!-- parity:stories-notify-settings -->
- [x] Story-restriction notices (TGX-verbatim `ChatDisabledStory` / `ChatRestrictedStory`): the `canPostStory` error channel delivers the Disabled notice for both the TDLib client-side-gate message ("Not enough rights to post stories in this group") and `CHAT_ADMIN_REQUIRED`, and the Restricted notice for `USER_RESTRICTED`; unrecognized errors keep the generic eligibility failure. No Until variant — the channel carries no until-date. <!-- parity:stories-restriction-notice -->

#### Post stories
- [x] Story composer: photo picker (path entry — native file picker deferred; `postStory` via td_api.tl:13715) <!-- parity:stories-post-photo-composer -->
- [x] Video story upload (`inputStoryContentVideo`, td_api.tl:6681) <!-- parity:stories-post-video -->
- [x] canPostStory eligibility check before posting (schema td_api.tl:13702) <!-- parity:stories-can-post-check -->
- [x] postStory with caption + privacy selector (td_api.tl:13715; post-succeeded/failed reducer upserts story + queues tray refresh, DECISIONS.md 9.3) <!-- parity:stories-post-call -->
- [x] Privacy selector: Everyone / Contacts / Close friends / Selected users (`storyPrivacySettings*`, td_api.tl:8928-8937) <!-- parity:stories-post-privacy -->
- [x] Formatted caption with entities on post <!-- parity:stories-post-caption -->
- [x] Add story areas (link + suggested-reaction stickers) on own post (`inputStoryAreas`, td_api.tl:6619; composer text inputs, DECISIONS.md 9.4) <!-- parity:stories-post-areas -->
- [x] Active period / expiry selection (6h / 12h / 24h / 48h per `postStory` active_period comment, td_api.tl:13715) <!-- parity:stories-post-expiry -->
- [x] "Post to chat page" toggle (`postStory` is_posted_to_chat_page, td_api.tl:13715) <!-- parity:stories-post-to-chat-page -->
- [x] "Protect content" (no forwarding) toggle (`postStory` protect_content, td_api.tl:13715) <!-- parity:stories-post-protect -->
- [x] Honest pending / succeeded / failed states (`Posting` is set when the `postStory` answer lands; `updateStoryPostSucceeded`/`PostFailed` applied to the composer status line, DECISIONS.md 9.3) <!-- parity:stories-post-status -->
- [x] Edit own story content / caption / areas (editStory, td_api.tl:13732, wired in viewer+composer, Phase 9.5) <!-- parity:stories-edit -->
- [x] Edit story cover frame (editStoryCover, td_api.tl:13738, viewer cover editor, Phase 9.5) <!-- parity:stories-edit-cover -->
- [x] Change a posted story's privacy settings (setStoryPrivacySettings, td_api.tl:13743, viewer privacy editor, Phase 9.5) <!-- parity:stories-post-change-privacy -->
- [x] Post stories on behalf of a channel/supergroup (getChatsToPostStories, canPostStory target chat, Phase 9.5) <!-- parity:stories-post-as-channel -->
- [x] Admin story rights management (post / edit / delete others' stories; TGX strings RightStories*; covered by the existing admin-rights UI which reads/writes can_post/edit/delete_stories) <!-- parity:stories-admin-rights -->
- [x] Repost / re-share a story (storyRepostInfo / storyInteractionTypeRepost parsed; repost via postStory from_story_full_id, Phase 9.5) <!-- parity:stories-repost -->

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
- [x] 1:1 call verification emojis (`callStateReady.emojis` shown on the 1:1 call card, same rendering as group calls; code + demo only, live unverified) <!-- parity:calls-verify-emoji -->
- [x] Share screen in a call (1:1 SEND: `CallEngine::set_screen_share_enabled` re-issues capture sources with the `NTG_MEDIA_SOURCE_DESKTOP` description (replaces camera, ntgcalls no-mix rule), driver toggle `set_call_screen_share` with `MediaDeviceKind::Screen` gate, call-card "Share screen"/"Stop sharing" button + "Sharing your screen" state. 1:1 RECEIVE: P2P frames callback accepts `PLAYBACK+SCREEN` frames into their own slot, `latest_screen_frame` exposes them, inactive drops the slot, and the call card renders the share as the main tile with a peer-screen badge (slice C2l; the camera resumes when the share ends; the peer decode cache keys by (seq, is_screen) so the screen and camera streams never cross-render; `ActiveCall::remote_screen` closes the late-frame race; a paused share states the tile). Code + demo screenshot; live peer share unverified — no ntgcalls runtime on this VM, same caveat as `calls-start-video`) <!-- parity:calls-screen-share -->
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
- [x] Group video paused indicator ("paused" badge on tiles when `video_info.is_paused`; code + demo only, live unverified) <!-- parity:calls-group-video-pause -->
- [x] Local camera preview tile in group calls (engine delivers group CAPTURE frames as the local frame; self tile in the participant grid with live preview, avatar when camera off, local mute badge; code + demo only, live unverified) <!-- parity:calls-group-video-self -->
- [x] Auto-rejoin group call after network loss (auto-rejoin on `need_rejoin`, max 3 attempts, manual retry resets) <!-- parity:calls-rejoin -->
- [x] Record group call (`startGroupCallRecording` video / `endGroupCallRecording`, `can_be_managed`-gated; REC indicator with duration; code + demo only, live recording unverified) <!-- parity:calls-recording -->
- [x] RTMP stream key for video chat (`getVideoChatRtmpUrl` / `replaceVideoChatRtmpUrl`, fetch admin-gated, regenerate owner-gated; code + demo only, live unverified) <!-- parity:calls-rtmp -->
- [x] Set / rename video chat title (`setVideoChatTitle`, 1-64 chars, `can_be_managed`-gated; code + demo only, live unverified) <!-- parity:calls-title -->
- [x] Schedule video chat for later (`createVideoChat` start_date with 10s–8d validation; scheduled card shows start time + admin-only **Start now** (`startScheduledVideoChat`), Join appears once TDLib activates the call; code + demo only, live activation unverified) <!-- parity:calls-schedule -->
- [x] Notify me when a scheduled video chat starts (`toggleVideoChatEnabledStartNotification`, scheduled-only, any viewer; new flag arrives via `updateGroupCall`; code + demo only, live round-trip unverified) <!-- parity:calls-schedule-notify -->
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
- [ ] Trending sticker sets tab (partial: backend wired — getTrendingStickerSets/viewTrendingStickerSets + state; tab UI pending) <!-- parity:stickers-trending -->
- [ ] Search sticker sets / stickers (searchStickerSets/searchStickers) <!-- parity:stickers-search --> (partial: backend wired — searchStickerSets/searchStickers + state; picker UI pending)
- [ ] Favorites sticker tab (getFavoriteStickers/addFavoriteSticker/removeFavoriteSticker) <!-- parity:stickers-favorites --> (partial: backend wired — get/add/removeFavoriteSticker + state; tab UI pending)
- [ ] Recent stickers tab + clear recent stickers (getRecentStickers/clearRecentStickers) <!-- parity:stickers-recent --> (partial: backend wired — getRecentStickers/clearRecentStickers + state; tab UI pending)
- [ ] Install sticker set (changeStickerSet install) <!-- parity:stickers-install --> (partial: backend wired — changeStickerSet install + cache invalidation; UI affordance pending)
- [ ] Remove sticker set with confirm dialog (partial: backend wired — changeStickerSet remove + cache invalidation; UI confirm dialog pending) <!-- parity:stickers-remove -->
- [ ] Archive sticker set + Archived view in settings (restore path) <!-- parity:stickers-archive --> (partial: backend wired — changeStickerSet archive; Archived view UI pending)
- [ ] Reorder installed sticker sets (partial: backend wired — reorderInstalledStickerSets + cache invalidation; drag-reorder UI pending) <!-- parity:stickers-reorder -->
- [x] Dynamic set order (auto-place recently used sets above others) <!-- parity:stickers-dynamic-order --> (backend + state: sticker sends pass update_order_of_installed_sticker_sets, updateInstalledStickerSets reorders the cached sets in place; picker already lists stored order, no UI change)
- [ ] Open sticker set preview screen (title, stickers grid, install/remove from preview) <!-- parity:stickers-set-preview -->
- [ ] "No sticker sets installed" empty state <!-- parity:stickers-empty-state -->
- [ ] "X sets installed" counts and batch install/remove feedback <!-- parity:stickers-install-counts --> (partial: backend wired — changeStickerSet install + cache invalidation; UI affordance pending)
- [ ] Sticker suggestions by emoji in composer (Installed + recommended / Only installed / None) <!-- parity:stickers-suggest-by-emoji --> (partial: backend wired — `StickerSuggestMode` in `src/sticker_suggest.rs` (persisted in `MediaPrefs`, serde-defaulted), trailing-emoji detection, `RequestPurpose::SuggestStickers` + `searchStickers` dispatch into `StickerPanel::suggestions` (never the search slot), `InstalledOnly` filters to installed set ids, `update_sticker_suggestions` driver dedupes repeats and drops stale in-flight suggests; composer suggestion-row UI pending)
- [ ] Animated sticker (TGS) playback in picker and history (partial: format parsed, only thumbnails rendered) <!-- parity:stickers-animated-playback -->
- [ ] Video sticker (WebM) playback (partial: format parsed, static thumb only) <!-- parity:stickers-video-playback -->
- [ ] "Loop Animated Stickers" setting <!-- parity:stickers-loop-setting -->
- [ ] Premium sticker gating ("Sending this sticker requires Telegram Premium") <!-- parity:stickers-premium-gate -->
- [ ] Show "choosing a sticker" chat action of others <!-- parity:stickers-typing-action -->
- [ ] GIF search + trending GIFs (inline bot path searchInlineBots/getInlineQueryResults) <!-- parity:gifs-search-trending --> (partial: backend wired — getInlineQueryResults search + AnimationItem results + next_offset paging + updateAnimationSearchParameters state; search UI pending. Note: `searchInlineBots` does not exist in TDLib 1.8.67 — the bot resolves via the `animation_search_bot_username` option, see DECISIONS.md S9)
- [ ] Save GIF to media keyboard (addSavedAnimation) <!-- parity:gifs-save --> (partial: backend wired — addSavedAnimation + cache invalidation; picker UI pending)
- [ ] Delete saved GIF (removeSavedAnimation + confirm) <!-- parity:gifs-delete --> (partial: backend wired — removeSavedAnimation + cache invalidation; confirm dialog pending)
- [ ] "No GIFs" empty state <!-- parity:gifs-empty-state -->
- [ ] "Autoplay GIFs" setting <!-- parity:gifs-autoplay-setting -->
- [ ] GIF loop playback in history (partial: static frame cache only, ui/mod.rs:3015) <!-- parity:gifs-history-playback -->
- [ ] Apply updateAnimationSearchParameters to GIF search (search still rides the animation_search_bot_username inline-bot path) <!-- parity:gifs-search-parameters --> (partial: backend wired — update parsed + provider/emojis stored in state; search UI pending)
- [ ] Emoji picker in composer with categories (Smileys & People, etc.) and search <!-- parity:emoji-picker --> (partial: backend wired — getEmojiCategories + searchEmojis parsed into EmojiPanel; picker UI pending)
- [ ] Insert emoji at cursor in composer text <!-- parity:emoji-insert -->
- [ ] Big emoji rendering for emoji-only messages (Big Emoji setting) <!-- parity:emoji-big -->
- [ ] Custom emoji packs (browse/install/remove, "Emoji Sets" settings screen) <!-- parity:emoji-custom-packs --> (partial: backend wired — getInstalledStickerSets/getArchivedStickerSets/getTrendingStickerSets/searchStickerSets with stickerTypeEmoji + changeStickerSet reuse + reorderInstalledStickerSets; "Emoji Sets" screen pending)
- [ ] Render custom emoji inside message text <!-- parity:emoji-custom-render --> (partial: backend wired — getCustomEmojiStickers resolves ids to stickers in EmojiPanel; render in text pending)
- [ ] Suggest animated emoji in composer <!-- parity:emoji-suggest-animated --> (partial: backend wired — getAnimatedEmoji + animatedEmoji parse into EmojiPanel; composer suggestion UI pending)
- [ ] Emoji status: select/set status, timed status (1h/2h/8h/2d/custom), trending statuses <!-- parity:emoji-status --> (partial: backend wired — setEmojiStatus incl. null-clear, getRecent/getThemed/getDefault/getUpgradedGiftEmojiStatuses parsed into EmojiPanel; status picker UI pending)
- [ ] Clear recent emoji statuses <!-- parity:emoji-status-clear-recent --> (partial: backend wired — clearRecentEmojiStatuses + cache invalidation; UI affordance pending)
- [ ] Clear recent emoji <!-- parity:emoji-clear-recent --> (partial: no TDLib API — recent plain emoji are client-side state in every official client; the picker slice owns local storage)
- [ ] Dynamic emoji pack order setting <!-- parity:emoji-dynamic-pack-order --> (partial: backend wired — reorderInstalledStickerSets with stickerTypeEmoji; the dynamic toggle itself is client-side recency ordering, settings UI pending)
- [ ] Emoji pack download states (Downloading…/Downloaded/Update Needed/Installing…) <!-- parity:emoji-pack-states -->
- [ ] "Send Stickers & GIFs" permission denied messaging in groups <!-- parity:stickers-permission-messaging -->
- [ ] Group sticker set management (setSupergroupStickerSet / setSupergroupCustomEmojiStickerSet) <!-- parity:stickers-group-set --> (partial: backend landed — both builders + drivers, `can_set_sticker_set` / `sticker_set_id` / `custom_emoji_sticker_set_id` parsed into cached full info; group settings UI deferred post-Phase-9)

### Bots, polls & payments

- [x] Inline keyboard rows rendered under messages with per-button styles (ui/mod.rs:21636) <!-- parity:bots-inline-keyboard-render -->
- [x] URL buttons open the link (ui/mod.rs:21663) <!-- parity:bots-inline-url -->
- [x] Callback buttons answered via getCallbackQueryAnswer; bot answer shown as toast / URL opened (connect.rs:4786, ui/mod.rs:3941) <!-- parity:bots-inline-callback -->
- [x] Switch-inline buttons insert "@bot query" into the composer (ui/mod.rs:21678) <!-- parity:bots-inline-switch -->
- [x] Copy-text buttons copy the text (ui/mod.rs:21681) <!-- parity:bots-inline-copy -->
- [x] Login URL buttons resolve via getLoginUrlInfo; open/consent/failure handled (ui/mod.rs) <!-- parity:bots-inline-login -->
- [x] Web app buttons (partial: honest browser fallback; no in-app web view yet) <!-- parity:bots-inline-webapp -->
- [x] Password-protected callback buttons prompt for 2-step password via callbackQueryPayloadDataWithPassword; wrong-password errors surfaced <!-- parity:bots-inline-callback-password -->
- [x] Game buttons launch via getCallbackQueryAnswer with callbackQueryPayloadGame (messageGame short name); answer URL opens in browser <!-- parity:bots-inline-game -->
- [x] Buy buttons open the payment checkout dialog (getPaymentForm → paymentForm) (ui/mod.rs) <!-- parity:bots-inline-buy -->
- [x] User buttons open the private chat with the user (ui/mod.rs) <!-- parity:bots-inline-user -->
- [x] Per-button disabled flag on inline buttons (verified: TDLib 1.8.67 exposes `inlineKeyboardButtonTypeDisabled`; `envelope.rs:8791` parses it to `InlineKeyboardButtonType::Disabled`; `inline_keyboard_button` (src/ui/mod.rs:44503) renders it non-interactive via the `element.disabled(true)` fallthrough (44599) — no click handler attached — with tooltip "This button is disabled" (44616)) <!-- parity:bots-inline-disabled-buttons -->
- [x] Custom reply keyboards rendered above the composer; text sends, one-time hides on tap, contact/location/poll honestly disabled <!-- parity:bots-custom-keyboard -->
- [x] Force-reply markup focuses the composer with the reply target set <!-- parity:bots-force-reply -->
- [ ] Force-reply keyboards render the reply-keyboard bar (the official clients show the custom-keyboard UI for forceReply markup; Quill covers only composer focus) <!-- parity:bots-force-reply-keyboard -->
- [ ] Stop button for streaming bot drafts (cancel an in-flight streaming bot reply) <!-- parity:bots-streaming-draft-stop -->
- [x] Bot info panel with description and tappable /command buttons inserting into the composer (ui/mod.rs:13726) <!-- parity:bots-info-panel -->
- [x] Bot START button / start_parameter deep links (partial: link parser + armed START state are wired, but Quill registers no t.me/tg: URL scheme so OS deep-link intake is out of this slice): t.me/<bot>?start=<param> parses to (bot, param); START button sends sendBotStartMessage with the parameter (state.rs, ui/mod.rs) <!-- parity:bots-start -->
- [x] Restart bot: confirm-gated; clears the bot chat history (deleteChatHistory, kept in list) then re-sends sendBotStartMessage with an empty parameter (connect.rs:restart_bot) <!-- parity:bots-restart -->
- [x] Share bot: copies the t.me/<username> link to the clipboard (ui/mod.rs) <!-- parity:bots-share -->
- [x] Block / unblock bot: setMessageSenderBlockList (CL3 plumbing); label follows updateChatBlockList state (ui/mod.rs) <!-- parity:bots-block -->
- [x] Bot menu button (partial: honest browser fallback; no in-app web view) — renders botInfo.menu_button in the profile panel, URL opens in OS browser <!-- parity:bots-menu-button -->
- [x] Bot privacy settings (read-only): privacy-policy URL button, /privacy command fallback, else the schema's telegram.org/privacy-tpa fallback note — no client-side bot privacy setting exists in the schema <!-- parity:bots-privacy -->
- [x] Similar bots section in the bot profile: getBotSimilarBots, names resolved from the user cache, tap opens the bot chat (state.rs, connect.rs, ui/mod.rs) <!-- parity:bots-similar -->
- [x] `/` command menu merging chat-specific bot commands and global getCommands (state.rs:3100) <!-- parity:bots-command-menu -->
- [ ] Ephemeral-command icon in the bot command list (BotCommand.is_ephemeral) <!-- parity:bots-ephemeral-command-icon -->
- [ ] Inline mode: type @bot in composer, inline query results list, send an inline result (no getInlineQueryResults code in src) <!-- parity:bots-inline-mode -->
- [ ] Games: send / play, high scores (no game code at all) <!-- parity:bots-games -->
- [x] Poll creation dialog: question, add/remove options (2–10), validation errors, anonymous + multiple-answer toggles (ui/mod.rs:14860, poll.rs:112) <!-- parity:bots-poll-create -->
- [x] Create quiz polls: quiz-mode toggle, mark correct option (radio), quiz explanation (0–200 chars, ≤2 line feeds); sends `inputPollTypeQuiz` (ui/mod.rs, poll.rs, requests.rs) <!-- parity:bots-poll-create-quiz -->
- [x] Poll description field (shown above the poll title) (ui/mod.rs, requests.rs) <!-- parity:bots-poll-description -->
- [x] Poll duration setting: auto-close after N hours (1–24 UI ceiling, `open_period`; true max is server `getOption("poll_open_period_max")`) (ui/mod.rs, poll.rs) <!-- parity:bots-poll-duration -->
- [x] Revoting toggle in creation dialog (`allows_revoting`; forced off in quiz mode) (ui/mod.rs, requests.rs) <!-- parity:bots-poll-revoting -->
- [x] Shuffle options toggle (`shuffle_options`) (ui/mod.rs, requests.rs) <!-- parity:bots-poll-shuffle -->
- [x] Show voters toggle (verified: TGX implements it as the inverse of `is_anonymous`; the poll dialog's "Anonymous voting" checkbox (src/ui/mod.rs:33786) drives `PollDialog.is_anonymous` (325, default true at 360), frozen into `PollDraft` (392), sent via `connect.rs:10777` as `"is_anonymous": poll.is_anonymous` in the `inputMessagePoll` payload (`requests.rs:4184`); voter-list display is the already-checked `bots-poll-voters` box) <!-- parity:bots-poll-show-voters -->
- [x] Country restriction setting: comma-separated ISO codes (`country_codes`; count cap is server-enforced `poll_country_count_max`, channel-only) (ui/mod.rs, poll.rs, requests.rs) <!-- parity:bots-poll-countries -->
- [x] Poll discard-confirmation prompt when closing the dialog with unsent input (ui/mod.rs) <!-- parity:bots-poll-discard -->
- [x] Voting: single, multiple, retract-when-revoting, quiz answering, closed-poll blocked, optimistic UI with rollback on failure (poll.rs:51, connect.rs:5287) <!-- parity:bots-poll-vote -->
- [x] Live poll updates applied in place via updatePoll (state.rs:3962) <!-- parity:bots-poll-live-update -->
- [x] Results: percentage bars, voter counts, chosen marks, correct-answer mark on closed quizzes (ui/mod.rs:21764) <!-- parity:bots-poll-results -->
- [x] View voter list (getPollVoters) <!-- parity:bots-poll-voters -->
- [x] Stop poll / stop quiz with confirmation warning (stopPoll + red confirm banner; TGX warning copy) <!-- parity:bots-poll-stop -->
- [x] Quiz explanation auto-shown after an incorrect answer (`pollTypeQuiz.explanation`; no lamp-icon on-demand reveal yet — see DECISIONS.md) <!-- parity:bots-poll-quiz-explanation -->
- [x] Vote restriction reasons display (closed / country / membership) <!-- parity:bots-poll-restrictions -->
- [x] Poll entry gated on can_send_polls; restricted-poll notices <!-- parity:bots-poll-permissions -->
- [x] Poll stopped service message <!-- parity:bots-poll-service-message -->
- [x] Invoice message rendering: product card with title, description, total price, TEST badge; paid invoices link to their receipt (ui/mod.rs) <!-- parity:bots-payment-invoice -->
- [x] Payment checkout flow: getPaymentForm dialog, validateOrderInfo with shipping options, saved/new credentials, terms consent, sendPaymentForm; provider/additional-option/verification URLs open in the OS browser; Stars forms honestly declined (envelope.rs, requests.rs, connect.rs, state.rs, ui/mod.rs) <!-- parity:bots-payment-checkout -->
- [x] Payment receipts: getPaymentReceipt dialog from paid invoices ("View receipt") plus messagePaymentSuccessful/messagePaymentSuccessfulBot rows (ui/mod.rs) <!-- parity:bots-payment-receipt -->
- [ ] Recurring payments <!-- parity:bots-payment-recurring -->
- [ ] Clear payment/shipping info (privacy) <!-- parity:bots-payment-clear -->
- [ ] Signed gifts: personal comment when buying a collectible gift via Marketplace (sendResoldGift text) <!-- parity:gifts-signed-comment -->
- [ ] Signed gifts: custom signature on Marketplace gift purchase (blocked: no TDLib/raw API for a gift signature field — concept-level search: sendResoldGift/inputInvoiceStarGiftResale carry text/message only) <!-- parity:gifts-signed-signature -->

### Settings

- [x] Light/dark theme switcher, live via `Theme::change` (🎨 Appearance dialog, `src/ui/appearance.rs` `apply_appearance`) <!-- parity:settings-theme-switch -->
- [x] Auto-night mode: Off / System (OS appearance) / Scheduled (local-time window, 1-min re-check; `night_active`, `src/settings.rs`) <!-- parity:settings-auto-night -->
- [x] Accent color picker (presets + Default), live via theme accent override <!-- parity:settings-accent-color -->
- [x] Chat wallpaper (solid-color presets + Default), painted behind the message list <!-- parity:settings-chat-wallpaper -->
- [x] Message font size (12–20 px), live across message text and captions (`BubbleLook`) <!-- parity:settings-font-size -->
- [x] Bubble vs plain chat style, live (`BubbleLook.plain` drops bubble bg/rounding) <!-- parity:settings-bubble-style -->
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
- [x] Privacy: Last Seen & Online <!-- parity:settings-privacy-lastseen -->
- [x] Privacy: Phone Number visibility <!-- parity:settings-privacy-phone -->
- [x] Privacy: Profile Photos visibility <!-- parity:settings-privacy-photo -->
- [x] Privacy: Forward My Messages (link in forwarded messages) <!-- parity:settings-privacy-forwards -->
- [x] Privacy: Call Me, incl. peer-to-peer calls (partial: base Everybody/Contacts/Nobody choice only; call exceptions not editable — see DECISIONS.md) <!-- parity:settings-privacy-calls -->
- [x] Privacy: Add Me to Groups and Channels <!-- parity:settings-privacy-invites -->
- [x] Privacy: See My Read Date <!-- parity:settings-privacy-readreceipts -->
- [x] Blocked users list <!-- parity:settings-blocked-users -->
- [x] Privacy exceptions per rule (always allow / never allow user lists; rule keys only — call rules keep the base choice, see DECISIONS.md) <!-- parity:settings-privacy-exceptions -->
- [x] Auto-download per network (mobile / Wi-Fi / roaming) and media type <!-- parity:settings-auto-download -->
- [x] Use less data for calls <!-- parity:settings-less-data-calls -->
- [x] Storage usage view with per-chat / file-type breakdown <!-- parity:settings-storage-usage -->
- [x] Clear cache <!-- parity:settings-clear-cache -->
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
- [ ] Check for updates automatically on launch against GitHub Releases (latest tag vs compiled-in `CARGO_PKG_VERSION`), with an opt-out toggle in Settings <!-- parity:platform-update-check-auto -->
- [ ] Manual "Check for updates" action in Settings/menu <!-- parity:platform-update-check-manual -->
- [ ] Update-available UI: non-intrusive banner/dialog showing the new version and release notes <!-- parity:platform-update-available-ui -->
- [ ] One-click download, install, and restart (replace own binary, relaunch; user confirms — no silent auto-install) <!-- parity:platform-update-install -->
- [ ] Honest updater states: already up to date, no network, download/install failed with retry <!-- parity:platform-update-states -->
- [ ] Outdated-feature placeholder: placeholder card with one-tap update button when the app can't render a new feature <!-- parity:platform-update-placeholder -->
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
