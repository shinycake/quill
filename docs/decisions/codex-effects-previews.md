# Message effects playback and wallpaper/theme link cards

## Wallpaper and theme link previews (done)

tdesktop (`WebPageType::WallPaper` / `Theme`, `ThemeDocument`): the card shows
the document's preview picture under the text (wallpaper 160 px wide, height
from the preview's proportions clamped to 100..240; theme 272x176), with the
file size in a pill at the top left, and a "View" button only when the page
has a wallpaper document or a Desktop theme file (`.tdesktop-theme`,
`.tdesktop-palette`, or the Desktop theme mime type). A link with no document
draws its colors (fill or pattern) at 160x240 and has no button.

Changed: `linkPreviewTypeBackground` and `linkPreviewTypeTheme` are parsed
into `LinkPreviewKind::Background` / `Theme`
(`src/telegram/envelope/message_link_preview/docs.rs`), the button follows the
rules above, the preview files are downloaded with the chat's thumbnails, and
`src/ui/message_text/link_preview_tile.rs` draws the picture (wallpapers
through the gallery's wallpaper renderer). Clicking opens the link: `bg/`
opens Quill's wallpaper preview. `addtheme/` links cannot be applied (TDLib
has no Desktop theme API and Quill does not use Desktop theme files), so the
existing notice explains that, as before.

Verified: unit tests for parsing, button rules, size text and card boxes.
Not visually checked in a demo capture.

## Message effects (done)

tdesktop: a message with `effect_id` shows a small icon at the left of its
footer; the effect (2x big-emoji size, 224 px) plays centred on that icon,
shifted a third of its width toward the chat middle, mirrored for incoming
messages, when the badge first shows for an unread incoming message or one
just sent, unless effects are off in power saving (the message is marked as
played regardless), and on every click of the badge (power saving ignored).
The send menu (private chats only, with something to send) offers an effect
picker and "Send with Effect".

Done: `effect_id` on messages, `getMessageEffect`,
`updateAvailableMessageEffects`, `messageSendOptions.effect_id` (text, media,
albums), the footer badge (icon, or emoji when the effect has no icon), click
replay, first-show playback with the power-saving rule and window-active
rule, following a message across its temporary-to-final id change, and unit
tests of the core parsing and request JSON.

The send-menu picker, the "Send with Effect" card, the driver and UI
tests and the visual check followed in
`docs/decisions/message-effects-picker.md`.

Only `parity:render-theme-wallpaper-docs` is in the parity fragment.
