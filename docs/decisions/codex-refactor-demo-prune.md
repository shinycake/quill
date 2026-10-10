# Demo pruning: candidate list

A proposal for the owner to approve before anything is deleted. This PR adds
only this file.

`--screenshot-demo` has 292 registered kinds (origin/main at `404d33e2`).
Most were added as the screenshot for one PR and nothing has used them
since. Each kind costs a fixture, a setup method and a registration, and
the setups read and write `QuillApp` fields, so every state refactor and
many feature PRs edit them.

## How a kind gets on the list

A kind is a candidate when all of these hold:

- No README or site image comes from it. Every image in `README.md`,
  `docs/screenshots/readme-*.png` and `site/img/` is captured from
  `ready-showcase` (with `QUILL_DEMO_SHOWCASE` picking the view), so this
  rules out only that kind.
- No script, test, workflow or source file names it, apart from the frozen
  list in `src/ui/demos/kinds_before_registry.txt` and its own
  `register_demos!` entry. Searched: `scripts/`, `tests/`, `.github/`,
  `site/`, `README.md`, `ROADMAP.md`, the other `docs/*.md`, `Cargo.toml`,
  `build.rs`, and string literals under `src/`.
- Or it is a variant that another kind already shows through its
  environment switch.

Decision docs do not count as a use. Where a kind appears in one, the table
names the doc, since that is usually the PR it was the screenshot for.

## Summary

| | Kinds |
| --- | ---: |
| Registered | 292 |
| Kept (a script, test, README line or source file uses them) | 59 |
| Candidates with no use | 222 |
| Candidates named only by ROADMAP.md, for screenshots that are gone | 10 |
| Covered by an env-switched kind | 1 |

So 233 of 292 kinds could go.

## Before deleting

- The frozen list test (`every_earlier_kind_is_still_registered`) checks
  that the 291 original kinds still resolve. The deletion PR has to remove
  the deleted kinds from `kinds_before_registry.txt` in the same commit.
- Some fixtures (`apply_ready_*`) are shared between a candidate and a kept
  kind, or with GPUI tests. Delete a fixture only when the compiler says
  nothing else calls it.
- Removing a whole `ui/demos/<area>.rs` file also removes its `mod` line in
  `ui/demos/mod.rs`.
- The 10 ROADMAP-only kinds need their ROADMAP lines edited in the same PR.
- For the env-variant check I read the setups of the kinds that switch
  on a `QUILL_DEMO_*` variable and compared them with the kinds nearest in
  name and fixture. Only one is a real duplicate. Others look close but use
  different fixtures: `ready-invite-links` and `ready-links-boosts`
  (`admin-links`, `link-requests`), `ready-location` and
  `ready-rendering-leftovers` (`location` opens the share panel, not the
  message), `ready-sessions` and `ready-session-details`.
- When a kind is the only way to see a screen, a GPUI UI test of that
  screen is the replacement, as `docs/contributing/structure.md` (in the
  guardrails PR) recommends.

## The list

Grouped by the file that registers the kind.

### Candidates

`admin_extras_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-admin-extras` | screenshot for `codex-admin-extras` |

`bots_extras_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-bot-extras` | screenshot for `codex-bots-extras` |

`chatlist_global_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-chatlist-birthdays` | screenshot for `codex-chatlist-global` |
| `ready-chatlist-calls-clear` | screenshot for `codex-chatlist-global` |
| `ready-chatlist-contacts-index` | screenshot for `codex-chatlist-global` |
| `ready-chatlist-stories-menu` | screenshot for `codex-chatlist-global` |
| `ready-chatlist-suggestions` | screenshot for `codex-chatlist-global` |
| `ready-chatlist-suggestions-phone` | screenshot for `codex-chatlist-global` |

`chatlist_rows_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-archive-hint` | screenshot for `codex-chatlist-rows` |
| `ready-chat-badges` | screenshot for `codex-chatlist-rows` |
| `ready-chat-export` | screenshot for `codex-platform-shortcuts-data` |
| `ready-folders-chat-picker` | screenshot for `codex-chatlist-rows` |
| `ready-folders-chats` | screenshot for `codex-chatlist-rows` |
| `ready-folders-toast` | screenshot for `codex-chatlist-rows` |
| `ready-window-settings` | screenshot for `codex-platform-shortcuts-data` |

