//! Failed requests for stories, story albums and close friends.
use crate::state::*;

impl Session {
    /// Reacts to a failed stories request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_stories_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        // Phase 9.3: a `postStory` / `canPostStory` error — the
        // composer shows it instead of spinning forever.
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::Stories(StoriesPurpose::Live(purpose))) => {
                self.apply_live_story_error(purpose, pending.map(|p| p.id), err);
            }
            Some(RequestPurpose::PostStory) => {
                self.stories.post.outcome =
                    StoryPostOutcome::Failed(format!("Posting failed: {}", error_reason(err)));
            }
            Some(RequestPurpose::CheckCanPostStory) => {
                // S14: a chat-level story restriction surfaces the
                // TGX-verbatim notice; anything else keeps the
                // generic eligibility failure.
                self.stories.post.check_error = Some(
                    crate::story_restriction::notice_for_error_class(err.class)
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            format!("Eligibility check failed: {}", error_reason(err))
                        }),
                );
            }
            Some(RequestPurpose::GetStoryInteractions) => {
                if let Some(pending) = pending {
                    self.fail_story_viewers(
                        pending,
                        format!("Could not load viewers: {}", error_reason(err)),
                    );
                }
            }
            Some(
                purpose @ (RequestPurpose::GetStoryStatistics
                | RequestPurpose::GetStoryPublicForwards),
            ) => {
                if let Some(pending) = pending {
                    self.fail_story_insights(
                        pending,
                        purpose,
                        format!("Could not load statistics: {}", error_reason(err)),
                    );
                }
            }
            Some(RequestPurpose::LoadActiveStories) => {
                self.stories
                    .main_paging
                    .failed(err.code, std::time::Instant::now());
            }
            Some(RequestPurpose::LoadHiddenStories) => {
                self.stories
                    .hidden_paging
                    .failed(err.code, std::time::Instant::now());
            }
            // Only a not-found or invalid answer means the story is gone; a
            // timeout, FLOOD_WAIT or 500 is asked for again after a wait.
            Some(RequestPurpose::GetStory) => {
                if let Some(story) = pending.and_then(|p| Some((p.chat_id?.0, p.story_id?))) {
                    if matches!(err.code, 400 | 404) {
                        self.stories.missing.insert(story);
                    } else {
                        self.stories
                            .story_fetch_failed(story, std::time::Instant::now());
                    }
                }
            }
            Some(RequestPurpose::GetStoryCustomEmojiStickers) => {
                self.stories
                    .custom_emoji_fetch_failed(std::time::Instant::now());
            }
            Some(RequestPurpose::SearchPublicStories) => {
                if let Some(pending) = pending {
                    self.fail_story_search(
                        pending,
                        format!("Could not search stories: {}", error_reason(err)),
                    );
                }
            }
            Some(RequestPurpose::ReportStory) => {
                if let Some(pending) = pending {
                    self.fail_story_report(
                        pending,
                        format!("Reporting failed: {}", error_reason(err)),
                    );
                }
            }
            // Phase 9.5: a posted-story management call failed —
            // clear the spinner and surface the sanitized error.
            Some(
                RequestPurpose::EditStory
                | RequestPurpose::EditStoryCover
                | RequestPurpose::SetStoryPrivacySettings,
            ) => {
                if let Some(pending) = pending {
                    self.stories.manage.settle(
                        pending.id,
                        Some(format!("Story update failed: {}", error_reason(err))),
                    );
                }
            }
            // Phase 9.5 (review fix-up): `getChatsToPostStories`
            // failed — the "Post as" picker draws the error instead of
            // silently showing only "Myself"; it is not the eligibility
            // check's error.
            Some(RequestPurpose::GetChatsToPostStories) => {
                self.stories.post_as.fail(format!(
                    "Could not load \"Post as\" chats: {}",
                    error_reason(err)
                ));
            }
            // Phase 9.7: a story-page mutation error — the page's
            // status line shows it instead of spinning forever.
            Some(
                purpose @ (RequestPurpose::GetChatStoryAlbums
                | RequestPurpose::GetStoryAlbumStories
                | RequestPurpose::CreateStoryAlbum
                | RequestPurpose::ReorderStoryAlbums
                | RequestPurpose::DeleteStoryAlbum
                | RequestPurpose::SetStoryAlbumName
                | RequestPurpose::AddStoryAlbumStories
                | RequestPurpose::RemoveStoryAlbumStories
                | RequestPurpose::ReorderStoryAlbumStories
                | RequestPurpose::GetChatArchivedStories
                | RequestPurpose::GetChatPostedToChatPageStories
                | RequestPurpose::SetChatPinnedStories
                | RequestPurpose::GetCloseFriends
                | RequestPurpose::SetCloseFriends
                | RequestPurpose::SetChatActiveStoriesList
                | RequestPurpose::ToggleStoryIsPostedToChatPage),
            ) => {
                if purpose == RequestPurpose::GetCloseFriends
                    && let Some(pending) = pending
                {
                    self.stories.close_friends.fail_load(pending.id);
                }
                // The profile gallery's Stories tab loads from this request
                // too: it fails instead of loading forever.
                if purpose == RequestPurpose::GetChatPostedToChatPageStories
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.fail_profile_tab(
                        chat_id,
                        SharedMediaTab::Stories,
                        crate::state::request_errors::call_request_error_line(
                            err,
                            "Could not load this tab",
                        ),
                    );
                }
                self.fail_story_page_op(pending, error_reason(err).to_string());
            }
            _ => {}
        }
    }
}
