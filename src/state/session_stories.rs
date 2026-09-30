//! Story tray and story interaction state.
use super::*;

impl Session {
    /// Phase 9.1: insert or drop a story-tray entry. Only the main story
    /// list shows in the tray; archived (`list == Archive`) and hidden
    /// (`list == None`) chats are removed.
    pub(crate) fn upsert_story_tray_entry(&mut self, entry: ChatActiveStoriesView) {
        if entry.list == Some(StoryListView::Main) {
            self.story_tray.insert(entry.chat_id, entry);
        } else {
            self.story_tray.remove(&entry.chat_id);
        }
    }

    /// Phase 9.7: start tracking a story-page mutation (`Sending`).
    pub fn begin_story_page_op(&mut self, label: impl Into<String>) {
        self.story_page_op = Some(StoryPageOp {
            label: label.into(),
            state: StoryPageOpState::Sending,
        });
    }

    /// Phase 9.7: start tracking a story-page read (`Checking`).
    pub fn begin_story_page_check(&mut self, label: impl Into<String>) {
        self.story_page_op = Some(StoryPageOp {
            label: label.into(),
            state: StoryPageOpState::Checking,
        });
    }

    /// Phase 9.7: mark the story-page op `Succeeded` when its answer
    /// lands — only if the op was sent for this purpose (the driver sets
    /// the label from the same purpose).
    pub(crate) fn succeed_story_page_op(&mut self, purpose: RequestPurpose) {
        if self
            .story_page_op
            .as_ref()
            .is_some_and(|op| op.label == story_page_op_label(purpose))
        {
            self.story_page_op = Some(StoryPageOp {
                label: story_page_op_label(purpose),
                state: StoryPageOpState::Succeeded,
            });
        }
    }

    /// Phase 9.7: mark the story-page op `Failed` on a TDLib error.
    pub(crate) fn fail_story_page_op(&mut self, purpose: RequestPurpose, reason: String) {
        if self
            .story_page_op
            .as_ref()
            .is_some_and(|op| op.label == story_page_op_label(purpose))
        {
            self.story_page_op = Some(StoryPageOp {
                label: story_page_op_label(purpose),
                state: StoryPageOpState::Failed(reason),
            });
        }
    }

    /// Phase 9.7: clear the story-page op once its load answer landed (the
    /// loaded list itself is the honest state — no status line needed).
    pub(crate) fn clear_story_page_op(&mut self, purpose: RequestPurpose) {
        if self
            .story_page_op
            .as_ref()
            .is_some_and(|op| op.label == story_page_op_label(purpose))
        {
            self.story_page_op = None;
        }
    }