`composer_leftovers_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-restricted-composer` | screenshot for `codex-composer-leftovers` |
| `ready-voice-pause` | screenshot for `codex-composer-leftovers` |

`demos/bots_profile.rs`:

| Kind | Reason |
| --- | --- |
| `ready-bot-profile` | no reference outside its registration |
| `ready-inline-results` | no reference outside its registration |
| `ready-rich-ai-tools` | screenshot for `parity-msg-richtext-ai-tools` |
| `ready-rich-editor` | no reference outside its registration |
| `ready-rich-message` | screenshot for `codex-history-rows-owned` |
| `ready-rich-premium-gate` | screenshot for `parity-msg-richtext-premium-gate` |
| `ready-username` | no reference outside its registration |

`demos/calls.rs`:

| Kind | Reason |
| --- | --- |
| `ready-auto-delete` | screenshot for `codex-notifications-mute` |
| `ready-call` | screenshot for `codex-call-peer-negotiation` |
| `ready-call-devices` | no reference outside its registration |
| `ready-call-reconnecting` | no reference outside its registration |
| `ready-call-screenshare` | no reference outside its registration |
| `ready-call-screenshare-receive` | no reference outside its registration |
| `ready-call-swap` | no reference outside its registration |
| `ready-call-video` | no reference outside its registration |
| `ready-calls-settings` | no reference outside its registration |
| `ready-chat-ttl` | no reference outside its registration |
| `ready-group-call` | no reference outside its registration |
| `ready-group-call-invitation` | no reference outside its registration |
| `ready-group-call-invite` | no reference outside its registration |
| `ready-group-call-join-as` | screenshot for `codex-calls-polish` |
| `ready-group-call-manage` | no reference outside its registration |
| `ready-group-call-polish` | screenshot for `codex-calls-polish` |
| `ready-group-call-scheduled` | no reference outside its registration |
| `ready-group-call-stage` | screenshot for `codex-calls-live` |
| `ready-mute-custom` | screenshot for `codex-notifications-mute` |

`demos/chat_list.rs`:

| Kind | Reason |
| --- | --- |
| `ready-archive-bar` | screenshot for `codex-chatlist-archive-stories` |
| `ready-archive-menu` | screenshot for `codex-chatlist-archive-stories` |
| `ready-archive-row` | screenshot for `codex-chatlist-archive-stories` |
| `ready-chat-header` | screenshot for `codex-chat-header` |
| `ready-chat-list` | screenshot for `codex-chatlist-rows`, `codex-interface-scale-full`, `codex-rust-cargo-updates` |
| `ready-chat-list-2` | no reference outside its registration |
| `ready-chat-list-archive` | no reference outside its registration |
| `ready-chat-preview` | no reference outside its registration |
| `ready-chat-rows` | screenshot for `codex-chat-row-polish-2` |
| `ready-join-bar` | screenshot for `codex-join-bar-search-rows` |
| `ready-multiline-rows` | screenshot for `codex-join-bar-search-rows` |
| `ready-notification-sound` | no reference outside its registration |
| `ready-pin-drag` | screenshot for `codex-chatlist-archive-stories` |
| `ready-search-previews` | screenshot for `codex-join-bar-search-rows` |
| `ready-shared-media` | no reference outside its registration |
| `ready-stories-collapsed` | screenshot for `codex-chatlist-swipe-stories` |
| `ready-stories-collapsing` | screenshot for `codex-chatlist-swipe-stories` |
| `ready-stories-expanded` | screenshot for `codex-chatlist-swipe-stories` |
| `ready-swipe-mute` | screenshot for `codex-chatlist-swipe-stories` |
| `ready-swipe-reached` | screenshot for `codex-chatlist-swipe-stories` |
| `ready-top-bars` | screenshot for `codex-chat-top-bars` |

`demos/composer.rs`:

