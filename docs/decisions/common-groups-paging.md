# Common groups paging

## What tdesktop does
`info/profile/info_profile_common_groups` (and the profile's common groups tab) lists the groups you share with a user, loading more as you scroll (`getCommonChats` with the last chat id as offset). The section title is a plural string ("1 group in common", "N groups in common") and the list can be searched.

## What changed
- `getGroupsInCommon` loads 40 at a time (`GROUPS_IN_COMMON_PAGE`). The next page sends the last listed chat id as `offset_chat_id` (`ConnectDriver::fetch_more_groups_in_common`). A page shorter than 40 marks the list complete (`CommonGroupsPaging::exhausted`); `loading_more` guards against a second request.
- A follow-up page appends (dropping overlap); a failed follow-up page keeps the groups already shown instead of replacing the list with a Retry row. Opening the profile again or Retry starts over.
- `src/ui/profile_panels/common_groups.rs` (new): the section, a search field once the list holds more than 20 groups (filters the loaded rows by title, with a "No results" line), and a Load more row while the server has more, which becomes "Loading…" while a page is in flight. The title uses the new plural key `lng_profile_common_groups` and the count from the full info (`group_in_common_count`), so it reads the real total before every page is loaded. The shared-media count row uses the same key.
- The search field is made when the profile opens (`reset_common_groups_search`), one per opened user.

## Not done
No scroll-triggered loading (a Load more row instead); the search filters only the groups loaded so far.

## Verified
Reducer tests for paging (append with overlap, short page ends the list, failed follow-up keeps the list), a GPUI test on the `ready-info-common-groups` demo (45 groups, first 40 loaded) checking the search field and Load more row exist, light and dark captures.
