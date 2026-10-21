# The QR code box

Parity item `profile-qr-code-box`, PR 1 of 2. PR 2 adds the upgraded-gift themes and saves the chosen colors and emoji (`profile-share-theme`).

## What tdesktop does

- `ui/boxes/peer_qr_box.cpp` (`FillPeerQrBox`, `relayout`, `ComputeCardLayout`, `ComputeOpenLayout`): a 390-point box with a white card in the middle. The card holds the code with the plane or the avatar in the middle and the `@USERNAME` under it. Close sits at the top right. Copy, the Colors and Emoji island, and Share sit along the bottom. Your own box has a free-message pill between the card and the buttons. Room for the pill is kept in every peer box.
- `ui/effects/peer_qr_surface.cpp`, `peer_qr_emoji_field.cpp`: a radial backdrop from the chosen colors, and the picked emoji tiled behind the card with a falloff toward the edges. The tiles drift, bob, tilt and pop in when the picks change.
- The panels grow out of the island in 420 ms and shrink back in 360 ms, eased `cubic-bezier(0.16, 1, 0.3, 1)`. The card shrinks above the panel, and the panel's content fades in after 60 ms over 220 ms. A press outside or Esc closes the panel. Esc does not close the box while a panel is open.
- The colors and emoji only show when the box can be saved (`ShareThemes::CanSave`). That means your own box, or a public group or channel whose info you may change.
- `PeerQrFreeController`: while messages to you cost Stars, the pill offers a free code (`getUserLink` with a temporary or permanent token). Its menu resets the code, toggles the timer and restores the fee. Toasts last 2.8 s.
- Callers: the settings cover and the info top bar (your own box), the profile username, phone and link rows, the proxy box (`connection_box.cpp`), invite links (`edit_peer_invite_link*.cpp`), business chat links (`settings_chat_links.cpp`), folder links and the channel privacy box. The `settingsSectionQrCode` link opens your own box.

## What Quill does

- `quill::peer_qr` owns the state: `PeerQrBox::apply(action, env) -> effects` for the panels, colors, emoji and the free-code flow, plus the layout, colors, pattern and link math as pure functions with unit tests. `ui/peer_qr` keeps the open box, runs the effects and draws the box from that state and the session each frame.
- Entry points:
  - your own box from the Account settings page ("Get QR Code", one row) and from `settingsSectionQrCode`;
  - the QR button on the profile username row, or on your own phone row when you have no username;
  - the QR button on a public group's or channel's username;
  - the proxy list's QR button, an invite link's "Get QR Code", and a business chat link's "Get QR Code".
  The old QR spots (the proxy QR dialog, the inline invite-link QR, the business link QR page) are gone.
- Copy puts the card image on the clipboard, like tdesktop. Share opens Quill's share-to-chat chooser with the link. The card image is attached in the chosen chat's composer, unsent. While the chooser is up the box hides under it. A cancelled share brings the box back unchanged.
- The image is one SVG: the radial gradient, the pattern, the card shadow, the card, the code and the name. It is rendered at four pixels per point off the UI thread and encoded as PNG.

## Deviations

- **The mark in the middle.** The code carries the Quill mark on an ink disc. It does not carry Telegram's plane or the peer's avatar. Error correction is M, so the disc (17.5 % of the side) stays well inside what the code can lose.
- **No rotation in the pattern.** GPUI cannot rotate an image. The tiles keep the drift, bob, falloff, entry pop and the swap's shrink and pop. They lose the tilt and sway, dragging, the glide, the bloom from the code, the dust, the wave and the plane fly-in. With reduced motion or power saving the pattern holds its first frame.
- **Backdrop.** GPUI has no radial gradient, so Quill draws one image per color pair at half size in Rust (off the UI thread) and scales it up. It follows the card while a panel opens. A new color fades in over 220 ms. The backdrop is clipped to the box's rounded corners when it is painted.
- **Colors and emoji are a preview.** Picking a color or emoji changes this open box only. Saving them as your profile's share theme is PR 2.
- **The emoji panel** lists Quill's Unicode emoji catalog, eight to a row, in a virtual list. tdesktop uses its emoji list with sections.
- **Pattern emoji.** The pattern uses the emoji set's picture when a set is chosen. Otherwise Quill draws the system emoji once into a 128-pixel still through the SVG renderer.
- **The saved name.** The image's name lines use the UI font family when the SVG renderer can find it, else a sans-serif list. Each line is stretched to the width the box measured, so it fits the card whatever font the renderer finds.
- **Toasts** show inside the box above the buttons. tdesktop shows them over the box's layer.
- **The info top bar** has no Quill counterpart. Your own box opens from the Account settings row instead.
- **The proxy box** always uses the new box. Share is hidden without a session, as tdesktop does.
- **Not migrated:** the folder-link QR (Quill has none) and the channel privacy box's QR (`edit_privacy_box.cpp`).
- **Glass** on the buttons and the panels is a translucent fill of the computed glass color, with no blur. Pattern tiles under the buttons or an open panel are left out, so the icons and the panel's content stay readable. tdesktop draws them under its blurred glass.

## Not verified

- The free code against a real server. The demo has no connection: the pill and its menu are shown with an injected code, and the reducer's request and answer paths are covered by unit tests.
- Clipboard images on Linux and Windows. On macOS, GPUI writes only the strings of an item with more than one entry, so copy writes the image alone (no link next to it).

## Demo

`ready-peer-qr` opens your own box. `QUILL_DEMO_PEER_QR_*` flags pick the subject, a color, emoji, a panel, a message price and a free code (see `src/ui/peer_qr/demo.rs`). The UI tests in `src/ui/peer_qr/tests.rs` drive the same demo.
