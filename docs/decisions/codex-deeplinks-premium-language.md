# Premium offer, language pack and gift code links

## What tdesktop does
`core/local_url_handlers.cpp`:
- `premium_offer` (`tg://premium_offer`, `t.me/premium_offer`) opens the
  Premium settings page (`ShowPremium`).
- `setlanguage?lang=X` / `t.me/setlanguage/X` calls `switchWithWarning`
  (asks before switching); with no language it opens the language list.
- `t.me/giftcode/<slug>` calls `ResolveGiftCode` -> `checkGiftCode`; an
  expired code shows a toast, otherwise `GiftCodeBox` lists From, To, Gift,
  Reason and Date, with a "Use Link" button (or the used-on date).
- There is no handler for a privacy policy link; `settings/privacy*` paths
  go to the privacy settings, which Quill already routes.

## What changed
- `premium_offer` already resolved through TDLib to the Premium page. The
  web form `t.me/premium_offer` was being treated as a username, so it and
  `premium_multigift` are now reserved paths (`WEB_RESERVED_PATHS`) and go
  to `getInternalLinkType`. The `settingsSectionPremium` section now opens
  Premium too (it showed the settings list before).
- `internalLinkTypePremiumGiftCode` and `internalLinkTypeLanguagePack` are
  parsed and routed (`DeepLinkUi::GiftCode`, `DeepLinkUi::LanguagePack`).
- `settingsSectionPrivacyPolicy` and `settingsSectionLanguage` give an
  explanatory message (policy URL; Quill is English only).
- New requests `checkPremiumGiftCode`, `applyPremiumGiftCode`,
  `getLanguagePackInfo`, with purposes in the payments and settings domains,
  payloads `GiftCodeInfo` / `LanguagePackInfo`, state in
  `payments.gift_code` and `settings.language_link`, and driver methods in
  `src/connect/link_info.rs`.
- `src/ui/link_info_boxes.rs`: the gift code box (From, To, Gift, Reason,
  Date, used-on line) and the language box. `applyPremiumGiftCode` is sent
  only by the "Use Link" click, only for a checked, unused code, and not
  twice. The language box never changes a setting.

## Verified
Unit tests: link routing, envelope parse, request JSON, wording, reducer
and driver flows (check never applies, apply gated on an unused code, no
double apply, errors keep the box open, language lookup sends nothing
else). Gate. Not run against a live account: no code was applied and no
language was changed. The boxes were not screenshotted.
