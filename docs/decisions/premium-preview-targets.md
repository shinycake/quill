# Premium upsells open their own feature

Parity ids: `premium-preview-targets`, `premium-business-preview`.

## What tdesktop does

tdesktop has three ways into Premium, and each upsell picks one:

- `ShowPremiumPreviewBox(feature)` (`boxes/premium_preview_box.cpp`): the
  preview of one feature, paging through the list that holds it, with a
  single button. The button is "Unlock Premium Reactions / Stickers /
  Animated Emoji / Emoji Status" for those four features and "About
  Telegram Premium" for the rest. It opens the Premium page with the
  feature's ref (`Settings::ShowPremium(LookupPremiumRef(feature))`).
- `ShowPremiumPreviewToBuy(feature)`: the same preview with the plans and
  the subscribe button, or Close for a Premium account. Business rows,
  folder tags, chat links, quick replies and chat translation use it.
- `Settings::ShowPremium(ref)`: the Premium page itself, with a `/start` ref
  for the bot (`require_premium`, `lastseen_hidden`,
  `noncontact_peers_require_premium`, `channel_boost__<id>`, `ai_compose`,
  and `double_limits__<limit>` from every limit box).

A preview pages through the Business list when that list holds the feature
(`getBusinessFeatures`, `business_animations`), else through the Premium
list, else shows the feature alone. Gifts get their own explainer: three
rows (unique, tradable, wearable) and "Understood".

## What changed

- `premium_hub/feature.rs`: typed `PremiumFeature` (every
  `premiumFeature*` constructor, with title, text and `/start` ref) and
  `BusinessFeature` (`businessFeature*`, tdesktop's fallback order).
  `PremiumInfo.features` is now `Vec<PremiumFeature>`; the hand-matched
  title strings and `feature_ref` are gone.
- `premium_hub/promo.rs`: `PromoFeature` (Premium, Business or Gifts),
  `PreviewOpen` (Overview, ToBuy, Box), `PromoPage::Preview { feature,
  opened }`, `preview_order`, and clips keyed by feature from
  `animations` and `business_animations`.
- `getBusinessFeatures` request, envelope and reducer; the Business hub
  lists the server's order (tdesktop's fallback until it arrives) and gains
  the "Tag Your Chats" row.
- UI: `open_premium_at` (box), `open_premium_to_buy`, `open_premium_from`
  (ref). The preview moved to `premium_ui/preview.rs`. Every call site was
  checked against tdesktop:
  - box: emoji status (main menu, status panel, status time box, web app
    status requests), name colors, Gifts (auction box), Business intro,
    Disable Sharing, message effects, AI compose, reactions and saved tags,
    stickers, No ads (Hide ads, the report result, the About These Ads
    link), voice to text (locked transcribe button and the trials toast),
    checklists (the create/add/mark toasts), Stealth Mode (upgraded
    stories).
  - to buy: Business rows for a free account, folder tags (always), chat
    links, quick replies, chat translation switch.
  - ref: `require_premium`, `lastseen_hidden`,
    `noncontact_peers_require_premium`, `channel_boost__<supergroup id>`,
    `ai_compose`, `double_limits__*` from the folder and Premium limit
    boxes.
- Saved Messages tags bar: a free account sees "Unlock" first and every tag
  opens the tags preview (`SearchTags`).
- A refused "Hide ads" or a `reportSponsoredResultPremiumRequired` now asks
  for the No ads preview (`hub.promo_request`, opened by the frame upkeep)
  instead of a "needs Premium" banner.

## Skipped or different

- tdesktop's caption, folder title and contact name custom-emoji gates have
  nothing to hook to: Quill has no custom emoji input in those fields.
- The free-account "Add tag" chip when there are no tags yet: Quill hides
  the tags bar when there are none.
- Stories and Business list boxes (`UpgradedStoriesPreviewBox`,
  `TelegramBusinessPreviewBox`) are not built; those features open the
  ordinary preview.
- Left/Right keys for paging and closing a to-buy preview when Premium
  turns on are not built.

## Verification

- Unit tests: feature round trips and refs, Business list parsing, preview
  order and paging, the limits page, the transcribe lock, the reducer for
  `businessFeatures`, sponsored hide without Premium.
- GPUI tests (`premium_ui/preview_tests.rs`, business and reaction tests):
  the box pages and unlocks to the Premium page with the feature's ref; to
  buy pages the Business list; Gifts closes with Understood; refs and the
  limits feature; the No ads request is taken once; a free Business row
  opens its preview to buy.
- `/verify` with the new `ready-premium-preview` demo
  (`QUILL_DEMO_PREMIUM=box|to-buy|gifts|business`), light and dark, and the
  click paths Unlock to the Premium page and a free Greeting row to its
  preview. The demo has no promotion clips, so real video playback in the
  preview is not shown.
