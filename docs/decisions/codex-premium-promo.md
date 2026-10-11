# Premium promo page and limit upsell boxes

Items: `parity:premium-promo-page`, `parity:premium-limit-upsell`.

## What tdesktop does

- `settings/settings_premium.cpp`: a header with the star, the feature list
  from the server, a "Doubled limits" row that opens a page of free versus
  Premium limits, and a subscribe button that names the monthly price.
  Subscribing goes through the store or the Premium bot.
- `boxes/premium_limits_box.cpp` (`SimpleLimitBox`): a line from the free to
  the Premium value, one or two sentences (`lng_*_limit1/2`), and
  "Increase Limit" next to "Cancel"; an OK button for Premium accounts or
  when Premium is not sold. Used for pins, joined communities, public
  links, caption length, saved GIFs and file size. Folder limits already
  had their own box here.

## What changed

- `premium_limits.rs`: the limit kinds, fallback values, message
  classification (`CHANNELS_TOO_MUCH`, pinned-chat errors, caption too long,
  public-link errors), box text and the upload size check.
- `premium_hub/promo.rs`: plans with prices from `getPremiumState`, the
  subscribe label ("Subscribe for X per month"), the limit lookup from
  `getPremiumFeatures`, and the "Doubled limits" lines.
- `TdError.premium_limit` keeps only the classification. Failed pin, join,
  username and send requests open the box instead of an error line
  (`state/domains/payments/limit.rs`, one call in each domain's error
  handler). `is_premium_available` is now read, so Premium is not offered
  where it is not sold.
- UI (`ui/premium_ui/limit_box.rs`, `promo.rs`): the limit box and the
  rebuilt Premium dialog with the limits page. Hooks: the pin pre-check,
  caption-too-long on send, a file over 2 GB (4 GB with Premium) before
  sending, and "Add to GIFs" on a full list.
- Subscribing still opens @PremiumBot; Quill takes no payment
  (`premium-in-app-purchase` stays blocked). `viewPremiumFeature` and
  `clickPremiumSubscriptionButton` are not called, and neither does tdesktop.

## Follow-up round

- Folder limits (`FiltersLimitBox`, `FilterChatsLimitBox`, `FilterLinksLimitBox`)
  were already hooked at their sites (new folder, folder editor save,
  invite links, the chat pickers) with their own box in
  `ui/folder_extras.rs`. Its button now says "Increase Limit" and opens the
  Premium page like the other boxes ("Get Premium" for the tag notice).
- `joinChatByInviteLink` failing with `CHANNELS_TOO_MUCH` opens the
  communities box (`state/domains/chats/error.rs`).
- File size: TDLib 1.8.68 has no upload-size option and `getPremiumFeatures`
  has no file-size limit type, so the 2000 MiB / 4000 MiB values stay as
  constants (tdesktop reads them from app config, which TDLib does not
  expose).
- Feature previews: tapping a feature opens a preview page in the Premium
  dialog (not a separate box) with its `premiumFeaturePromotionAnimation`
  clip played muted and looped on `NativeVideo`, the text, dots and
  Previous / Next paging, and the subscribe button. The clip is downloaded
  when the page opens. "Premium stickers" shows example stickers from
  `getPremiumStickerExamples` instead of a video, as tdesktop does. The
  player lives in `ui/premium_preview_state.rs`. Not done: the swipe pager
  and the per-feature demo layouts of tdesktop (chat mock-ups, reaction
  effects); the clip and the first frames fall back to a gradient with a
  star until downloaded.

## Not done

Swipe paging and tdesktop's feature-specific preview layouts (see above).
The GIF box appears when the loaded saved list was already full, not from a
server error, because TDLib replaces the oldest GIF silently.

## Verified

Unit tests for classification, text, plans, limit lookup, size checks and
the reducer routing. Visual check with a temporary demo (not committed):
promo page, "Doubled limits" page and the caption limit box rendered as
expected.
