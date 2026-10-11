# Members list inline in the group info panel

## What tdesktop does
`info/profile/info_profile_members.cpp` puts a members block under the group
details: a header with the count, a search button once the group has 20
members (`kEnableSearchMembersAfterCount`), an add button when the viewer may
add people, then the rows. `ParticipantsOnlineSorter`
(`boxes/peers/edit_participants_box.cpp`) sorts basic groups and megagroups up
to `chatSizeMax` by `SortByOnlineValue` (bots last) and re-sorts when a status
changes; bigger groups keep the server's recent order. `MemberListRow` shows
the custom title, or an "owner" / "admin" pill. The row context menu is the
participants box menu. Rows load 16 first, then 200 per page with an offset.

## What changed
- `src/inline_members.rs`: pure rules (sort value, which groups sort, badge,
  title, search threshold, local search for basic groups, paging decisions,
  page merge). Unit tested, including ties and status changes.
- `GetSupergroupMembersMore { filter, offset }` purpose plus
  `fetch_supergroup_members_more` in `src/connect/moderation/members_more.rs`;
  the reducer appends the page to the cached one without repeats.
- `src/ui/inline_members*.rs`: the block in the supergroup and basic-group
  panels. Rows reuse `member_menu_build` (mention, tag, promote, restrict,
  ban, remove) and open the member's profile on click. The search button
  opens the member dialog with its search box focused, as tdesktop opens its
  full members section. When the viewer may not see the members (a channel),
  the block lists the administrators instead, titled "Administrators". The
  list draws 20 rows and reveals more, then
  fetches the next page, when scrolled near its end. Add opens the existing
  member dialog with its add section open. `updateChatMember` refetches the
  open panel's page; the last page stays on screen meanwhile.
- Hooks: `mod` lines, one `QuillApp` field, one line in `new_with_demo`, one
  call in `open_info_panel_target`, one in the stale-members drain, two
  `children(...)` lines in the panels.

## Differences
The search page cache is shared with the member dialog. Online sorting uses
the same size cutoff as tdesktop's default `chatSizeMax`.

## Verified
Unit tests (core, reducer, driver request shape). Visual check with a
temporary demo (not committed): online members first, "admin", custom title
and plain-text tags, header count, search and add buttons. gate.sh: see the PR.
Not verified: live scrolling to a second server page, right-click menus
against a real group.