| Kind | Reason |
| --- | --- |
| `ready-composer-wysiwyg` | screenshot for `codex-composer-input` |
| `ready-deep-link-info` | screenshot for `parity-platform-deep-links` |
| `ready-deep-link-invite` | screenshot for `parity-platform-deep-links` |
| `ready-deep-link-share` | screenshot for `codex-deep-link-types` |
| `ready-paste-image` | screenshot for `parity-platform-paste-image` |
| `ready-suggest-emoji` | screenshot for `codex-composer-suggest` |
| `ready-suggest-hashtag` | screenshot for `codex-composer-suggest` |

`demos/groups.rs`:

| Kind | Reason |
| --- | --- |
| `ready-block-user` | no reference outside its registration |
| `ready-bot-topics` | screenshot for `codex-subsection-tabs` |
| `ready-bot-topics-bottom` | screenshot for `codex-subsection-tabs` |
| `ready-bot-topics-left` | screenshot for `codex-subsection-tabs` |
| `ready-chat-avatars` | no reference outside its registration |
| `ready-contacts-manage` | no reference outside its registration |
| `ready-folder-badges` | screenshot for `codex-notify-os` |
| `ready-folders-add-link` | screenshot for `codex-folders-appearance` |
| `ready-folders-icons` | screenshot for `codex-folders-appearance` |
| `ready-folders-manage` | screenshot for `codex-folders-appearance` |
| `ready-folders-share` | screenshot for `codex-folders-appearance` |
| `ready-folders-sidebar` | screenshot for `codex-folders-appearance`, `codex-rust-cargo-updates` |
| `ready-notify-os` | screenshot for `codex-notify-os` |
| `ready-slow-mode` | screenshot for `codex-composer-polish` |
| `ready-topic-post` | no reference outside its registration |

`demos/groups_admin.rs`:

| Kind | Reason |
| --- | --- |
| `ready-admin-management` | no reference outside its registration |
| `ready-avatar-profile` | no reference outside its registration |
| `ready-channel-stats` | screenshot for `codex-gpui-kit-0.7.1` |
| `ready-community-create` | no reference outside its registration |
| `ready-community-hub` | no reference outside its registration |
| `ready-community-info` | no reference outside its registration |
| `ready-group-manage` | no reference outside its registration |
| `ready-groups2` | no reference outside its registration |
| `ready-invite-links` | no reference outside its registration |

`demos/media.rs`:

| Kind | Reason |
| --- | --- |
| `ready-animated-emoji` | no reference outside its registration |
| `ready-custom-emoji` | screenshot for `codex-rich-text-inline` |
| `ready-drop-folder` | screenshot for `codex-composer-core` |
| `ready-drop-zones` | screenshot for `codex-composer-core` |
| `ready-emoji-panel` | screenshot for `codex-media-panel` |
| `ready-game-card` | no reference outside its registration |
| `ready-player-bar` | screenshot for `codex-audio-player-bar` |
| `ready-shortcuts` | no reference outside its registration |

`demos/messages.rs`:

| Kind | Reason |
| --- | --- |
| `ready-edit-media` | screenshot for `codex-edit-media`, `codex-history-motion` |
| `ready-jump-date` | screenshot for `codex-find-in-history` |
| `ready-reply-media` | screenshot for `codex-history-motion` |
| `ready-reveal` | screenshot for `codex-history-motion` |
| `ready-scheduled` | screenshot for `codex-scheduled-messages`, `codex-selection-bulk` |
| `ready-search-filters` | screenshot for `codex-find-in-history` |
| `ready-search-frequent` | screenshot for `codex-search-upgrades` |
| `ready-search-from` | screenshot for `codex-find-in-history` |
| `ready-search-from-hits` | screenshot for `codex-find-in-history` |
| `ready-search-public` | screenshot for `codex-search-upgrades` |
| `ready-select-keyboard` | screenshot for `codex-selection-bulk` |
| `ready-select-mode` | screenshot for `codex-history-motion`, `codex-selection-pin` |

`demos/payments.rs`:

