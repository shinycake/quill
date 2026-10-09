# Typed deep links through getInternalLinkType

Gap batch 13, item 23.

## What Telegram Desktop does

`core/local_url_handlers.cpp` matches the link text against a regex list
(`join`, `addlist`, `addstickers|addemoji`, `addtheme`, `msg_url`, `proxy`,
`socks`, `bg`, `resolve`, `privatepost`, `invoice`, `login`, `boost`, ...).
Each handler opens a box or page. `ResolveUsernameOrPhone` reads `start`,
`startgroup`, `post`, `comment`, `thread`/`topic`, `t` (video timestamp),
`text`, `story` and `phone` and calls `showPeerByLink`. `ShareUrl` shows a
recipient chooser and puts the text in that chat's draft; it refuses a url
that starts with `@` so a share link cannot run an inline bot query. Anything
no handler matches goes to `getDeepLinkInfo` and the answer is shown in a box
(`HandleUnknown`). `core/deep_links/*` routes `tg://settings/...` to the
matching settings page. tdesktop never opens the browser for a link it
recognises but cannot act on; a plain `t.me` URL with no handler is the only
case that ends up in the browser, and in Quill in-message URLs already do that.

## What changed

- `src/deep_link_types.rs` (pure): parses a `getInternalLinkType` answer into
  `InternalLink` and `route()` decides: a follow-up chat flow, a UI action, the
  `getDeepLinkInfo` fallback, or a "not supported by Quill yet" message.
- `connect::deep_links`: the old local fast path keeps the forms it already
  handled. A link with a parameter it drops (`comment`, `thread`, `topic`, `t`,
  `startgroup`, `text`, ...) or with no local form asks `getInternalLinkType`.
  `t.me/+<digits>` is a phone link, no longer an invite hash.
- New follow-ups: `getMessageLinkInfo` (message / comment / thread /
  timestamp links), `searchStickerSet`, `searchUserByPhoneNumber`.
- UI (`ui/deep_link_routes.rs`): sticker/emoji set opens the existing preview
  dialog with Add; proxy hands the original link to the existing proxy
  confirmation; a share or message draft opens a chat chooser, then the chat
  with the text in the composer (never sent); settings sections open the
  matching existing page; `+phone` opens the private chat (draft prefilled);
  a story link opens the viewer; a message link jumps to the message, opens
  its thread, and seeks `?t=` once the message has loaded.
- Deep links stay opt-in (Settings > default handler); unchanged.

## Not supported yet (message, no browser, as tdesktop shows a feature box)

login code (Quill is already signed in, so the message says so), invoice,
boost, gift code, voice/video chat and live stream, group call, `addlist`
folder invite, bg / theme preview, `?startgroup` / `?startchannel` (add a bot
to a group), and settings pages Quill has no screen for (the settings list
opens). Their parity ids are not claimed.

## Differences

- The share chooser lists the 14 most recent chats; no search box.
- Bot `start=` still only prefills `/start <param>`; autostart is not honoured.
- In-message `t.me` URLs still open in the browser (unchanged).

## Verified

- Unit tests for parsing and routing of every link type
  (`deep_link_types`), request shapes against the schema, and driver-level
  flows with recorded TDLib JSON (`connect/tests/deep_link_routing.rs`):
  sticker set, message/comment/thread/timestamp, hidden chat, phone,
  phone not found, proxy, share, settings, unsupported, unknown fallback.
- Demo capture of the share chooser (`--screenshot-demo ready-deep-link-share`).
- Unverified: live TDLib answers for comment links (which chat TDLib returns
  for `?comment=`), and the UI flows beyond the share dialog capture.
