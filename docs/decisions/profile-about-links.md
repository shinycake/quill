# Profile about links

## What tdesktop does
`info/profile/info_profile_values.cpp` (`AboutWithEntities`) parses a group or channel description for links, mentions and hashtags, and for groups and megagroups strips external links (`StripExternalLinks`: only Telegram's own hosts stay clickable, `t.me` included). `info/profile/info_profile_actions.cpp` (about label, ~1463-1528) gives the text a context menu: copy the link under the pointer, Copy Text, Translate (Selected when text is selected), unless the language is skipped.

## What changed
- `quill::text::about::about_entities` asks TDLib's synchronous `getTextEntities` (through `TdHub::shared_api()` and `TdJson::execute`) and keeps links, emails, mentions and hashtags. tdesktop parses on the client only because it has no TDLib; here TDLib's own parser handles TLDs, balanced parentheses, Unicode punctuation and username rules.
- It runs when `supergroupFullInfo` or `updateSupergroupFullInfo` is applied (`session_apply_chats.rs`), and the result is stored in `SupergroupFullInfoData::description_entities`. Render only reads state. Without a loaded `libtdjson` (offline demos, tests) the list is empty, so demo fixtures set it by hand (`ready-info-channel`, `ready-info-group`).
- `strip_external_links` applies the group rule at render time, because the full info can arrive before the chat is known to be a group or a channel.
- `src/ui/profile_panels/about_text.rs` draws the block through `rich_text_line`, so link clicks reuse the message link handling (confirmation for hidden or look-alike links stays). The right-click menu is built when it opens: the copy item of the link under the pointer (the noted right-clicked link is taken so it cannot go stale), Copy Text, and Translate when `translate_menu_offered` says so (language detection runs only then).
- New lang key `qll_people_about_copied`.

## Not done
- Translate Selected / Copy Selected over a text selection (the about block has no selection model).
- The user bio row still copies only; it already carries entities from TDLib, so linkifying it is a separate change to the details-row model.
- Basic-group descriptions: the panel for them shows no description today.

## Verified
Unit tests on a canned `textEntities` answer (UTF-16 to UTF-8, parentheses), the group strip rule (external, schemeless and hidden links), and, when `libtdjson` loads, TDLib's own answer on the hard cases (`Node.js`, balanced parentheses, trailing Unicode punctuation, username next to punctuation). GPUI tests check the block exists for a channel and a group and which links each keeps. Light and dark captures of `ready-info-channel` and `ready-info-group`.
