# Misc bundle: system window frame, bot permissions, poll option media

Three items from the parity list, each checked against tdesktop and TDLib 1.8.68 first.

## Use system window frame (`data-native-frame`)

tdesktop: Settings > Advanced has a "Use system window frame" switch where `Ui::Platform::NativeWindowFrameSupported()` is true (Linux and Windows; the label reads "Use Qt window frame" on Wayland). It is off by default and applies at once (`nativeWindowFrameChanges` refreshes the title widget and the size limits). macOS has no switch.

Quill before: Windows hid the system title bar for good (`appears_transparent`). Linux asked for no decorations at all, so GPUI used server-side ones and the kit `TitleBar` drew a second bar with no buttons under the window manager's.

Now:
- `native_window_frame` in `appearance_prefs.json`, off by default. The switch sits in General next to the tray switches and only exists on Linux and Windows.
- `src/ui/window_frame.rs`: Linux asks for client-side decorations at creation (Quill's own frame, which is what the kit title bar is built for) and switches to server-side ones live with `Window::request_decorations`. Windows gets a new vendored function, `gpui_windows::native_frame::set_native_frame`, that flips the `hide_title_bar` flag and sends `SWP_FRAMECHANGED`. GPUI's own `request_decorations` is a no-op on Windows and is called with `Server` at every window's creation, so it cannot carry the choice.
- With the system frame, `FrameTitleBar` drops the caption and the kit's caption buttons (the OS draws both) and keeps only the action buttons in a slim row. The row is left out when a window has none (chat windows).
- Every Quill window follows: the main window and chat windows call `window_frame::sync` from their render, which acts once per change per window.
- Wayland: the label stays "Use system window frame". Quill is not a Qt app, and "server-side" there means the compositor's decorations, with a fallback to Quill's when the compositor has none (GPUI does that fallback).

Not verified: Linux and Windows. This machine is a Mac with no cross toolchain, so both code paths were read against GPUI's source but not built or run. The Windows module needs `llvm-rc` to compile and could not be built here.

## Bots and websites (`settings-bots-websites`)

The audit entry named three things. tdesktop has:
- "Clear Payment and Shipping Info": already in Quill.
- "Logged in with Telegram" websites list: already in Quill.
- Mini-app permissions: a "Permissions" section on a bot's profile with an "Emoji Status" switch, shown when the bot has `canManageEmojiStatus` (`DetailsFiller::makeBotPermissions`, `bots.toggleUserEmojiStatusPermission`). Missing from Quill, and so was the mini-app side (`web_app_request_emoji_status_access`, `web_app_set_emoji_status`), which Quill answered with "cancelled" or "unsupported".
- Delete cloud drafts: tdesktop has no such action. `messages.clearAllDrafts` appears only in its API scheme. Nothing was added, to stay with tdesktop. TDLib has `clearAllDraftMessages` if the owner wants it as a Quill-only extra.

Now:
- `BotInfo.can_manage_emoji_status` is parsed; the bot profile shows "Permissions > Emoji Status" for a bot that has it. The switch calls `toggleBotCanManageEmojiStatus`, moves at once like tdesktop's local flag and goes back if the call fails. Like tdesktop, the section stays on screen after the access is turned off, so it can be given again.
- `web_app_request_emoji_status_access`: allowed straight away when the bot has it; otherwise a prompt ("{bot} requests access to set your emoji status. You will be able to revoke this access in the profile page of {bot}.", Allow/Cancel). Allow needs Premium: without it the Premium page opens and the prompt comes back (tdesktop's box stays behind its Premium preview), otherwise it calls `toggleBotCanManageEmojiStatus` and answers `allowed` or `cancelled`.
- `web_app_set_emoji_status`: validated like `Panel::processEmojiStatusRequest` (no arguments closes the app, a bad id answers `SUGGESTED_EMOJI_INVALID`, a negative duration `DURATION_INVALID`). The custom emoji is resolved with `getCustomEmojiStickers`, must be an emoji-set sticker, and a "Set Emoji Status" prompt follows. Confirm (Premium) sets it with an expiry when a duration was given and answers `emoji_status_set` as soon as the request is sent, as tdesktop does; anything else answers `USER_DECLINED`.

Differences that remain:
- The prompts are the mini-app helper's native popups, which carry text only. tdesktop paints the status emoji and the bot's avatar in its boxes; Quill names the emoji by its character in the sentence.

## Links and media in poll options (`polls-option-media`)

tdesktop (`history_view_poll.cpp`, `create_poll_box.cpp`, `history_view_add_poll_option.cpp`, `poll/*`): each option may carry media, and the text may hold custom emoji. TDLib 1.8.68 allows `pollMediaAnimation`, `pollMediaLink`, `pollMediaLocation`, `pollMediaPhoto`, `pollMediaSticker`, `pollMediaVenue` and `pollMediaVideo` there. tdesktop shows a rounded square at the end of the option (twice the radio's diameter): the photo, a clip's first frame with a play badge, the sticker, the link's picture or a link glyph, or a map tile with a pin. A click opens the photo or document viewer, a sticker preview, a map preview, or "Open Link" for a link. While creating, each option has an attach button with Photo or video, Sticker, Link (and Location when its map picker exists) and Remove. The same button is on the "Add an option" field.

Now:
- Parsing: `PollOption` keeps `entities` and `media` (`PollOptionMedia` in `poll_option_media.rs`), with the files they name, in messages and in `updatePoll`.
- Showing: option rows draw the square (`ui/polls/option_thumb.rs`), download its picture with the other visible-message thumbnails, and request a map tile for places through the existing `getMapThumbnailFile` path. Custom emoji in option text are resolved and drawn like those in messages.
- Clicking: photos, clips and GIFs open in the media viewer as a new `ViewerSource::Poll` (one item, no message actions, no delete). A link always asks first, with the address shown and look-alike letters marked. A place opens the map, as Quill's location rows do. A sticker opens its pack.
- Creating: per-row attach button with Photo or video (file picker, read off the UI thread; photos over 10 MB are refused as TDLib says; videos are probed with ffprobe), Sticker (the normal sticker panel; the next sticker clicked goes to the option, not the chat), Link and Remove, on new polls and on the "Add an Option" field. The request builds `inputPollMediaPhoto`, `inputPollMediaVideo`, `inputPollMediaSticker` or `inputPollMediaLink`.

Differences that remain:
- Option text is typed in a plain field, so custom emoji cannot be inserted while creating an option (tdesktop allows it for Premium). Showing them works.
- No Location entry in the attach menu (Quill has no map picker, and tdesktop hides the entry when its picker is unavailable). Places received from others show.
- A place opens the browser's map, as Quill's location messages do, where tdesktop opens a preview box. The link box has no webpage preview card and adds a "Copy Link" button (Quill's shared open-link box).
- A sticker click opens the sticker pack dialog; tdesktop shows a large preview with a menu.
- The picture sits after the percentage at the row's end; tdesktop puts the percentage on the left and the picture on the right. The square is the same size.
- A picked video shows its file name and length in the dialog, not a frame. A photo shows the picture.
- No drag and drop of media onto an option field.
- Poll question, description and quiz-explanation media are separate features and not part of this item.

## How it was checked

- Unit tests: bridge parsing, request JSON, poll option media parsing, window-frame state, driver tests with the recording sender (emoji status access, option media), UI tests for the poll dialog attach flows, the bot profile switch, the mini-app prompts (with `cat` standing in for the web view helper), clicks on option media, the system-frame title bar, and the settings switch.
- A temporary screenshot demo (not committed) with a photo, a video, a link and a place on poll options: all four squares draw next to the text and the percentage.
- Not verified: Linux and Windows window frames (see above), any live Telegram account, the Premium path of the emoji status prompts against a real server, custom emoji in option text from a real poll.
