//! Applies TDLib updates and answers for stories, story albums and close friends.
use crate::state::*;
use crate::telegram::envelope::StoriesPayload;

impl Session {
    /// Applies one stories payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_stories_payload(
        &mut self,
        payload: StoriesPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            StoriesPayload::UpdateChatActiveStories { active_stories } => {
                // Phase 9.1: keep the tray entry only for the main story
                // list; archived / hidden chats drop out of the tray.
                self.upsert_story_tray_entry(active_stories);
            }
            StoriesPayload::ChatActiveStories { active_stories } => {
                // Phase 9.1: `getChatActiveStories` answer — refresh the
                // tray entry (matched by `@extra` in the UI's fetch guard,
                // but the object itself is authoritative).
                self.upsert_story_tray_entry(active_stories);
            }
            StoriesPayload::Story { story, files } => {
                // Phase 9.1: `getStory` response or `updateStory` update.
                self.remember_files(&files);
                // Phase 9.3: a `postStory` answer is the pending story —
                // its id is the temporary id the succeeded/failed updates
                // correlate against.
                if pending.is_some_and(|p| p.purpose == RequestPurpose::PostStory) {
                    self.story_post.outcome = StoryPostOutcome::Posting { story_id: story.id };
                }
                self.stories.insert((story.poster_chat_id, story.id), story);
            }
            StoriesPayload::StoryAlbums { albums } => {
                // Phase 9.7: `getChatStoryAlbums` — honored only for the
                // matching purpose (a stray `storyAlbums` never flips the
                // UI); replaces the chat's album list.
                if pending.is_some_and(|p| p.purpose == RequestPurpose::GetChatStoryAlbums)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.story_albums.insert(chat_id.0, albums);
                    self.clear_story_page_op(RequestPurpose::GetChatStoryAlbums);
                }
            }
            StoriesPayload::StoryAlbum { album } => {
                self.apply_story_album(album, pending);
            }
            StoriesPayload::Stories {
                total_count,
                stories,
                pinned_story_ids,
            } => self.apply_stories(total_count, stories, pinned_story_ids, pending, extra, seq),
            StoriesPayload::CanPostStoryResult { result } => {
                // Phase 9.3: `canPostStory` answer — honored only for the
                // composer's own check (purpose-gated, so a stray result
                // never flips the UI).
                if pending.is_some_and(|p| p.purpose == RequestPurpose::CheckCanPostStory) {
                    self.story_post.eligibility = Some(result);
                    self.story_post.check_error = None;
                }
            }
            StoriesPayload::UpdateStoryDeleted {
                poster_chat_id,
                story_id,
            } => {
                // Phase 9.2: drop the story from the cache and from the
                // poster's tray entry. The UI closes the viewer when its
                // current story disappears from the cache.
                self.stories.remove(&(poster_chat_id, story_id));
                let empty = if let Some(tray) = self.story_tray.get_mut(&poster_chat_id) {
                    tray.stories.retain(|info| info.story_id != story_id);
                    tray.stories.is_empty()
                } else {
                    false
                };
                if empty {
                    self.story_tray.remove(&poster_chat_id);
                }
            }
            StoriesPayload::UpdateStoryPostSucceeded {
                story,
                files,
                old_story_id,
            } => {
                // Phase 9.2: a story posted from another client is live —
                // upsert it and refresh the poster's tray row so an own
                // story appears in the tray.
                self.remember_files(&files);
                let poster_chat_id = story.poster_chat_id;
                // Phase 9.3: our own pending post went live — the composer
                // shows "Posted".
                if matches!(
                    self.story_post.outcome,
                    StoryPostOutcome::Posting { story_id } if story_id == old_story_id
                ) {
                    self.story_post.outcome = StoryPostOutcome::Succeeded;
                }
                self.stories.insert((story.poster_chat_id, story.id), story);
                self.story_tray_refresh.insert(poster_chat_id);
            }
            StoriesPayload::UpdateStoryPostFailed { story, error } => {
                // Phase 9.2: a story failed to post — drop it like a delete
                // (it never went live).
                // Phase 9.3: the failure reaches our own pending post (the
                // `sendStory` blocker was retracted — the constructor is
                // `postStory`, schema `td_api.tl:13715`) and the composer
                // shows it.
                if matches!(
                    self.story_post.outcome,
                    StoryPostOutcome::Posting { story_id } if story_id == story.id
                ) {
                    self.story_post.outcome = StoryPostOutcome::Failed(format!(
                        "Posting failed: {}",
                        error_reason(&error)
                    ));
                }
                self.stories.remove(&(story.poster_chat_id, story.id));
                let empty = if let Some(tray) = self.story_tray.get_mut(&story.poster_chat_id) {
                    tray.stories.retain(|info| info.story_id != story.id);
                    tray.stories.is_empty()
                } else {
                    false
                };
                if empty {
                    self.story_tray.remove(&story.poster_chat_id);
                }
            }
            StoriesPayload::StoryAvailableReactions {
                reactions,
                recent,
                popular,
                allow_custom_emoji,
            } => {
                if let Some(pending) = pending
                    && let RequestPurpose::Messages(MessagesPurpose::GetMessageAvailableReactions {
                        message_id,
                    }) = pending.purpose
                    && let Some(chat_id) = pending.chat_id
                {
                    self.accept_message_reaction_options(
                        chat_id,
                        MessageId(message_id),
                        reactions,
                        recent,
                        popular,
                        allow_custom_emoji,
                    );
                } else {
                    // Phase 9.2: `getStoryAvailableReactions` answer — the
                    // viewer picker options.
                    self.story_available_reactions = Some(reactions);
                }
            }
            StoriesPayload::StoryInteractions { interactions } => {
                // Phase 9.5: a `getStoryInteractions` page — honored only
                // for the viewer's own fetch (purpose-gated, and the page
                // is dropped when the viewer moved to another story).
                if pending.is_some_and(|p| p.purpose == RequestPurpose::GetStoryInteractions)
                    && let Some(pending) = pending
                {
                    self.accept_story_interactions(pending, interactions);
                }
            }
            StoriesPayload::StoryStatistics { statistics } => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetStoryStatistics
                {
                    self.accept_story_statistics(pending, statistics);
                }
            }
            StoriesPayload::PublicForwards { forwards } => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetStoryPublicForwards
                {
                    self.accept_public_forwards(pending, forwards);
                }
            }
            StoriesPayload::FoundStories { found } => {
                if pending.is_some_and(|p| p.purpose == RequestPurpose::SearchPublicStories) {
                    self.accept_found_stories(found);
                }
            }
            StoriesPayload::ReportStoryResult(result) => {
                // Phase 9.5: a `reportStory` answer — honored only for the
                // viewer's own report flow.
                if pending.is_some_and(|p| p.purpose == RequestPurpose::ReportStory)
                    && let Some(pending) = pending
                {
                    self.accept_story_report(pending, result);
                }
            }
            StoriesPayload::UpdateStoryStealthMode {
                active_until_date,
                cooldown_until_date,
            } => {
                // Phase 9.5: stealth-mode state changed (Telegram X keeps
                // the same two timestamps; there is no getter, so updates
                // are the only source).
                self.apply_update_story_stealth_mode(active_until_date, cooldown_until_date);
            }
        }
    }
}