| Kind | Reason |
| --- | --- |
| `ready-payments` | no reference outside its registration |
| `ready-video-playback` | no reference outside its registration |

`demos/platform.rs`:

| Kind | Reason |
| --- | --- |
| `ready-offline` | no reference outside its registration |
| `ready-offline-toast` | screenshot for `parity-platform-offline-errors` |
| `ready-reconnecting` | screenshot for `parity-platform-reconnect-states` |

`demos/privacy_media.rs`:

| Kind | Reason |
| --- | --- |
| `ready-ask-question` | screenshot for `codex-settings-account` |
| `ready-blockquote-expandable` | no reference outside its registration |
| `ready-bubble-headers` | screenshot for `codex-bubble-headers`, `codex-rust-cargo-updates` |
| `ready-caption-position` | no reference outside its registration |
| `ready-code-language` | screenshot for `codex-composer-core`, `codex-composer-input` |
| `ready-composer-preview` | no reference outside its registration |
| `ready-file-open-confirm` | screenshot for `codex-privacy-data-settings` |
| `ready-forum-thread-stories` | screenshot for `codex-forum-thread-stories` |
| `ready-forums-saved` | screenshot for `codex-forums-saved` |
| `ready-gift-cards` | screenshot for `codex-stars-gifts-premium` |
| `ready-gifts` | screenshot for `codex-stars-gifts-premium` |
| `ready-mentions` | screenshot for `codex-mentions` |
| `ready-premium` | screenshot for `codex-stars-gifts-premium` |
| `ready-preview-cards` | no reference outside its registration |
| `ready-privacy-calls` | screenshot for `codex-settings-account` |
| `ready-privacy-gifts` | screenshot for `codex-privacy-data-settings` |
| `ready-rendering-leftovers` | screenshot for `codex-rendering-leftovers` |
| `ready-reply-keyboard` | screenshot for `codex-translate-bar-keyboards` |
| `ready-rtl-composer` | screenshot for `codex-gpui-kit-0.7.1`, `codex-rtl-bubbles`, `codex-rtl-composer` and more |
| `ready-rtl-polish` | screenshot for `codex-rtl-emoji`, `codex-rtl-polish` |
| `ready-service-media` | screenshot for `codex-render-service-media` |
| `ready-service-messages` | no reference outside its registration |
| `ready-session-details` | screenshot for `codex-privacy-data-settings` |
| `ready-settings-help` | screenshot for `codex-settings-account` |
| `ready-stars` | screenshot for `codex-stars-gifts-premium` |
| `ready-threads` | screenshot for `codex-comments-threads` |
| `ready-translate` | screenshot for `codex-translate-bar-keyboards`, `codex-translation` |

`demos/security.rs`:

| Kind | Reason |
| --- | --- |
| `ready-2fa-forgot` | no reference outside its registration |
| `ready-2fa-manage` | no reference outside its registration |
| `ready-2fa-reset` | no reference outside its registration |
| `ready-account` | no reference outside its registration |
| `ready-accounts` | no reference outside its registration |
| `ready-appearance` | screenshot for `codex-composer-core`, `codex-gpui-kit-0.7.1`, `codex-interface-scale-full` |
| `ready-appearance-power` | screenshot for `codex-appearance-2` |
| `ready-appearance-wallpapers` | screenshot for `codex-appearance-wallpapers`, `codex-interface-scale-full`, `codex-rust-cargo-updates` |
| `ready-background-link` | screenshot for `codex-chat-wallpapers-themes` |
| `ready-chat-look` | screenshot for `codex-chat-wallpapers-themes` |
| `ready-chat-theme` | screenshot for `codex-chat-wallpapers-themes` |
| `ready-dictionaries` | screenshot for `codex-data-storage` |
| `ready-key-verification` | no reference outside its registration |
| `ready-keybindings` | no reference outside its registration |
| `ready-local-storage` | no reference outside its registration |
| `ready-lock-screen` | screenshot for `codex-local-passcode` |
| `ready-login-email` | no reference outside its registration |
| `ready-login-prevented` | no reference outside its registration |
| `ready-new-login` | no reference outside its registration |
| `ready-passcode-create` | screenshot for `codex-local-passcode` |
| `ready-passcode-settings` | screenshot for `codex-local-passcode` |
| `ready-recovery-email` | no reference outside its registration |
| `ready-secret-bot-alert` | no reference outside its registration |
| `ready-secret-chat` | no reference outside its registration |
| `ready-secret-picker` | no reference outside its registration |
| `ready-self-destruct` | no reference outside its registration |
| `ready-service-notice` | no reference outside its registration |
| `ready-session-toggles` | no reference outside its registration |
| `ready-subscriptions` | no reference outside its registration |
| `ready-terms` | no reference outside its registration |
| `ready-web-sessions` | no reference outside its registration |

