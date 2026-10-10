## codex/forum-thread-stories (2026-10-09)

Parity cluster: topic and thread info, story statistics, public story search.

### What tdesktop does
- Story statistics (`info/statistics/info_statistics_inner_widget.cpp`): an overview of views, public shares, reactions and private shares, two charts (story interactions, story reactions), and a "Public Shares" list loaded in pages.
- Topic and reply-thread details live in the topic profile and the replies section header: who created the topic and when, status, unread counts, reply count.
- Public stories can be searched by hashtag, cashtag, location or venue.

### What changed
- Requests (`telegram/requests_story_insights.rs`, all in `schema/td_api.tl`): `getStoryStatistics`, `getStoryPublicForwards`, `searchPublicStoriesByTag`, `searchPublicStoriesByLocation`, `searchPublicStoriesByVenue`. Limits are clamped to 1..100 and the tag is sent without its `#` or `$`.
- Parsing (`telegram/envelope/story_insights.rs`): `storyStatistics`, `publicForwards` (message and story forwards), `foundStories`. Story areas now keep `locationAddress` parts and the venue provider and id, which the location and venue searches need.
- State (`state/story_insights.rs`): `Session::story_insights` (statistics, forwards, paging) and `Session::story_search` (query, hits, paging). Answers for a story the viewer has left are dropped, errors land in the panel, and found stories join the normal story cache so the viewer can open them.
- Driver (`connect/story_insights.rs`): statistics are only requested when `story.can_get_statistics`; requests are deduplicated while in flight.
- Topic info: `ForumTopic` and `ForumTopicInfoUpdate` gained `creation_date`, `creator` and `is_outgoing`. Only two struct literals needed changes, so no builder was added.
- UI: a "Statistics" button and panel in the story viewer; "Stories here" in the viewer for stories tagged with a venue or location; a "Public stories" section in the sidebar search (a typed `#tag` or `$tag` offers the search, hits open in the viewer); an info button on the topic strip, "Topic info" in the topic tab menu, and an info button on the thread root bar. Rows for the topic and thread cards come from `topic_info.rs`.

### Not done
- `forum-second-column`, `thread-section`, `stories-reply-media` and `stories-live-stream-playback` are untouched. The thread view already has a composer, but forum-topic replies still do not open as threads, so the item stays open.
- Public shares that are story reposts are listed but cannot be opened from the list.
- Message public forwards for ordinary channel posts (`getMessagePublicForwards`) are out of scope.

### How verified
- Unit tests: request shapes, envelope parsing, reducer (paging, stale answers, errors), driver gating and paging, info row formatting, tag detection.
- Demo kind `ready-forum-thread-stories` with `QUILL_DEMO_FTS_VIEW=topic|thread|stats|search`, captured with `demo-capture` and viewed. No account was used and nothing was posted.
- The gate (fmt, clippy with and without `ui`, tests) passes.
