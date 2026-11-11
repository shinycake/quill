//! Story tray and story interaction state.
use super::*;

/// Most story-reaction custom emoji stickers kept per session.
pub const STORY_CUSTOM_EMOJI_CAP: usize = 512;

impl Session {
    /// Phase 9.1: insert or drop a story-tray entry. Every chat with
    /// active stories is kept (the chat list paints a ring on its
    /// avatar whichever story list it is in, like tdesktop's
    /// `hasActiveStories`); only the main list shows in the tray, see
    /// `ordered_story_tray`.
    pub(crate) fn upsert_story_tray_entry(&mut self, entry: ChatActiveStoriesView) {
        if entry.stories.is_empty() {
            self.stories.tray.remove(&entry.chat_id);
        } else {
            self.stories.tray.insert(entry.chat_id, entry);
        }
    }

    /// The story ring for a chat's avatar in the chat list.
    pub fn chat_story_ring(&self, chat_id: i64) -> Option<crate::story_ring::StoryRing> {
        crate::story_ring::StoryRing::from_active(self.stories.tray.get(&chat_id)?)
    }

    /// Phase 9.7: start tracking a story-page mutation (`Sending`) sent as
    /// `request`.
    pub fn begin_story_page_op(&mut self, label: impl Into<String>, request: RequestId) {
        self.track_story_page_op(label.into(), StoryPageOpState::Sending, request);
    }

    /// Phase 9.7: start tracking a story-page read (`Checking`) sent as
    /// `request`.
    pub fn begin_story_page_check(&mut self, label: impl Into<String>, request: RequestId) {
        self.track_story_page_op(label.into(), StoryPageOpState::Checking, request);
    }

    /// The chat page comes from the pending request the op tracks.
    fn track_story_page_op(&mut self, label: String, state: StoryPageOpState, request: RequestId) {
        let chat_id = self
            .requests
            .get(request)
            .and_then(|pending| pending.chat_id)
            .map(|chat| chat.0);
        self.stories.page_ops.begin(StoryPageOp {
            label,
            state,
            request,
            chat_id,
        });
    }

    /// The driver could not send `request`: its op fails with `reason`.
    pub fn fail_story_page_send(
        &mut self,
        label: String,
        request: RequestId,
        chat_id: Option<ChatId>,
        reason: String,
    ) {
        self.stories.page_ops.begin(StoryPageOp {
            label,
            state: StoryPageOpState::Failed(reason),
            request,
            chat_id: chat_id.map(|chat| chat.0),
        });
    }

    /// The op tracking the call `pending` answers, if it is still tracked:
    /// an answer settles the op of its own request only, whatever else the
    /// page has in flight.
    fn story_page_op_for(&mut self, pending: Option<&PendingRequest>) -> Option<&mut StoryPageOp> {
        self.stories.page_ops.get_mut(pending?.id)
    }

    /// Phase 9.7: mark the story-page op `Succeeded` when its answer lands.
    pub(crate) fn succeed_story_page_op(&mut self, pending: Option<&PendingRequest>) {
        if let Some(op) = self.story_page_op_for(pending) {
            op.state = StoryPageOpState::Succeeded;
        }
    }

    /// Phase 9.7: mark the story-page op `Failed` on a TDLib error.
    pub(crate) fn fail_story_page_op(&mut self, pending: Option<&PendingRequest>, reason: String) {
        if let Some(op) = self.story_page_op_for(pending) {
            op.state = StoryPageOpState::Failed(reason);
        }
    }

    /// The story or page a finished op acted on was left: its "Done" or
    /// "Failed" line goes, so it never shows under another story or
    /// another chat's page. An op still in flight keeps its line until its
    /// answer.
    pub fn settle_story_page_op(&mut self) {
        self.stories.page_ops.settle_finished();
    }

    /// The story page of `chat_id` was closed: its calls are no longer
    /// tracked, in flight or finished.
    pub fn leave_story_page(&mut self, chat_id: i64) {
        self.stories.page_ops.forget_chat(chat_id);
    }

    /// Phase 9.7: clear the story-page op once its load answer landed (the
    /// loaded list itself is the honest state — no status line needed).
    pub(crate) fn clear_story_page_op(&mut self, pending: Option<&PendingRequest>) {
        if let Some(pending) = pending {
            self.stories.page_ops.remove(pending.id);
        }
    }

    /// Phase 9.1: tray entries for the story row above the chat list:
    /// main-list entries only, sorted by `(order, chat_id)` descending
    /// (schema `chatActiveStories` comment, line 6781).
    pub fn ordered_story_tray(&self) -> Vec<&ChatActiveStoriesView> {
        self.ordered_story_list(StoryListView::Main)
    }

    /// One story list as the viewer's walk sees it: each poster and whether
    /// it has unread stories, in tray order.
    pub fn story_walk_sources(
        &self,
        list: StoryListView,
    ) -> Vec<crate::story_tray_walk::TraySource> {
        self.ordered_story_list(list)
            .into_iter()
            .map(crate::story_tray_walk::TraySource::of)
            .collect()
    }