`demos/signin.rs`:

| Kind | Reason |
| --- | --- |
| `connection-closed` | screenshot for `codex-session-recovery` |
| `wait-code-firebase` | screenshot for `codex-auth-leftovers` |
| `wait-code-flash` | screenshot for `codex-auth-leftovers` |
| `wait-code-fragment` | screenshot for `codex-auth-leftovers` |
| `wait-code-missed` | screenshot for `codex-auth-leftovers` |
| `wait-code-resend` | screenshot for `codex-signin-polish` |
| `wait-phone-banned` | screenshot for `codex-signin-polish` |
| `wait-phone-country` | screenshot for `codex-signin-polish` |
| `wait-phone-formatted` | screenshot for `codex-signin-polish` |
| `wait-qr` | no reference outside its registration |

`demos/stories.rs`:

| Kind | Reason |
| --- | --- |
| `ready-story-albums` | screenshot for `codex-story-albums-theme` |
| `ready-story-areas` | no reference outside its registration |
| `ready-story-composer` | no reference outside its registration |
| `ready-story-edit` | no reference outside its registration |
| `ready-story-more` | screenshot for `codex-stories-more` |
| `ready-story-post` | screenshot for `parity-stories-custom-reactions` |
| `ready-story-video` | screenshot for `codex-stories-more` |
| `ready-story-viewers` | no reference outside its registration |

`folder_demo_followups.rs`:

| Kind | Reason |
| --- | --- |
| `ready-folders-delete` | no reference outside its registration |
| `ready-folders-limit` | no reference outside its registration |
| `ready-folders-menu` | no reference outside its registration |
| `ready-folders-new-chats` | no reference outside its registration |
| `ready-folders-new-chats-join` | no reference outside its registration |
| `ready-folders-tag-color` | screenshot for `codex-gpui-kit-0.7.1` |
| `ready-folders-tags` | screenshot for `codex-folders-followups` |

`forum_column_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-forum-column` | screenshot for `codex-forum-column-threads` |

`group_admin_settings_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-group-admin-settings` | screenshot for `codex-group-admin-toggles` |

`links_boosts_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-links-boosts` | screenshot for `codex-admin-links-boosts` |

`member_moderation_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-member-moderation` | screenshot for `codex-member-moderation` |

`message_menu_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-message-menu` | screenshot for `codex-gpui-kit-0.7.1`, `codex-member-moderation`, `codex-message-context-menu` and more |

`profile_panels_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-profile-panels` | screenshot for `codex-profile-panels-2`, `codex-profile-panels` |

`proxy.rs`:

| Kind | Reason |
| --- | --- |
| `ready-proxy` | screenshot for `codex-proxy-settings` |

`render_followups_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-render-followups` | screenshot for `codex-render-followups` |

`reply_options_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-reply-elsewhere` | screenshot for `codex-reply-elsewhere` |
| `ready-reply-external` | screenshot for `codex-reply-elsewhere` |
| `ready-reply-quote` | screenshot for `codex-reply-elsewhere` |

`share_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-forward-bar` | screenshot for `codex-share-box` |
| `ready-send-as` | screenshot for `codex-share-box` |
| `ready-share-box` | screenshot for `codex-share-box` |

`updates_sync_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-updates-sync` | screenshot for `codex-updates-sync` |

