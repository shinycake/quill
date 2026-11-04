# Telegram links open in Quill

## What was wrong

A `t.me` link in a message, and the button under its link preview ("View
stickers", "View channel" and the rest), went straight to the browser. The
typed deep-link path (`getInternalLinkType` → `deep_link_types::route`) only
ran for links from the OS, a second launch, mentions and Mini Apps. So
`https://t.me/addemoji/CandyFontEmoji` opened a web page instead of the
emoji pack box that Quill already has.

## What Telegram Desktop does

`HiddenUrlClickHandler::Open` / `UrlClickHandler::Open` run every URL
through `Core::TryConvertUrlToLocal` first. Anything on `t.me`,
`telegram.me`, `telegram.dog` or `<username>.t.me` becomes a `tg://` URL
and is handled by `openLocalUrl`. A hidden `t.me` link gets no "Open this
link?" box, because it never leaves the app. Only links it can't convert
(`t.me/` itself, `t.me/iv` it leaves to the IV code) reach the browser.

## What Quill does now

- `QuillApp::open_link` is the single door for links the user presses.
  When signed in, a Telegram link (`quill::deep_link_types::is_telegram_link`)
  is queued on the deep-link pump instead of being handed to the OS. Every
  caller (message text, link preview card, inline URL buttons, the confirm
  box, bot menus) gets this.
- A message link to a Telegram host skips the hidden-link confirmation, like
  tdesktop.
- Text links to `tg:` addresses are clickable now (`TextEntity::tg_href`).
  Before, they had no href at all.
- The link preview card checks for a Telegram link before trying Instant
  View, so the preview's button runs the page's own link.
- `internalLinkTypeInstantView` opens the IV reader, or its fallback URL in
  the browser.
- When TDLib can't type a web link (an error from `getInternalLinkType`), or
  types it as something Quill has no screen for, the web page opens in the
  browser (`DeepLinkUi::Browser`). Before, the user got "This link isn't
  supported by Quill yet." A `tg:` link that can't be handled still gets
  that message, because it has no web page. The browser fallback calls
  `open_web_link` directly, so it can't loop back into the pump.
- The sticker set box labels an emoji pack's button "Add N Emoji" /
  "Remove Emoji" (tdesktop `lng_stickers_add_emoji`,
  `lng_custom_emoji_remove_pack_button`).

## Link types audit (TDLib 1.8.69, 59 `internalLinkType*`)

Handled before this change: 37 types, including sticker and emoji
sets, chats, bots, Mini Apps, messages, stories, invites, folders, proxies,
backgrounds, language packs, invoices, gift codes, gift
auctions, boosts, group calls, settings pages, the phone and Saved Messages
links. TON wallet links show the official-apps notice.

Added here: `InstantView`.

Still without a Quill screen, so they open their web page (or say so for
`tg:`), while tdesktop handles them itself:

| Type | tdesktop | Needs in Quill |
|---|---|---|
| `BusinessChat` (`t.me/m/<slug>`) | opens the chat with the draft | `getBusinessChatLinkInfo` |
| `ChatAffiliateProgram` | opens the bot with the referrer | `searchChatAffiliateProgram` |
| `DirectMessagesChat` (`?direct`) | opens the channel's direct messages | resolve, then the DM chat |
| `LiveStory` | plays the live story | resolve, then the live story |
| `StoryAlbum` (`?album=`) | opens the profile's story album | resolve, then the album |
| `GiftCollection` | opens the profile's gift collection | resolve, then the collection |
| `Theme` (`t.me/addtheme/<slug>`) | the theme preview | a cloud theme preview (today a message) |
| `UpgradedGift` (`t.me/nft/<slug>`) | the unique gift box | `getUpgradedGift` |
| `UserToken` (`t.me/contact/<token>`) | opens the contact's chat | `searchUserByToken` |
| `StarPurchase` | the Stars purchase box | Star packs box |
| `PremiumGiftPurchase` | the gift Premium box | gift Premium box |
| `NewPrivateChat`, `ChatSelection`, `Search`, `NewStory` | the matching screen | navigation |
| `PhoneNumberConfirmation` | confirm-phone box | `sendPhoneNumberCode` flow |
| `PassportDataRequest`, `Oauth`, `RestorePurchases` | passport form, OAuth box, mobile only | no Quill feature |
| `QrCodeAuthentication` | — | a message: it's for another device |

These are a follow-up.
