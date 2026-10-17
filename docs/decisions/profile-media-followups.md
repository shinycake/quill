# Profile and shared media follow-ups

Leftover Low findings from the reviews of the shared-media search and
common-groups PRs.

## Channel member count

tdesktop's profile and info code writes a channel's count with
`lng_chat_status_subscribers` and a group's with `lng_chat_status_members`,
both with `lt_count_decimal` (`info_profile_status_label.cpp:50-60`,
`info_profile_actions.cpp:3292`). `lt_count_decimal` is the full number with
thousands separators ("12,345 subscribers"), not the compact "12.3K" form.
tdesktop never pairs `lt_count_short` with these keys, the chat header
included (`history_view_top_bar_widget.cpp:1942-2004`); Quill's compact header
and cover counts were a separate gap, fixed in `chat-count-decimal`. Quill's
members block header printed "12345 members" for channels too. It now uses the
two lang keys through `lang::trn_decimal`, so channels read "12,345
subscribers", groups "12,345 members", and translations apply.

## Per-tab fetch stamps in shared media

One generation was shared by every tab, so any tab's `begin_fetch` dropped the
answers of other tabs' in-flight pages and left their `loading_more` set.
Each tab now holds the stamp of the request it waits on. `begin_fetch`
restamps only its own tab (and clears its `loading_more`, since the new first
page supersedes an older page); opening, closing and dropping a search
restamp the affected tabs. `fail` also clears `loading_more`.

## Stale search input

The input subscription ignores events whose entity is not the current
`media_search.input`. When the field is swapped because the key changed,
focus moves to the new field if the old one had it and the new field is
drawn (`shared_media_search_shown`); the focus call runs through
`window.defer`, after the frame, so render only swaps the field.

## Common groups

tdesktop's `ListController::loadMoreRows`
(`info_common_groups_inner_widget.cpp:95`) has no failure handler: after an
error its request id stays set and it never asks again. Quill matches that:
a failing follow-up page keeps the loaded groups and marks the list
exhausted, so Load more goes away rather than repeating (for example TDLib's
"Wrong offset_chat_id"). The no-results text is `lng_bot_groups_not_found`
("No groups found"), as in tdesktop's `prepare()`.

## Verification

Reducer and driver tests, GPUI tests for the stale input, focus and no-results
text, and light and dark captures of `ready-info-channel`.
