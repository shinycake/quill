# Read time, privacy and wallpaper Premium upsells

Parity ids: `premium-readtime-box`, `premium-privacy-upsells`.

## What tdesktop does

- Read time (`history_view_context_menu.cpp` `AddWhoReactedAction`,
  `who_reacted_context_action.cpp` `WhenAction`): in a private chat, when
  your own privacy hides the read time (`WhoReadState::MyHidden`), the
  menu's read row gets a "show when" button. It opens
  `Ui::ShowOrPremiumBox(ShowOrPremium::ReadTime)`: "Show My Read Time"
  (`updateHideReadTime(false)` and a toast) or "Subscribe to Telegram
  Premium" (`ShowPremium("revtime_hidden")`). When the other person hides
  it, the row only says so.
- Last seen (`settings_privacy_controllers.cpp`
  `LastSeenPrivacyController`): while last seen is hidden from someone,
  the editor shows "Hide read time" and, for a free account, "Subscribe to
  Telegram Premium" with its note (`ShowPremium("lastseen")`).
  `confirmSave` shows a one-time warning before a free account saves a
  restricting rule, then sets the app setting `lastSeenWarningSeen`.
- Voice messages (`VoicesPrivacyController`): for a free account the
  restricting choices carry a lock and show a toast whose "Telegram
  Premium" link opens `ShowPremium("voice_restrictions_require_premium")`;
  under the list sits a Subscribe button with its note.
- Wallpapers (`background_preview_box.cpp` `applyForPeer`): for a user
  chat where Premium is sold, applying shows "Apply for me", "Apply for me
  and {user}" (locked for a free account, opening
  `ShowPremiumPreviewBox(PremiumFeature::Wallpapers)`) and Cancel.

## What changed

- `profile_cover/show_or_premium_box.rs` (was `last_seen_box.rs`) builds
  both kinds of the box from one skin table; `DialogKind::ShowOrPremium`
  replaces `ShowLastSeen`. The last seen box keeps its `lastseen_hidden`
  ref.
- `message_menu_ui/read_date_row.rs`: the read row with "show when" for
  `messageReadDateMyPrivacyRestricted`.
- `privacy/premium_rows.rs`: the two Subscribe rows, the voice lock toast,
  `save_privacy_rule` (every rule save goes through it) and the
  `LastSeenWarning` dialog. `quill::privacy::prefs` keeps the seen flag in
  `privacy_hints.json` under the app data root, app-wide like tdesktop.
- The voice radio rows show a lock icon for a free account; the old muted
  "needs Telegram Premium" notes are gone.
- `chat_look_ui/apply.rs`: the chat look footer offers the two Apply
  buttons when a new wallpaper is picked for a private chat; the "Also set
  for" checkbox and the "for you only" note are gone.

## Different from tdesktop

- The read row keeps Quill's "Seen time hidden" text instead of
  tdesktop's bare "read", with tdesktop's "show when" button beside it.
- The read time box uses Quill's icon circle (double check) instead of
  the `show_or_premium_readtime` Lottie, like the existing last seen box.
- Quill saves privacy rules on each click, so the warning comes before the
  click's save instead of before the box's Save.
- tdesktop draws the wallpaper buttons over a blurred preview; Quill puts
  them in the chat look dialog's footer. A locked "for both" closes the
  chat look before the preview opens, because the chat look dialog
  outranks the Premium dialog in Quill's single-dialog shell.
- The "Show my …" results are a status note, like the existing last seen
  box, not a toast.

## Verification

- Unit tests: the warning decision (`last_seen_warning_due`) and the seen
  flag.
- GPUI tests: the read row's "show when" opens the box, Show My Read Time
  shows read dates again, Premium sends `revtime_hidden`, their hidden
  read time has no button; the last seen box's Premium sends
  `lastseen_hidden`; the last seen and voice Subscribe rows and their
  refs; the warning on first restrict (Cancel keeps the rule, Continue
  saves it and is not asked again); the voice lock; the wallpaper Apply
  for me, locked for both (BackgroundForBoth box), Premium for both, and
  a theme-only change keeping one Apply.
- `/verify` light and dark: `ready-privacy-premium`
  (`QUILL_DEMO_PRIVACY=lastseen|warning|voices`),
  `ready-chat-look-wallpaper` (`QUILL_DEMO_PREMIUM=0|1`) with the locked
  click, `ready-origin-menus` with `QUILL_DEMO_ORIGIN=read-hidden` and the
  "show when" click.
