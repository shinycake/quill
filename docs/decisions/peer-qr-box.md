# The QR code box

Parity item `profile-qr-code-box`, PR 1 of 2. PR 2 adds the upgraded-gift themes and saves the chosen colors and emoji (`profile-share-theme`).

## What tdesktop does

- `ui/boxes/peer_qr_box.cpp` (`FillPeerQrBox`, `relayout`, `ComputeCardLayout`, `ComputeOpenLayout`): a 390-point box with a white card in the middle. The card holds the code with the white plane on an ink disc in the middle (`PaintCard`, never the userpic) and the `@USERNAME` under it. Close sits at the top right. Copy, the Colors and Emoji island, and Share sit along the bottom. Your own box has a free-message pill between the card and the buttons, and every peer box keeps room for it.
- `ui/effects/peer_qr_surface.cpp`, `peer_qr_emoji_field.cpp`: a radial backdrop from the chosen colors, and the picked emoji tiled behind the card with a falloff toward the edges. The tiles drift, bob, tilt and pop in when the picks change.
- The panels grow out of the island in 420 ms and shrink back in 360 ms, eased `cubic-bezier(0.16, 1, 0.3, 1)`. The card shrinks above the panel, and the panel's content fades in after 60 ms over 220 ms. A press outside or Esc closes the panel. Esc does not close the box while a panel is open.
- `peer_qr_colors_panel.cpp`: six circles to a row from the top left, 48 points with a 12-point gap. Each is a diagonal gradient from the center color to the edge color. The chosen one gets a white ring 6 points in, with a faint dark line under it on a light circle.
- The colors and emoji only show when the box can be saved (`ShareThemes::CanSave`). That means your own box, or a public group or channel whose info you may change.
- `PeerQrFreeController`: while messages to you cost Stars, the pill offers a free code (`getUserLink` with a temporary or permanent token). Its menu, a popup from the pill's top edge, resets the code, toggles the timer and restores the fee. A server error is toasted and changes nothing. An empty or expired answer is "unavailable", and after a reset the fee comes back. Answers for a dropped request are ignored (`generation`). A temporary own link drops a permanent code. Toasts last 2.8 s.
- Your own link (`OwnQrLinkState`): the username link, else the phone link while the number is visible, else a temporary `getUserLink` contact link with a countdown. Each box makes a new state, so reopening the box retries a failed link. The error is toasted.
- Copy puts the card image on the clipboard. Copy and share check after rendering that the box still shows the same link and colors (`ImageIsCurrent`), and refuse a link about to run out (`RefuseImage`).
- Callers: the settings cover and the info top bar (your own box), the profile username, phone and link rows, the proxy box (`connection_box.cpp`), invite links (`edit_peer_invite_link*.cpp`), business chat links (`settings_chat_links.cpp`), folder links and the channel privacy box. The `settingsSectionQrCode` link opens your own box.

## What Quill does

