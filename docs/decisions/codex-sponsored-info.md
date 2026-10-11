# Sponsored info box

## What tdesktop does
The Ad menu on a sponsored message opens `AboutBox` (`menu/menu_sponsored.cpp`,
`lng_sponsored_revenued_*`): subtitle, three points (privacy, channel creator
revenue share, can be removed with Premium) and a "Can I Launch an Ad?" footer
linking to the ad platform. Advertiser info (`sponsor.info`,
`additional_info`) lives in an "Advertiser info" submenu, copyable.

## What changed
- The inline "About These Ads" panel in the sponsored footer is now a real
  dialog, `src/ui/sponsored_about.rs` (`DialogKind::SponsoredAbout`,
  `register_dialogs!`, priority 7455), opened from "About this ad".
- It adds the Premium and ads.telegram.org "Learn more" links and an
  "Advertiser info" block with Copy, built by the unit-tested
  `advertiser_lines` (trim, drop empty and duplicate lines).
- Report ad and Hide ads stay in the menu, using the existing flows.
- `MessageUi.sponsored_about_open: bool` became `sponsored_about: Option<_>`.

## Verified
gate.sh OK; two unit tests; demo-capture screenshot of the dialog on
`ready-sponsored` (temporary demo hook, not committed). The "Level N channels"
sentence is omitted since Quill has no boost-level data for it here.