    /// Phase 9.1: tray entries for the story row above the chat list:
    /// main-list entries only, sorted by `(order, chat_id)` descending
    /// (schema `chatActiveStories` comment, line 6781).
    pub fn ordered_story_tray(&self) -> Vec<&ChatActiveStoriesView> {
        let mut entries: Vec<&ChatActiveStoriesView> = self
            .story_tray
            .values()
            .filter(|entry| entry.list == Some(StoryListView::Main))
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse((entry.order, entry.chat_id)));
        entries
    }

    /// Phase 9.5: (re)start the viewers panel for a story — resets rows
    /// and pagination when the story changes, keeps them when the same
    /// story is re-opened.
    pub fn begin_story_viewers(&mut self, chat_id: i64, story_id: i32) {
        let same = self
            .story_viewers
            .as_ref()
            .is_some_and(|state| state.chat_id == chat_id && state.story_id == story_id);
        if !same {
            self.story_viewers = Some(StoryViewersState {
                chat_id,
                story_id,
                ..Default::default()
            });
        }
    }

    pub fn clear_story_viewers(&mut self) {
        self.story_viewers = None;
    }

    /// Phase 9.5: accumulate one `storyInteractions` page into the
    /// viewers panel. A page for a different story (stale response after
    /// the viewer moved on) is dropped.
    pub fn accept_story_interactions(
        &mut self,
        pending: &PendingRequest,
        view: StoryInteractionsView,
    ) {
        let (Some(chat_id), Some(story_id)) = (pending.chat_id, pending.story_id) else {
            return;
        };
        let Some(state) = self.story_viewers.as_mut() else {
            return;
        };
        if state.chat_id != chat_id.0 || state.story_id != story_id {
            return;
        }
        state.loading = false;
        state.error = None;
        state.total_count = view.total_count;
        state.next_offset = view.next_offset;
        state.rows.extend(view.interactions);
    }

    /// Phase 9.5: mark the viewers fetch as failed; the panel shows the
    /// error with a retry instead of spinning forever.
    pub fn fail_story_viewers(&mut self, pending: &PendingRequest, message: String) {
        let (Some(chat_id), Some(story_id)) = (pending.chat_id, pending.story_id) else {
            return;
        };
        if let Some(state) = self.story_viewers.as_mut()
            && state.chat_id == chat_id.0
            && state.story_id == story_id
        {
            state.loading = false;
            state.error = Some(message);
        }
    }

    /// Phase 9.5: start the `reportStory` flow for a story — the UI
    /// renders `Checking` until the first answer lands.
    pub fn begin_story_report(&mut self, chat_id: i64, story_id: i32) {
        self.story_report = Some(StoryReportFlow {
            chat_id,
            story_id,
            stage: StoryReportStage::Checking,
        });
    }

    pub fn clear_story_report(&mut self) {
        self.story_report = None;
    }

    /// Phase 9.5: apply a `ReportStoryResult` answer. A result for a
    /// different story (stale response) is dropped. Mirrors TDLib's
    /// `ReportStoryQuery` mapping (`td/telegram/StoryManager.cpp:1406-1430`
    /// @d1085f9): an empty option list is a success, not a picker.
    pub fn accept_story_report(&mut self, pending: &PendingRequest, result: ReportStoryResult) {
        let (Some(chat_id), Some(story_id)) = (pending.chat_id, pending.story_id) else {
            return;
        };
        let Some(flow) = self.story_report.as_mut() else {
            return;
        };
        if flow.chat_id != chat_id.0 || flow.story_id != story_id {
            return;
        }
        flow.stage = match result {
            ReportStoryResult::Ok => StoryReportStage::Reported,
            ReportStoryResult::OptionRequired { title: _, options } if options.is_empty() => {
                StoryReportStage::Reported
            }
            ReportStoryResult::OptionRequired { title, options } => {
                StoryReportStage::PickOption { title, options }
            }
            ReportStoryResult::TextRequired {
                option_id,
                is_optional,
            } => StoryReportStage::TextRequired {
                option_id,
                is_optional,
            },
        };
    }

    /// Phase 9.5: the follow-up `reportStory` (reason picked / details
    /// submitted) is in flight.
    pub fn story_report_sending(&mut self, chat_id: i64, story_id: i32) {
        if let Some(flow) = self.story_report.as_mut()
            && flow.chat_id == chat_id
            && flow.story_id == story_id
        {
            flow.stage = StoryReportStage::Sending;
        }
    }

    /// Phase 9.5: a raw TDLib error on a `reportStory` request ends the
    /// flow with the sanitized message (there is no `Failed` result
    /// variant — errors arrive as `error` answers).
    pub fn fail_story_report(&mut self, pending: &PendingRequest, message: String) {
        let (Some(chat_id), Some(story_id)) = (pending.chat_id, pending.story_id) else {
            return;
        };
        if let Some(flow) = self.story_report.as_mut()
            && flow.chat_id == chat_id.0
            && flow.story_id == story_id
        {
            flow.stage = StoryReportStage::Failed(message);
        }
    }

    /// Phase 9.5: `reportStory` failed to send at all — the driver took
    /// the pending request back, so no answer will ever arrive. End the
    /// flow with the error instead of spinning on `Checking`/`Sending`
    /// forever.
    pub fn fail_story_report_send(&mut self, chat_id: i64, story_id: i32, message: String) {
        if let Some(flow) = self.story_report.as_mut()
            && flow.chat_id == chat_id
            && flow.story_id == story_id
        {
            flow.stage = StoryReportStage::Failed(message);
        }
    }

    /// Phase 9.5: store the latest `updateStoryStealthMode` state.
    pub fn apply_update_story_stealth_mode(
        &mut self,
        active_until_date: i32,
        cooldown_until_date: i32,
    ) {
        self.story_stealth = StoryStealthMode {
            active_until_date,
            cooldown_until_date,
        };
    }
}
