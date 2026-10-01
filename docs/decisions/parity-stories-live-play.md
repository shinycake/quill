## Slice: stories-live-play (2026-09-30, loop 1)

- **Scope:** join (not RTMP-play) live stories. `storyContentLive`
  carries `group_call_id` + `is_rtmp_stream` (`schema/td_api.tl:6662`).
  Joining = the story's group call via the existing C3a group-call
  machinery (`joinLiveStory`, the schema’s dedicated live-story join method).
- **What was built:**
  - `StoryContentView::Live` now carries `{ group_call_id,
    is_rtmp_stream }` (was a unit variant); `LiveStoryCall` rides on
    `StoryViewerItem`.
  - Driver `join_live_story(chat_id, story_id)`: gates (chats path,
    story cached, live content, no active 1:1/group call, no pending
    join) → `getGroupCall` (existing `fetch_group_call`) + records a
    `LiveStoryJoinIntent { group_call_id, request }`. The ingest pump
    (`maybe_join_live_story`) issues `joinLiveStory` once the
    `groupCall` answer has created the unjoined tracker; the intent is
    kept while the `getGroupCall` request is in flight (so an early
    ingest can't lose it) and dropped when the request completes
    without a tracker (fetch failed — a later tap retries). The
    group-call overlay's Join button stays the manual fallback.
  - UI: the story viewer's live placeholder becomes a "🔴 Live story"
    + "Join live" button (failures surface via `status_note`); RTMP
    lives keep an honest "not supported yet" note.
  - Tests: envelope parse carries the id/flag; driver test covers the
    full two-step (getGroupCall shape → updateGroupCall → joinLiveStory
    shape, intent cleared) plus all four refusal gates.
  - README box `parity:stories-live-play` checked with the RTMP note.
- **Key decisions (ponytail):**
  - Reused `fetch_group_call` + `joinLiveStory` + the UpdateGroupCall
    tracker path — no new request purposes, no parallel join plumbing.
  - RTMP playback remains outside this slice and has not been verified.
- **Not verifiable without live Telegram:** a real live story join
  (needs a contact streaming now); the request shapes and pump wiring
  are driver-tested.
- **Out of this slice:** RTMP live-story playback; live-story viewer
  counts / top donors (`liveStoryDonors`); starting a live story
  (`startLiveStory` is posting-side).

