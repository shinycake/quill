# Profile share themes in the QR box

Parity item `profile-share-theme`, PR 2 of the QR box (PR 1 is `profile-qr-code-box`, docs/decisions/peer-qr-box.md). It saves the colors and emoji picked in the box as the peer's share theme, adds upgraded-gift themes and the gifts panel, and shows each peer's saved theme when their box opens. TDLib 1.8.69: `userFullInfo.share_theme`, `supergroupFullInfo.share_theme`, `getProfileShareThemeBackdrops`, `setProfileShareTheme`. tdesktop: `api_share_themes.cpp`, `info_profile_share_theme.cpp`, `info_profile_qr_gifts.cpp`, `peer_qr_box.cpp`, `peer_qr_gifts_panel.cpp`, `peer_qr_gift_field.cpp`.

## What it does

- **Saved themes.** `share_theme` is parsed from both full infos and their updates (`src/peer_qr/theme.rs`): null, `profileShareThemeEmoji` (up to six distinct emoji, custom ones by id, and a backdrop unless `use_default_background`), or `profileShareThemeUpgradedGift`. An emoji theme with no emoji on the default background reads as the default, as in tdesktop `Parse`. The session keeps one theme per user and per supergroup (`PeerQrData::themes`).
- **Opening a box** asks for the peer's full info when it is not known yet, and for the backdrops. The box shows the saved theme until something is picked (tdesktop `chosen`). A saved backdrop whose colors are the default ones shows as the default circle, so picking that circle again saves nothing.
- **Who can change it** follows tdesktop `ShareThemes::CanSave`: your own box, or a group or channel that has a username where you are the creator or an admin with `can_change_info`. Anyone else's box shows their theme, with no Colors, Emoji or Gifts buttons. A box that loses the right on a later step closes its panel and goes back to the saved theme (tdesktop `RevertToServed`).
- **Saving** works like tdesktop `SaveAppearance`. A pick only previews. When a panel closes or the box closes, the pick is compared with the theme being saved, or else the saved one, and `setProfileShareTheme` is sent only if they differ. When it succeeds, the session keeps what was sent (tdesktop `_kept`). When it fails, the box shows the classified error text as a toast and keeps the pick on screen. `chat_id` is your own user id (your private chat) or the supergroup's chat.
- **Backdrops** are asked for when a box opens, again an hour after the last request, and right away after a failure. The old list stays up while a refresh is on its way (tdesktop `kRefreshEach`).
- **The gifts panel** is the island's third button. It lists the upgraded gifts the peer owns with `getReceivedGifts`: no unlimited, upgradable or non-upgradable regular gifts, no hosted ones, 50 to a page. Burned gifts are skipped and repeats are dropped by gift id. Opening the panel asks for the first page again. In a long list, the next page is asked for once the last row scrolls into view, one page at a time. Layout follows tdesktop `ChooseLayout`: one or two wide tiles (1.28 squares tall), a row of three, or a scrolling grid. The panel opens only as tall as one, two or three tiles or the empty state need, capped at the colors panel's height. A tile is its backdrop's radial gradient with rounded corners, the model at 82 % of its height, and a white ring when it is the shown gift. Picking a gift clears the emoji. Picking an emoji while a gift shows moves to the default backdrop.
- **The empty state** shows "No gifts yet", the about line and, in your own box or a channel you may send gifts to, "Browse Gifts", which opens Quill's send-gift dialog for yourself or the channel (tdesktop `BrowseShownValue`). A group gets no Browse button.
- **The gift pattern** (`src/peer_qr/gift_pattern.rs`, from tdesktop `EnumerateModels` and `EnumerateSymbols`) draws the model on a wide staggered grid and the symbol, tinted in the backdrop's pattern color at 37 %, on a finer grid kept clear of the models. Both drift like the emoji pattern. The symbols fade in with a 300 ms stagger and the models pop in with a 200 ms stagger, each layer starting once its still is made. With reduced motion the pattern shows settled. Copy and share draw the same settled pattern into the image.
- **All of it goes through the reducer.** `PeerQrBox::apply` takes `PickGift`, `MoreGifts`, `Served`, `SaveAnswer` and `Closing`, and returns `SaveTheme` and `LoadGifts` effects. The UI runs those effects and reads the session and the box each frame. Stills are cached, never state.

## Deviations

- **No model tilt.** GPUI cannot rotate images, so models are drawn upright (the same limit as the emoji pattern). tdesktop's gift leaving animation (`_giftLeaving`) is not drawn: a new gift's pattern comes in over nothing.
- **Symbol fades in steps.** GPUI paints images at full opacity, so the symbols are grouped into twelve opacity steps, each its own layer with an opacity. Once settled they are one layer at 37 %.
- **Tiles show no symbol pattern.** tdesktop paints the symbol across each tile's background (`PaintPattern`). Quill's tiles have the gradient and the model only.
- **The empty state's sticker** is the kit's gift icon at 48 points in the 80-point slot, in place of tdesktop's animated dog. Its label heights are fixed at 20 and 36 points, since GPUI does not measure them before layout.
- **Model and symbol are first-frame stills,** not animated (TGS through rlottie, other formats through `image`, video stickers through their thumbnail). tdesktop plays the model.
- **Custom emoji** in a saved theme are kept, so saving the theme again sends them back, but the pattern draws only plain emoji.
- **Panel height jumps** when the gift list goes from loading to its final layout while the panel is open. tdesktop animates it.
- **Save errors after the box closed** are dropped. The session still keeps the saved theme when it succeeds.

## Memory and stack

There is no performance change to measure: the new work runs only while the box is open. Struct sizes, measured with `size_of` in the debug `demo-capture` test build:

| | main (6898573d5) | this PR |
|---|---|---|
| `QuillApp` | 41,016 B | 41,024 B |
| `Session` | 3,592 B | 3,792 B |
| `PeerQrUi` | 432 B | 240 B |

`PeerQrUi::open` is now `Option<Box<OpenQr>>` (`OpenQr` is 400 B), which pays for most of the new fields in `QuillApp`. `Session` grows by 200 B because `PeerQrData` sits inline in `UsersState` and now holds the themes map, the gifts list and the save queues (304 B). Their contents live on the heap. Both types stay well inside the bounds in `src/ui/stack_budget_tests.rs` (48 KB and 8 KB).

## Not verified

- Against a real server: the requests and answers are covered by recorded-JSON driver tests (`src/connect/tests/share_theme.rs`), and the box by reducer and UI tests. The demo saves at once.
- Real gift stickers: the demo uses two small generated TGS fixtures (`docs/screenshots/fixtures/demo-gift-model.tgs`, `demo-gift-symbol.tgs`).
- An admin's `can_change_info` right is known once `getChatMember(me)` has answered (as for the peer color dialog). A channel's box opened before that has no styling buttons until it does.

## Demo

`ready-peer-qr` takes `QUILL_DEMO_PEER_QR_PANEL=gifts`, `QUILL_DEMO_PEER_QR_GIFTS=<n>`, `QUILL_DEMO_PEER_QR_GIFT=<n>` and `QUILL_DEMO_PEER_QR_THEME=emoji|gift` (see `src/ui/peer_qr/demo.rs`). The UI tests are in `src/ui/peer_qr/tests/theme.rs`.