    /// The posters in one story list (main or archive), in tray order.
    pub fn ordered_story_list(&self, list: StoryListView) -> Vec<&ChatActiveStoriesView> {
        let mut entries: Vec<&ChatActiveStoriesView> = self
            .stories
            .tray
            .values()
            .filter(|entry| entry.list == Some(list))
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse((entry.order, entry.chat_id)));
        entries
    }

    /// Phase 9.5: (re)start the viewers panel for a story — resets rows
    /// and pagination when the story changes, keeps them when the same
    /// story is re-opened.
    pub fn begin_story_viewers(&mut self, chat_id: i64, story_id: i32) {
        let same = self
            .stories
            .viewers
            .as_ref()
            .is_some_and(|state| state.chat_id == chat_id && state.story_id == story_id);
        if !same {
            self.stories.viewers = Some(StoryViewersState {
                chat_id,
                story_id,
                ..Default::default()
            });
        }
    }

    pub fn clear_story_viewers(&mut self) {
        self.stories.viewers = None;
    }

    /// The driver sent the viewers page `request` for the story's panel.
    pub fn story_viewers_loading(&mut self, chat_id: i64, story_id: i32, request: RequestId) {
        if let Some(state) = self.stories.viewers.as_mut()
            && state.chat_id == chat_id
            && state.story_id == story_id
        {
            state.page = ViewersPage::Loading { request };
        }
    }

    /// The panel the answer to `pending` belongs to: the one that is still
    /// loading exactly that request.
    fn story_viewers_asking(&mut self, pending: &PendingRequest) -> Option<&mut StoryViewersState> {
        self.stories.viewers.as_mut().filter(|state| {
            state.page
                == ViewersPage::Loading {
                    request: pending.id,
                }
        })
    }

    /// Phase 9.2+: store `getCustomEmojiStickers` answers for the story
    /// reaction picker — keyed by sticker id (= custom emoji id).
    pub fn accept_story_custom_emoji_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stories.custom_emoji_retry = None;
        for sticker in stickers {
            // Bounded: past the cap the cache restarts empty; the story
            // viewer refetches the ids it still shows.
            if self.stories.custom_emoji_stickers.len() >= STORY_CUSTOM_EMOJI_CAP
                && !self.stories.custom_emoji_stickers.contains_key(&sticker.id)
            {
                self.stories.custom_emoji_stickers.clear();
            }
            self.stories
                .custom_emoji_stickers
                .insert(sticker.id, sticker);
        }
    }

    /// Phase 9.5: accumulate one `storyInteractions` page into the
    /// viewers panel. Only the panel that asked for this very request takes
    /// it: a page for another story, or one asked by a panel since closed
    /// and reopened, is dropped.
    pub fn accept_story_interactions(
        &mut self,
        pending: &PendingRequest,
        view: StoryInteractionsView,
    ) {
        let Some(state) = self.story_viewers_asking(pending) else {
            return;
        };
        state.page = ViewersPage::Idle;
        state.total_count = view.total_count;
        state.next_offset = view.next_offset;
        state.rows.extend(view.interactions);
    }

    /// Phase 9.5: mark the viewers fetch as failed; the panel shows the
    /// error with a retry instead of spinning forever.
    pub fn fail_story_viewers(&mut self, pending: &PendingRequest, message: String) {
        if let Some(state) = self.story_viewers_asking(pending) {
            state.page = ViewersPage::Failed(message);
        }
    }

    /// Phase 9.5: the first `reportStory` of a story's flow went out as
    /// `request` — the UI renders `Checking` until that answer lands.
    pub fn begin_story_report(&mut self, chat_id: i64, story_id: i32, request: RequestId) {
        self.stories.report = Some(StoryReportFlow {
            chat_id,
            story_id,
            stage: StoryReportStage::Checking { request },
        });
    }

    pub fn clear_story_report(&mut self) {
        self.stories.report = None;
    }

    /// The flow the answer to `pending` belongs to: the one that waits for
    /// exactly that request. An answer to an earlier request of the same
    /// story (the box was closed and reopened, the step was repeated) or to
    /// a story left behind moves nothing.
    fn story_report_asking(&mut self, pending: &PendingRequest) -> Option<&mut StoryReportFlow> {
        self.stories
            .report
            .as_mut()
            .filter(|flow| flow.stage.waiting_for() == Some(pending.id))
    }

    /// Phase 9.5: apply a `ReportStoryResult` answer. Mirrors TDLib's
    /// `ReportStoryQuery` mapping (`td/telegram/StoryManager.cpp:1406-1430`
    /// @d1085f9): an empty option list is a success, not a picker.
    pub fn accept_story_report(&mut self, pending: &PendingRequest, result: ReportStoryResult) {
        let Some(flow) = self.story_report_asking(pending) else {
            return;
        };
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
    /// submitted) went out as `request`.
    pub fn story_report_sending(&mut self, chat_id: i64, story_id: i32, request: RequestId) {
        if let Some(flow) = self.stories.report.as_mut()
            && flow.chat_id == chat_id
            && flow.story_id == story_id
        {
            flow.stage = StoryReportStage::Sending { request };
        }
    }

    /// Phase 9.5: a raw TDLib error on a `reportStory` request ends the
    /// flow with the sanitized message (there is no `Failed` result
    /// variant — errors arrive as `error` answers).
    pub fn fail_story_report(&mut self, pending: &PendingRequest, message: String) {
        if let Some(flow) = self.story_report_asking(pending) {
            flow.stage = StoryReportStage::Failed(message);
        }
    }

    /// Phase 9.5: `reportStory` failed to send at all — the driver took
    /// the pending request back, so no answer will ever arrive. The story's
    /// flow ends with the error instead of spinning on `Checking`/`Sending`
    /// forever (and is made when the first request never went out).
    pub fn fail_story_report_send(&mut self, chat_id: i64, story_id: i32, message: String) {
        self.stories.report = Some(StoryReportFlow {
            chat_id,
            story_id,
            stage: StoryReportStage::Failed(message),
        });
    }

    /// Phase 9.5: store the latest `updateStoryStealthMode` state.
    pub fn apply_update_story_stealth_mode(
        &mut self,
        active_until_date: i32,
        cooldown_until_date: i32,
    ) {
        self.stories.stealth = StoryStealthMode {
            active_until_date,
            cooldown_until_date,
        };
    }
}