- `quill::peer_qr` owns the state: `PeerQrBox::apply(action, env) -> effects` for the panels, colors, emoji and the free-code flow, plus the layout, colors, pattern and link math as pure functions with unit tests. Free-code requests carry a ticket (the box's id and a serial bumped by every request and every dropped one), so a late answer for an earlier box or request is ignored. `ui/peer_qr` keeps the open box, runs the effects and draws the box from that state and the session each frame.
- Drawing only reads. The name is measured each frame (GPUI caches shaped lines). The pictures (backdrop, code, emoji stills) start off the UI thread when the box opens, after every action and on every poll step. The same steps ask the driver for your contact link (`ensure_own_contact_link`). It sends `getUserLink` once the phone privacy rules have come back and call for one, and never while one is on its way. Esc reaches the box as the dialog's cancel, which closes an open panel first.
- The pill menu paints at the kit's popup priority, above the box. A cover over the whole window, one priority lower, takes any press off the menu and closes only the menu.
- The once-a-second clock runs only while something counts down: a temporary free code or a temporary contact link. It stops itself when nothing does or the box closes. The pattern, the panels and the backdrop fade use the frame clock and stop when settled.
- Entry points:
  - your own box from the Account settings page ("Get QR Code", one row) and from `settingsSectionQrCode`;
  - the QR button on the profile username row, or on your own phone row when you have no username (the button's click stops there, so the row does not also copy);
  - the QR button on a public group's or channel's username;
  - the proxy list's QR button, an invite link's "Get QR Code", and a business chat link's "Get QR Code".
  The old QR spots (the proxy QR dialog, the inline invite-link QR, the business link QR page) are gone.
- Copy depends on what the platform clipboard takes. macOS gets the card image alone, as tdesktop copies it (GPUI writes only the strings of an item with more than one entry). Windows gets the image and the link. The X11 and Wayland clipboards take text only, so Linux gets the link and the "Link copied" toast instead of "QR Code copied".
- Share opens Quill's share-to-chat chooser with the link. The card image is written to a temporary file off the UI thread and attached, unsent, in the chosen chat's composer. While the chooser is up the box hides under it. A cancelled share brings the box back unchanged and removes the file. If the next chat a link opens is not the chosen one, the file is removed too. A copy or share whose image comes back after its box closed does nothing, even when a new box shows the same link.
- The image is one SVG: the radial gradient, the pattern, the card shadow, the card, the code and the name. It is rendered at four pixels per point off the UI thread and encoded as PNG.
- The color circles, the emoji cells, the pill and the menu items are buttons, so Tab reaches them and Space presses them.

## Deviations

- **The mark in the middle.** The code carries the Quill mark on an ink disc at tdesktop's 17.5 % of the side, in place of Telegram's plane. Error correction is M, so the disc stays well inside what the code can lose.
- **No rotation in the pattern.** GPUI cannot rotate an image. The tiles keep the drift, bob, falloff, entry pop and the swap's shrink and pop. They lose the tilt and sway, dragging, the glide, the bloom from the code, the dust, the wave and the plane fly-in. With reduced motion or power saving the pattern holds its first frame.
- **Tiles stay off the card and the glass.** tdesktop tucks tiles partly under the card's edge and under its blurred buttons and panels. Quill's glass has no blur, so no tile is drawn where it would touch the card, a button or an open panel. The saved image leaves out tiles touching the card the same way.
- **Backdrop.** GPUI has no radial gradient, so Quill draws one image per color pair at half size in Rust (off the UI thread) and scales it up. It follows the card while a panel opens. A new color fades in over 220 ms. The backdrop is clipped to the box's rounded corners when it is painted.
- **Saving the theme** and the upgraded-gift themes are in docs/decisions/profile-share-theme.md (PR 2).
- **The colors panel** has no ring animation: the white ring is on the chosen circle at once. The hairline under it shows when either gradient color is close to white.
- **The emoji panel** lists Quill's Unicode emoji catalog, eight to a row, in a virtual list. tdesktop uses its emoji list with sections.
- **Pattern emoji.** The pattern uses the emoji set's picture when a set is chosen. Otherwise Quill draws the system emoji once into a 128-pixel still through the SVG renderer.
- **The saved name.** The image's name lines use the UI font family when the SVG renderer can find it, else a sans-serif list. Each line is stretched to the width the box measured, so it fits the card whatever font the renderer finds.
- **Toasts** show inside the box above the buttons. tdesktop shows them over the box's layer. A failed free code or contact link toasts Quill's classified error text, never TDLib's raw message.
- **Enter** does nothing in the box. The dialog maps Enter to its confirm, which the box ignores, so a focused button is pressed with Space.
- **The info top bar** has no Quill counterpart. Your own box opens from the Account settings row instead.
- **The proxy box** always uses the new box. Share is hidden without a session, as tdesktop does.
- **Not migrated:** the folder-link QR (Quill has none) and the channel privacy box's QR (`edit_privacy_box.cpp`).

## Not verified

- The free code against a real server. The demo has no connection: the pill and its menu are shown with an injected code, and the reducer's request and answer paths are covered by unit tests.
- Clipboard contents on real Linux and Windows desktops. The per-platform choice is covered by a unit test, and the UI test checks whichever one the host builds.

## Demo

`ready-peer-qr` opens your own box. `QUILL_DEMO_PEER_QR_*` flags pick the subject, a color, emoji, a panel, a message price and a free code (see `src/ui/peer_qr/demo.rs`). The UI tests in `src/ui/peer_qr/tests.rs` drive the same demo.