`viewer_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-viewer-extras` | screenshot for `codex-viewer-extras` |
| `ready-viewer-gif` | screenshot for `codex-viewer-gifs-paging` |
| `ready-viewer-shared` | screenshot for `codex-viewer-gifs-paging` |

`web_app_demo.rs`:

| Kind | Reason |
| --- | --- |
| `ready-mini-app` | screenshot for `codex-miniapp-webview` |

### Only ROADMAP.md names them

| Kind | Registered in | Reason |
| --- | --- | --- |
| `ready-bot-command-menu` | `demos/bots_profile.rs` | ROADMAP.md links a `docs/screenshots/ready-bot-command-menu.png` that is no longer in the repo; no reference outside its registration |
| `ready-contacts` | `demos/groups.rs` | ROADMAP.md links a `docs/screenshots/ready-contacts.png` that is no longer in the repo; no reference outside its registration |
| `ready-dice` | `demos/payments.rs` | ROADMAP.md links a `docs/screenshots/ready-dice.png` that is no longer in the repo; no reference outside its registration |
| `ready-folders` | `demos/groups.rs` | ROADMAP.md links a `docs/screenshots/ready-folders.png` that is no longer in the repo; no reference outside its registration |
| `ready-forum-topics` | `demos/groups.rs` | ROADMAP.md links a `docs/screenshots/ready-forum-topics.png` that is no longer in the repo; no reference outside its registration |
| `ready-location` | `demos/payments.rs` | ROADMAP.md links a `docs/screenshots/ready-location.png` that is no longer in the repo; no reference outside its registration |
| `ready-media-viewer` | `demos/payments.rs` | ROADMAP.md links a `docs/screenshots/ready-media-viewer.png` that is no longer in the repo; screenshot for `codex-viewer-parity` |
| `ready-poll` | `demos/payments.rs` | ROADMAP.md links a `docs/screenshots/ready-poll.png` that is no longer in the repo; screenshot for `codex-polls-checklists` |
| `ready-seek-bars` | `demos/media.rs` | ROADMAP.md links a `docs/screenshots/ready-seek-bars.png` that is no longer in the repo; no reference outside its registration |
| `ready-stories` | `demos/stories.rs` | ROADMAP.md links a `docs/screenshots/ready-stories.png` that is no longer in the repo; no reference outside its registration |

### Covered by an env-switched kind

| Kind | Covered by |
| --- | --- |
| `ready-admin-log` | `ready-admin-extras` with its default `QUILL_DEMO_ADMIN_EXTRAS=log` seeds the same admin-log fixture, adds the admin list and opens the same info panel |

### Kept

