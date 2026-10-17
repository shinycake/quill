# Profile about links

## What tdesktop does
`info/profile/info_profile_actions.cpp` (about label, around lines 1490-1530) shows a group or channel description as linkified text: URLs, @mentions, #hashtags and emails are clickable. Its context menu offers Copy Text and, unless the language is skipped, Translate (Translate Selected Text over a selection).

## What changed
- `quill::text::autolink::autolink_entities` finds URLs, bare domains, @mentions, #hashtags and emails in plain text and returns `TextEntity` spans with UTF-8 offsets. TDLib sends descriptions without entities and Quill has no `getTextEntities` plumbing, so this local parser is a documented deviation from tdesktop's `ParseEntities`.
- `src/ui/profile_panels/about_text.rs`: `about_text_block` renders the description through `rich_text_line` (so clicks reuse the message link handling) and attaches a right-click menu: Copy Text, Translate (gated by `translate_menu_offered`, like the message menu).
- The supergroup and channel info panel uses it. New lang key `qll_people_about_copied`.

## Not done
The user bio row still copies only; adding Translate there needs the details-row model changed (separate change).

## Verified
Unit tests for the parser (urls, bare domains, mentions, hashtags, emails, multibyte offsets, false positives like `1.5`), a GPUI test that the block exists, and light and dark captures of `ready-info-channel` showing the URL and email as links.
