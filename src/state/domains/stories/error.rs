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
            // Phase 9.5: a `getStoryInteractions` / `reportStory` /
            // `activateStoryStealthMode` error — the viewer panel /
            // report flow / stealth button shows it instead of
            // spinning forever.
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
            Some(RequestPurpose::SearchPublicStories) => {
                self.fail_story_search(format!("Could not search stories: {}", error_reason(err)));
            }
            Some(RequestPurpose::ReportStory) => {
                if let Some(pending) = pending {
                    self.fail_story_report(
                        pending,
                        format!("Reporting failed: {}", error_reason(err)),
                    );
                }
            }
            Some(RequestPurpose::ActivateStoryStealthMode) => {
                self.stories.stealth_error =
                    Some(format!("Stealth mode failed: {}", error_reason(err)));
            }
            // Phase 9.5: a posted-story management call failed —
            // clear the spinner and surface the sanitized error.
            Some(
                RequestPurpose::EditStory
                | RequestPurpose::EditStoryCover
                | RequestPurpose::SetStoryPrivacySettings,
            ) => {
                self.stories.manage.pending = false;
                self.stories.manage.error =
                    Some(format!("Story update failed: {}", error_reason(err)));
            }
            // Phase 9.5 (review fix-up): `getChatsToPostStories`
            // failed — surface a transient error so the "Post as"
            // picker doesn't silently show only "Myself".
            Some(RequestPurpose::GetChatsToPostStories) => {
                self.stories.post.check_error = Some(format!(
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
                if purpose == RequestPurpose::SetCloseFriends {
                    self.stories.close_friends_pending = None;
                }
                self.fail_story_page_op(purpose, error_reason(err).to_string());
            }
            _ => {}
        }
    }
}
