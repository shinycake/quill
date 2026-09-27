### Stories

#### View stories
- [x] Active-story tray above the chat list, unread ring vs muted read ring (ui/mod.rs:7566 story_tray; DECISIONS.md Phase 9.1) <!-- parity:stories-tray -->
- [x] Tap a tray entry opens fullscreen viewer on the latest story; missing details prefetched via getStory (ui/mod.rs:5427 open_story_viewer) <!-- parity:stories-viewer-open -->
- [x] Viewer renders photo stories at full size and video stories (video shows thumbnail only; full clip not renderable by the img element) (src/story_viewer.rs; DECISIONS.md Phase 9.1) <!-- parity:stories-viewer-photo-video -->
- [x] Viewer shows poster name + "Story N of M" + caption with formatted entities (src/story_viewer.rs:55-56 caption fields) <!-- parity:stories-viewer-caption -->
- [x] Viewer Prev / Next / Close; Escape closes viewer before other overlays (DECISIONS.md Phase 9.1) <!-- parity:stories-viewer-nav -->
- [ ] Segmented progress bar with auto-advance to next story <!-- parity:stories-progress-bar -->
- [x] Live/unsupported story content degrades to a placeholder in the item list (src/story_viewer.rs:11-13) <!-- parity:stories-live-placeholder -->
- [ ] Join or play live stories (storyContentLive / startLiveStory in schema) <!-- parity:stories-live-play -->
- [x] openStory/closeStory mark stories viewed; read state from max_read_story_id (telegram/requests.rs:2519,2531) <!-- parity:stories-read-state -->
- [x] Quick-react ❤️ toggle on viewer, chosen state shown (ui/mod.rs:5505-5566) <!-- parity:stories-quick-react -->
- [x] Reaction picker fed by getStoryAvailableReactions (ui/mod.rs:5532 toggle_story_reaction_picker) <!-- parity:stories-reaction-picker -->
- [ ] Chosen custom-emoji or paid reactions (parser drops them: chosen_reaction_emoji emoji-only, telegram/envelope.rs Phase 9.2) <!-- parity:stories-custom-reactions -->
- [x] Reaction removal (setStoryReaction with null; request asserts, telegram/requests.rs:2556) <!-- parity:stories-reaction-remove -->
- [x] Interaction counters (views / hearts / reposts, non-zero, when can_get_interactions) (ui/mod.rs Phase 9.2) <!-- parity:stories-interaction-counters -->
- [ ] Detailed viewers list (getStoryInteractions — explicit schema surface kept out of Phase 9.2; not called anywhere) <!-- parity:stories-viewers-list -->
- [x] Text reply to a story via sendMessage + inputMessageReplyToStory, gated on can_be_replied (telegram/requests.rs Phase 9.2; ui/mod.rs Reply row) <!-- parity:stories-reply -->
- [x] Delete own story, gated on can_be_deleted; viewer closes when the story leaves the cache; updateStoryDeleted handled (telegram/requests.rs:2584, telegram/envelope.rs:4660) <!-- parity:stories-delete -->
- [ ] Report story (reportStory in schema, unused) <!-- parity:stories-report -->
- [ ] Stealth mode / hide view from poster (activateStoryStealthMode, updateStoryStealthMode in schema; TGX consumes them, Quill does not) <!-- parity:stories-stealth-mode -->
- [ ] Clickable story areas (location, venue, suggested reaction, message, link, weather, gift) — parser drops areas (telegram/envelope.rs:3328) <!-- parity:stories-areas-view -->
- [ ] Story albums: view albums, add to album (schema getChatStoryAlbums / createStoryAlbum / ... in 1.8.67, unused) <!-- parity:stories-albums -->
- [ ] Archive story list (storyListArchive trays remove the row; no archive UI) (DECISIONS.md Phase 9.1 "Out of this slice") <!-- parity:stories-archive -->
- [ ] Pinned stories on chat page (getChatPostedToChatPageStories, setChatPinnedStories unused) <!-- parity:stories-pinned -->
- [ ] Story notification settings (mute stories per chat, story sound, show story poster) — parsed into fields only (telegram/envelope.rs:1859-1860) <!-- parity:stories-notify-settings -->
- [ ] "Only admins can send stories in this group" and story-restriction notices (TGX strings ChatDisabledStory / ChatRestrictedStory / ChatRestrictedStoryUntil; no Quill handling) <!-- parity:stories-restriction-notice -->

#### Post stories
- [ ] Story composer: photo picker (partial: post-success/fail updates parsed — telegram/envelope.rs:4774,4786) <!-- parity:stories-post-photo-composer -->
- [ ] Video story upload (partial: inputStoryContentVideo exists in schema, unused) <!-- parity:stories-post-video -->
- [ ] canPostStory eligibility check before posting (schema td_api.tl:13702, unused) <!-- parity:stories-can-post-check -->
- [ ] postStory with caption + privacy selector (partial: schema constructor verified td_api.tl:13715; post-succeeded/failed reducer upserts story + queues tray refresh, DECISIONS.md 9.2) <!-- parity:stories-post-call -->
- [ ] Privacy selector: Everyone / Contacts / Close friends / Selected users (schema storyPrivacySettings*, td_api.tl:8928-8937; unused) <!-- parity:stories-post-privacy -->
- [ ] Formatted caption with entities on post <!-- parity:stories-post-caption -->
- [ ] Add story areas (stickers, links, reactions) on own post <!-- parity:stories-post-areas -->
- [ ] Active period / expiry selection (postStory active_period param unused) <!-- parity:stories-post-expiry -->
- [ ] "Post to chat page" toggle (postStory is_posted_to_chat_page / toggleStoryIsPostedToChatPage unused) <!-- parity:stories-post-to-chat-page -->
- [ ] "Protect content" (no forwarding) toggle (postStory protect_content param unused) <!-- parity:stories-post-protect -->
- [ ] Honest pending / succeeded / failed states (partial: updateStoryPostSucceeded/PostFailed parsed and applied — telegram/envelope.rs:10966,10985; no UI states) <!-- parity:stories-post-status -->
- [ ] Edit own story content / caption / areas (editStory, td_api.tl:13732, unused) <!-- parity:stories-edit -->
- [ ] Edit story cover frame (editStoryCover, td_api.tl:13738, unused) <!-- parity:stories-edit-cover -->
- [ ] Change a posted story's privacy settings (setStoryPrivacySettings, td_api.tl:13743, unused) <!-- parity:stories-post-change-privacy -->
- [ ] Post stories on behalf of a channel/supergroup (getChatsToPostStories, canPostStory chat_id unused) <!-- parity:stories-post-as-channel -->
- [ ] Admin story rights management (post / edit / delete others' stories; TGX strings RightStories*; not in Quill) <!-- parity:stories-admin-rights -->
- [ ] Repost / re-share a story (storyRepostInfo / storyInteractionTypeRepost in schema; parser drops repost info) <!-- parity:stories-repost -->