| Kind | Used by |
| --- | --- |
| `need-tdjson` | `scripts/capture-connect-screenshots.sh` |
| `ready-albums` | `scripts/capture-connect-screenshots.sh` |
| `ready-audio` | `scripts/capture-connect-screenshots.sh` |
| `ready-bot-chat` | `ROADMAP.md`, `scripts/capture-connect-screenshots.sh` |
| `ready-bot-keyboard` | `ROADMAP.md`, `scripts/capture-connect-screenshots.sh` |
| `ready-channels` | `ROADMAP.md`, `scripts/capture-connect-screenshots.sh` |
| `ready-channels-admin` | `ROADMAP.md`, `scripts/capture-connect-screenshots.sh` |
| `ready-chat-list-3` | `README.md` |
| `ready-chat-list-search` | `README.md` |
| `ready-chats` | `scripts/capture-connect-screenshots.sh`, `scripts/idle-cpu-bench.sh`, `scripts/macos-navigation-smoke.sh`, `src/ui/esc_stack.rs` |
| `ready-chats-composer` | `scripts/capture-connect-screenshots.sh` |
| `ready-downloads` | `scripts/capture-connect-screenshots.sh` |
| `ready-drafts` | `scripts/capture-connect-screenshots.sh` |
| `ready-edit-delete` | `scripts/capture-connect-screenshots.sh` |
| `ready-emoji-packs` | `scripts/macos-accessibility-smoke.sh` |
| `ready-forward` | `scripts/capture-connect-screenshots.sh` |
| `ready-gif-playback` | `scripts/idle-cpu-bench.sh`, `scripts/macos-gif-smoke.sh` |
| `ready-gifs` | `scripts/capture-connect-screenshots.sh` |
| `ready-group-info-edit` | `README.md` |
| `ready-link-preview` | `scripts/capture-connect-screenshots.sh` |
| `ready-marketplace-gift` | `scripts/macos-accessibility-smoke.sh` |
| `ready-media` | `scripts/capture-connect-screenshots.sh` |
| `ready-mute-archive` | `scripts/capture-connect-screenshots.sh` |
| `ready-pin` | `scripts/capture-connect-screenshots.sh` |
| `ready-privacy` | `src/ui/privacy_extra.rs` |
| `ready-profile-edit` | `scripts/macos-settings-accessibility-smoke.sh`, `scripts/macos-voiceover-smoke.sh` |
| `ready-reactions` | `scripts/capture-connect-screenshots.sh` |
| `ready-reply` | `scripts/capture-connect-screenshots.sh` |
| `ready-search` | `scripts/capture-connect-screenshots.sh` |
| `ready-search-in-chat` | `scripts/capture-connect-screenshots.sh` |
| `ready-send-media` | `scripts/capture-connect-screenshots.sh` |
| `ready-sessions` | `scripts/macos-device-link-smoke.sh`, `scripts/macos-settings-accessibility-smoke.sh` |
| `ready-showcase` | `scripts/gen-showcase-fixtures.sh`, `scripts/hero/record.sh`, `scripts/hero/tour.sh` |
| `ready-spellcheck` | `scripts/capture-connect-screenshots.sh` |
| `ready-spellcheck-panel` | `scripts/capture-connect-screenshots.sh` |
| `ready-spellcheck-toggle` | `scripts/capture-connect-screenshots.sh` |
| `ready-sponsored` | `scripts/capture-connect-screenshots.sh` |
| `ready-sticker-playback` | `scripts/idle-cpu-bench.sh`, `scripts/macos-sticker-smoke.sh` |
| `ready-stickers` | `scripts/capture-connect-screenshots.sh` |
| `ready-storage-usage` | `scripts/macos-account-export-smoke.sh`, `scripts/macos-settings-accessibility-smoke.sh` |
| `ready-text-entities` | `ROADMAP.md`, `scripts/macos-accessibility-smoke.sh` |
| `ready-tray-behavior` | `scripts/macos-minimize-tray-smoke.sh` |
| `ready-typing` | `scripts/capture-connect-screenshots.sh`, `scripts/idle-cpu-bench.sh` |
| `ready-unread` | `scripts/capture-connect-screenshots.sh` |
| `ready-unread-read` | `scripts/capture-connect-screenshots.sh` |
| `ready-unsupported-message` | `scripts/macos-accessibility-smoke.sh` |
| `ready-update-changelog` | `scripts/macos-updater-smoke.sh` |
| `ready-update-failure` | `scripts/macos-updater-smoke.sh` |
| `ready-update-install` | `scripts/macos-updater-smoke.sh` |
| `ready-video` | `scripts/capture-connect-screenshots.sh` |
| `ready-video-note` | `scripts/capture-connect-screenshots.sh` |
| `ready-video-note-send` | `scripts/capture-connect-screenshots.sh` |
| `ready-video-pip` | `scripts/macos-pip-smoke.sh` |
| `ready-video-send` | `scripts/capture-connect-screenshots.sh` |
| `ready-voice` | `scripts/capture-connect-screenshots.sh` |
| `wait-code` | `scripts/capture-connect-screenshots.sh`, `scripts/macos-settings-accessibility-smoke.sh` |
| `wait-password` | `scripts/capture-connect-screenshots.sh`, `scripts/macos-navigation-smoke.sh`, `scripts/macos-settings-accessibility-smoke.sh` |
| `wait-phone` | `docs/build.md`, `docs/credentials.md`, `scripts/capture-connect-screenshots.sh`, `scripts/macos-settings-accessibility-smoke.sh` |
| `wait-premium` | `scripts/macos-settings-accessibility-smoke.sh` |
