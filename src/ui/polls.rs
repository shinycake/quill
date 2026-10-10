//! poll dialogs, voting, voters.

use super::app::QuillApp;
use super::scheduled::poll_deadline_picker;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::date_picker::DatePicker;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::poll::{POLL_OPTIONS_MAX, POLL_OPTIONS_MIN, chat_allows_polls, voter_count_label};
use quill::state::{PollStatsFetch, PollVotersFetch, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{MessageContent, MessageSender};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
pub(super) fn apply_ready_poll(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 15;
    let chat_json = format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Demo polls","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0,"unread_poll_vote_count":3}}}}"#
    );
    let position_json = format!(
        r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
    );
    for json in [chat_json, position_json] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));

    let formatted = |text: &str| -> String {
        let text_json = serde_json::to_string(text).unwrap();
        format!(r#"{{"@type":"formattedText","text":{text_json},"entities":[]}}"#)
    };
    let option = |id: &str, text: &str, voter_count: i32, vote_percentage: i32, is_chosen: bool| {
        let text_json = formatted(text);
        format!(
            r#"{{"@type":"pollOption","id":"{id}","text":{text_json},"voter_count":{voter_count},"vote_percentage":{vote_percentage},"is_chosen":{is_chosen}}}"#
        )
    };
    let poll_message = |message_id: i32,
                        poll_id: i64,
                        question: &str,
                        options: &str,
                        total_voter_count: i32,
                        is_anonymous: bool,
                        allows_multiple_answers: bool,
                        allows_revoting: bool,
                        is_closed: bool,
                        poll_type: &str,
                        vote_restriction_reason: &str| {
        let question_json = formatted(question);
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messagePoll","poll":{{"@type":"poll","id":{poll_id},"question":{question_json},"options":[{options}],"total_voter_count":{total_voter_count},"is_anonymous":{is_anonymous},"allows_multiple_answers":{allows_multiple_answers},"allows_revoting":{allows_revoting},"is_closed":{is_closed},"vote_restriction_reason":{vote_restriction_reason},"type":{poll_type}}},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}}}}}"#
        )
    };

    // Open regular poll: the user voted for "Sushi place" (is_chosen).
    let open_options = [
        option("opt-sushi", "Sushi place", 12, 55, true),
        option("opt-pizza", "Pizza", 7, 32, false),
        option("opt-tacos", "Tacos", 3, 13, false),
    ]
    .join(",");
    let open_poll = poll_message(
        106,
        9001,
        "Where should we eat lunch?",
        &open_options,
        22,
        true,
        false,
        true,
        false,
        r#"{"@type":"pollTypeRegular"}"#,
        "null",
    );

    // Closed quiz poll: correct answer is "Mars" (index 0), user answered
    // "Venus" (is_chosen) — results only, no voting affordance. Non-empty
    // explanation exercises the B4 quiz-explanation rendering.
    let closed_options = [
        option("opt-mars", "Mars", 18, 72, false),
        option("opt-venus", "Venus", 7, 28, true),
    ]
    .join(",");
    let closed_poll = poll_message(
        107,
        9002,
        "Which planet is known as the Red Planet?",
        &closed_options,
        25,
        true,
        false,
        false,
        true,
        r#"{"@type":"pollTypeQuiz","correct_option_ids":[0],"explanation":{"@type":"formattedText","text":"Mars looks red because iron oxide — rust — coats its surface.","entities":[]}}"#,
        "null",
    );

    // Restricted poll: the server reports a vote-restriction reason, so the
    // B4 restriction label renders instead of a dead tap.
    let restricted_options = [
        option("opt-rust", "Rust", 9, 60, false),
        option("opt-go", "Go", 6, 40, false),
    ]
    .join(",");
    let restricted_poll = poll_message(
        108,
        9003,
        "Best systems language?",
        &restricted_options,
        15,
        true,
        false,
        false,
        false,
        r#"{"@type":"pollTypeRegular"}"#,
        r#"{"@type":"pollVoteRestrictionReasonMembershipRequired","chat_id":15}"#,
    );

    // B15: an open poll that lets participants add options, with a
    // deadline and a subscribers-only note.
    let addable_options = [
        option("opt-tea", "Tea", 4, 40, false),
        option("opt-coffee", "Coffee", 6, 60, false),
    ]
    .join(",");
    let addable_question = formatted("Which drink should we stock?");
    let close_date = quill::local_time::now_unix() + 3 * 3600 + 20 * 60;
    let addable_poll = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":109,"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messagePoll","poll":{{"@type":"poll","id":9004,"question":{addable_question},"options":[{addable_options}],"total_voter_count":10,"is_anonymous":false,"allows_multiple_answers":true,"allows_revoting":true,"members_only":true,"close_date":{close_date},"can_see_results":true,"is_closed":false,"vote_restriction_reason":null,"type":{{"@type":"pollTypeRegular"}}}},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":true}}}}}}"#
    );
    // B15: results hidden until the poll closes.
    let hidden_options = [
        option("opt-yes", "Yes", 0, 0, false),
        option("opt-no", "No", 0, 0, false),
    ]
    .join(",");
    let hidden_question = formatted("Ship it on Friday?");
    let hidden_poll = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":110,"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messagePoll","poll":{{"@type":"poll","id":9005,"question":{hidden_question},"options":[{hidden_options}],"total_voter_count":8,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"can_see_results":false,"is_closed":false,"vote_restriction_reason":null,"type":{{"@type":"pollTypeRegular"}}}},"description":{{"@type":"formattedText","text":"","entities":[]}},"can_add_option":false}}}}}}"#
    );
    // B15: a group checklist — one task done by a teammate, one open.
    let checklist = |message_id: i32, title: &str, tasks: &str| {
        let title_json = formatted(title);
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messageChecklist","list":{{"@type":"checklist","title":{title_json},"tasks":[{tasks}],"others_can_add_tasks":true,"can_add_tasks":true,"others_can_mark_tasks_as_done":true,"can_mark_tasks_as_done":true}}}}}}}}"#
        )
    };
    let task = |id: i32, text: &str, done_by: Option<i64>| {
        let text_json = formatted(text);
        match done_by {
            Some(user) => format!(
                r#"{{"@type":"checklistTask","id":{id},"text":{text_json},"completed_by":{{"@type":"messageSenderUser","user_id":{user}}},"completion_date":1700000000}}"#
            ),
            None => format!(
                r#"{{"@type":"checklistTask","id":{id},"text":{text_json},"completed_by":null,"completion_date":0}}"#
            ),
        }
    };
    let trip_tasks = [
        task(1, "Book flights", Some(chat_id)),
        task(2, "Reserve the hotel", None),
        task(3, "Pack passports", None),
    ]
    .join(",");
    let trip_checklist = checklist(111, "Weekend trip", &trip_tasks);

    for json in [
        open_poll,
        closed_poll,
        restricted_poll,
        addable_poll,
        hidden_poll,
        trip_checklist,
    ] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    /// B4: open the "Stop poll" / "Stop quiz" confirm banner. TGX
    /// `StopPollWarn` / `StopQuizWarn`: nobody can vote afterwards and
    /// the action can't be undone.
    pub(super) fn begin_stop_poll(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        is_quiz: bool,
        cx: &mut Context<Self>,
    ) {
        self.pending_stop_poll = Some((chat_id, message_id, is_quiz));
        self.status_note = "confirm stop poll".into();
        cx.notify();
    }

    pub(super) fn cancel_stop_poll(&mut self, cx: &mut Context<Self>) {
        self.pending_stop_poll = None;
        self.status_note = "stop cancelled".into();
        cx.notify();
    }

    /// B4: confirm `stopPoll` (schema 1.8.67 line 12953). The closed
    /// poll arrives via `updatePoll`; demo mode flips it locally.
    pub(super) fn confirm_stop_poll(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, message_id, _)) = self.pending_stop_poll.take() else {
            return;
        };
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .stop_poll(chat_id, message_id);
            self.status_note = match result {
                Ok(_) => "stopping poll…".into(),
                Err(_) => "could not stop poll".into(),
            };
        } else if self.demo_session.is_some() {
            if let Some(history) = self
                .demo_session
                .as_mut()
                .and_then(|session| session.histories.get_mut(&chat_id.0))
                && let Some(message) = history.messages.get_mut(&message_id.0)
                && let MessageContent::Poll(poll_content) = &mut message.content
            {
                poll_content.poll.is_closed = true;
            }
            self.status_note = "poll stopped (demo)".into();
        }
        cx.notify();
    }

    /// B4: open the poll voter-list viewer. Live: fetches the first
    /// option's page (`getPollVoters`, schema 1.8.67 line 12941). Demo
    /// sessions have no TDLib — an honest note instead of a fake list.
    pub(super) fn open_poll_voters_dialog(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if self.demo_session.is_some() {
            self.status_note = "voter lists are unavailable in demo mode".into();
            cx.notify();
            return;
        }
        self.poll_voters_dialog = Some(PollVotersDialog {
            chat_id,
            message_id,
            selected_option: None,
            show_stats: false,
        });
        self.select_poll_voters_option(0, cx);
    }

    /// B15: "Poll Stats" (tdesktop `lng_polls_stats_title`): the vote
    /// graph from `getPollVoteStatistics` plus the per-option voter lists
    /// (with "Show more") when `getPollVoters` is available.
    pub(super) fn open_poll_stats_dialog(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if self.demo_session.is_some() {
            self.status_note = "poll stats are unavailable in demo mode".into();
            cx.notify();
            return;
        }
        let is_dark = super::chat_theme::is_dark_palette();
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .fetch_poll_vote_statistics(chat_id, message_id, is_dark)
                .is_err()
        {
            self.status_note = "could not load poll stats".into();
            cx.notify();
            return;
        }
        self.poll_voters_dialog = Some(PollVotersDialog {
            chat_id,
            message_id,
            selected_option: None,
            show_stats: true,
        });
        let can_get_voters = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| match &message.content {
                MessageContent::Poll(content) => content.poll.can_get_voters,
                _ => false,
            });
        if can_get_voters {
            self.select_poll_voters_option(0, cx);
        }
        cx.notify();
    }

    pub(super) fn close_poll_voters_dialog(&mut self, cx: &mut Context<Self>) {
        self.poll_voters_dialog = None;
        cx.notify();
    }

    /// B4: switch the voters dialog to another option (first page).
    pub(super) fn select_poll_voters_option(
        &mut self,
        option_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.poll_voters_dialog.as_mut() else {
            return;
        };
        dialog.selected_option = Some(option_index);
        let (chat_id, message_id) = (dialog.chat_id, dialog.message_id);
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .fetch_poll_voters(chat_id, message_id, option_index)
                .is_err()
        {
            self.status_note = "could not load voters".into();
        }
        cx.notify();
    }

    /// B4: next `getPollVoters` page for the dialog's selected option.
    pub(super) fn load_more_poll_voters(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.poll_voters_dialog.as_ref() else {
            return;
        };
        let (chat_id, message_id) = (dialog.chat_id, dialog.message_id);
        let option_index = dialog.selected_option.unwrap_or(0);
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .load_more_poll_voters(chat_id, message_id, option_index)
                .is_err()
        {
            self.status_note = "could not load more voters".into();
        }
        cx.notify();
    }

    /// B4: display name for a poll voter (same resolution as the event
    /// log's sender names).
    pub(super) fn poll_voter_name(&self, sender: &MessageSender) -> String {
        match sender {
            MessageSender::User { user_id } => self
                .session()
                .and_then(|session| session.user(*user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| format!("User {user_id}")),
            MessageSender::Chat { chat_id } => self
                .session()
                .and_then(|session| session.chats.get(chat_id))
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| format!("Chat {chat_id}")),
        }
    }

    /// B4: the poll voters dialog body — option rows, then the selected
    /// option's voters with a "Load more" button while the server
    /// reports more than loaded.
    pub(super) fn poll_voters_dialog_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.poll_voters_dialog.as_ref() else {
            return div().into_any_element();
        };
        let (chat_id, message_id) = (dialog.chat_id, dialog.message_id);
        let selected = dialog.selected_option;
        let poll = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| match &message.content {
                MessageContent::Poll(poll) => Some(poll.poll.clone()),
                _ => None,
            });
        let mut body = div().flex().flex_col().gap_1();
        if dialog.show_stats {
            body = body.child(self.poll_stats_section(chat_id, message_id, cx));
        }
        if let Some(poll) = poll.as_ref().filter(|poll| poll.can_get_voters) {
            for (index, option) in poll.options.iter().enumerate() {
                let label = format!(
                    "{} · {}",
                    option.text,
                    voter_count_label(option.voter_count)
                );
                let is_selected = selected == Some(index);
                body = body.child(
                    Button::new(format!("poll-voters-option-{index}"))
                        .label(label)
                        .ghost()
                        .text_color(if is_selected { accent() } else { text_menu() })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_poll_voters_option(index, cx);
                        })),
                );
            }
        }
        let fetch = selected.and_then(|index| {
            self.session()
                .and_then(|session| {
                    session
                        .poll_voters
                        .get(&(chat_id.0, message_id.0, index as i32))
                })
                .cloned()
        });
        body = match fetch {
            // Stats-only view without a voters list (anonymous polls).
            None if poll.as_ref().is_none_or(|poll| !poll.can_get_voters) => body,
            None => body.child(
                div()
                    .text_xs()
                    .text_color(text_muted())
                    .child("Select an option to see its voters."),
            ),
            Some(PollVotersFetch::Loading) => body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(text_muted())
                    .child(Spinner::new().small())
                    .child("Loading voters…"),
            ),
            Some(PollVotersFetch::Failed(reason)) => body
                .child(div().text_xs().text_color(danger()).child(reason))
                .child(
                    Button::new("poll-voters-retry")
                        .label("Retry")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let index = this
                                .poll_voters_dialog
                                .as_ref()
                                .and_then(|dialog| dialog.selected_option)
                                .unwrap_or(0);
                            this.select_poll_voters_option(index, cx);
                        })),
                ),
            Some(PollVotersFetch::Loaded {
                voters,
                total_count,
            }) => {
                let mut list = body;
                if voters.is_empty() {
                    list = list.child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child("No voters yet."),
                    );
                }
                for voter in &voters {
                    list = list.child(
                        div()
                            .text_sm()
                            .text_color(text_menu())
                            .child(self.poll_voter_name(voter)),
                    );
                }
                if voters.len() < total_count as usize {
                    // tdesktop: `lng_polls_show_more` — "Show more (N)".
                    let left = total_count as usize - voters.len();
                    list = list.child(
                        Button::new("poll-voters-more")
                            .label(format!("Show more ({left})"))
                            .ghost()
                            .text_color(accent())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.load_more_poll_voters(cx);
                            })),
                    );
                }
                list
            }
        };
        body.into_any_element()
    }

    /// B15: the "Votes" graph row (or its loading / error state) at the top
    /// of the poll stats dialog.
    fn poll_stats_section(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fetch = self
            .session()
            .and_then(|session| session.poll_stats.get(&(chat_id.0, message_id.0)))
            .cloned();
        let mut section = div().flex().flex_col().gap_1().pb_2();
        section = match fetch {
            None | Some(PollStatsFetch::Loading) => section.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(text_muted())
                    .child(Spinner::new().small())
                    .child("Loading stats…"),
            ),
            Some(PollStatsFetch::Failed(reason)) => section
                .child(div().text_xs().text_color(danger()).child(reason))
                .child(
                    Button::new("poll-stats-retry")
                        .label("Retry")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_poll_stats_dialog(chat_id, message_id, cx);
                        })),
                ),
            Some(PollStatsFetch::Loaded(graph)) => {
                match super::statistics::stats_graph_row("Votes over time", &graph, cx) {
                    Some(row) => section.child(row),
                    None => section.child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child("No vote statistics yet."),
                    ),
                }
            }
        };
        section.into_any_element()
    }

    /// Phase 4.2: poll option tap → `setPollAnswer` through the live driver
    /// (same guard style as the other send methods). In screenshot demos the
    /// tap flips the chosen marks locally (no live Telegram); the fixture
    /// already carries voted counts.
    pub(super) fn vote_on_poll(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = live
                .driver
                .send_poll_answer(chat_id, message_id, option_index);
            self.status_note = match result {
                Ok(_) => "voting…".into(),
                Err(_) => "could not vote".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_poll_vote(chat_id, message_id, option_index);
            self.status_note = "vote updated (demo)".into();
            cx.notify();
        }
    }

    /// The menu's "Retract vote": `setPollAnswer` with no options.
    pub(super) fn retract_poll_vote(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.retract_poll_vote(chat_id, message_id) {
                Ok(_) => "vote retracted".into(),
                Err(_) => "could not retract the vote".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut()
            && let Some(message) = session
                .histories
                .get_mut(&chat_id.0)
                .and_then(|history| history.messages.get_mut(&message_id.0))
            && let MessageContent::Poll(poll_content) = &mut message.content
            && quill::poll::can_retract_vote(&poll_content.poll)
        {
            for option in &mut poll_content.poll.options {
                option.is_chosen = false;
            }
            self.status_note = "vote retracted (demo)".into();
        }
        cx.notify();
    }

    /// Phase 4.2: open the poll creation dialog above the composer.
    pub(super) fn open_poll_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // B4: defensive gate — the composer hides the Poll entry when
        // `chatPermissions.can_send_polls` is false (schema 1.8.67 line
        // 1070); this refuses a stale race (permissions changed after
        // the entry rendered).
        let polls_allowed = self.session().and_then(|s| s.open_chat).is_none_or(|id| {
            self.session()
                .and_then(|s| s.chats.get(&id.0))
                .is_none_or(|chat| chat_allows_polls(chat.permissions.as_ref()))
        });
        if !polls_allowed {
            self.status_note = "polls are restricted in this chat".into();
            cx.notify();
            return;
        }
        self.poll_dialog = Some(PollDialog::new(window, cx));
        if let Some(dialog) = &self.poll_dialog {
            dialog
                .question_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    pub(super) fn close_poll_dialog(&mut self, cx: &mut Context<Self>) {
        self.poll_dialog = None;
        cx.notify();
    }

    /// Slice B3: Cancel/Esc on the poll dialog. With unsent input this arms
    /// the inline discard confirmation instead of closing silently.
    pub(super) fn request_close_poll_dialog(&mut self, cx: &mut Context<Self>) {
        let dirty = self
            .poll_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.is_dirty(cx));
        if dirty {
            if let Some(dialog) = self.poll_dialog.as_mut() {
                dialog.confirming_discard = true;
            }
            cx.notify();
        } else {
            self.close_poll_dialog(cx);
        }
    }

    // ------------------------------------------------------------------
    // Phase 6: Contacts tab, info panels, add-contact dialog.
    // ------------------------------------------------------------------

    /// kit Phase 2 (redo): poll voters hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_poll_voters_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PollVoters, |this, _, cx| {
                this.close_poll_voters_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let body = this.poll_voters_dialog_body(cx);
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(
                    if this
                        .poll_voters_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.show_stats)
                    {
                        "Poll Stats"
                    } else {
                        "Poll voters"
                    },
                ))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    pub(super) fn add_poll_option_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.poll_dialog.as_mut() else {
            return;
        };
        if dialog.option_inputs.len() >= POLL_OPTIONS_MAX {
            return;
        }
        let index = dialog.option_inputs.len();
        dialog.option_inputs.push(cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(format!("Option {}", index + 1))
                .auto_grow(1, 2)
                .submit_on_enter(false)
        }));
        cx.notify();
    }

    pub(super) fn remove_poll_option_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(dialog) = self.poll_dialog.as_mut() else {
            return;
        };
        if dialog.option_inputs.len() <= POLL_OPTIONS_MIN || index >= dialog.option_inputs.len() {
            return;
        }
        dialog.option_inputs.remove(index);
        // Keep the marked correct-answer row pointing at the same option.
        if let Some(marked) = dialog.quiz_correct_row {
            dialog.quiz_correct_row = if marked == index {
                None
            } else if marked > index {
                Some(marked - 1)
            } else {
                Some(marked)
            };
        }
        cx.notify();
    }

    /// Phase 4.2: freeze the dialog, validate, and send `inputMessagePoll`
    /// through the driver (same `sendMessage` path as the composer). The
    /// pending reply (if any) is attached like a normal send.
    pub(super) fn submit_poll_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = match self.poll_dialog.as_ref() {
            Some(dialog) => dialog.draft(cx),
            None => return,
        };
        if let Some(reason) = draft.validate() {
            self.status_note = reason.to_string();
            cx.notify();
            return;
        }
        let plan = self.session().and_then(|session| {
            let chat_id = session.open_chat?;
            let chat = session.chats.get(&chat_id.0)?;
            chat.can_post().then_some(chat_id)
        });
        let Some(chat_id) = plan else {
            self.status_note = "select a chat to send".into();
            cx.notify();
            return;
        };
        // Phase A1: slow-mode gate applies to polls too.
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        // Slice G1: the poll carries the composer's quote, if any.
        let reply_to = self
            .pending_reply
            .as_ref()
            .map(|reply| quill::telegram::SendReply {
                message_id: reply.message_id,
                quote: reply
                    .quote
                    .as_ref()
                    .map(|quote| (quote.text.clone(), quote.position)),
            });
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.send_poll_draft(chat_id, &draft, reply_to);
            match result {
                Ok(_) => {
                    self.poll_dialog = None;
                    self.pending_reply = None;
                    self.status_note = "sending poll…".into();
                }
                Err(_) => {
                    self.status_note = "could not send poll".into();
                }
            }
            cx.notify();
            return;
        }
        // Screenshot demos have no live driver; close the dialog honestly.
        let _ = window;
        self.poll_dialog = None;
        self.status_note = "polls need a live connection (demo)".into();
        cx.notify();
    }

    /// B4: "Stop poll" / "Stop quiz" confirm banner, styled like the
    /// delete confirm. TGX `StopPollWarn` / `StopQuizWarn`: nobody can
    /// vote afterwards, and the action can't be undone.
    pub(super) fn stop_poll_confirm_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_quiz = self
            .pending_stop_poll
            .is_some_and(|(_, _, is_quiz)| is_quiz);
        let title = if is_quiz {
            "Stop this quiz?"
        } else {
            "Stop this poll?"
        };
        let warning = if is_quiz {
            "If you stop this quiz now, nobody will be able to participate in it anymore.\n\nThis action cannot be undone."
        } else {
            "If you stop this poll now, nobody will be able to vote in it anymore.\n\nThis action cannot be undone."
        };
        div()
            .id("stop-poll-confirm")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(danger())
            .bg(danger_bg())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(danger())
                            .child(title),
                    )
                    .child(div().text_sm().text_color(text_primary()).child(warning)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("cancel-stop-poll")
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_stop_poll(cx);
                            })),
                    )
                    .child(
                        Button::new("confirm-stop-poll")
                            .label(if is_quiz { "Stop quiz" } else { "Stop poll" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_stop_poll(cx);
                            })),
                    ),
            )
    }

    /// Phase 4.2: the poll creation dialog, rendered above the composer.
    /// Question field, description field, dynamic option rows (2–10),
    /// quiz mode (correct-option radios + explanation), duration / country
    /// fields, anonymous / multiple-answers / revoting / shuffle / quiz
    /// toggles, discard confirmation, Create / Cancel.
    pub(super) fn poll_dialog_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.poll_dialog.as_ref()?;
        let mut panel = div()
            .id("poll-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(div().text_sm().font_semibold().child(if dialog.is_quiz {
                "New quiz"
            } else {
                "New poll"
            }))
            .child(
                Textarea::new(&dialog.description_input)
                    .aria_label("Poll description")
                    .h(px(40.)),
            )
            .child(
                Textarea::new(&dialog.question_input)
                    .aria_label("Poll question")
                    .h(px(64.)),
            );
        let is_quiz = dialog.is_quiz;
        let quiz_correct_row = dialog.quiz_correct_row;
        for (index, input) in dialog.option_inputs.iter().enumerate() {
            let mut row = div()
                .id(("poll-dialog-option", index as u64))
                .flex()
                .items_center()
                .gap_2();
            if is_quiz {
                let marked = quiz_correct_row == Some(index);
                row = row.child(
                    // Phase 6: kit Checkbox, not Radio — a quiz correct answer
                    // is toggleable (clicking the marked option clears it),
                    // and kit Radio cannot deselect itself.
                    Checkbox::new(format!("poll-quiz-correct-{index}"))
                        .checked(marked)
                        .accessibility_label(format!(
                            "Mark option {} as the correct answer",
                            index + 1
                        ))
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if let Some(dialog) = this.poll_dialog.as_mut() {
                                dialog.quiz_correct_row = if on { Some(index) } else { None };
                            }
                            cx.notify();
                        })),
                );
            }
            row = row.child(
                div()
                    .flex_1()
                    .child(Textarea::new(input).aria_label("Poll answer").h(px(40.))),
            );
            if dialog.option_inputs.len() > POLL_OPTIONS_MIN {
                row = row.child(
                    Button::new(format!("poll-remove-option-{index}"))
                        .label("✕")
                        .ghost()
                        .tooltip("Remove option")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_poll_option_row(index, cx);
                        })),
                );
            }
            panel = panel.child(row);
        }
        if dialog.option_inputs.len() < POLL_OPTIONS_MAX {
            panel = panel.child(
                Button::new("poll-add-option")
                    .label("Add option")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_poll_option_row(window, cx);
                    })),
            );
        }
        if is_quiz {
            panel = panel.child(
                Textarea::new(&dialog.explanation_input)
                    .aria_label("Quiz explanation")
                    .h(px(64.)),
            );
        }
        panel = panel
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(match &dialog.deadline {
                        // B15: an absolute deadline replaces the hours field.
                        Some(picker) => div()
                            .flex_1()
                            .child(DatePicker::new(picker).placeholder("Pick a deadline")),
                        None => div().flex_1().child(
                            Textarea::new(&dialog.duration_input)
                                .aria_label("Poll open duration in hours")
                                .h(px(40.)),
                        ),
                    })
                    .child(
                        div().flex_1().child(
                            Textarea::new(&dialog.countries_input)
                                .aria_label("Poll country codes")
                                .h(px(40.)),
                        ),
                    ),
            )
            .child(self.poll_toggle_row(cx));
        if dialog.confirming_discard {
            panel =
                panel.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().child("Discard this poll?"))
                        .child(Button::new("poll-discard-yes").label("Discard").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.close_poll_dialog(cx);
                            }),
                        ))
                        .child(
                            Button::new("poll-discard-no")
                                .label("Keep editing")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(dialog) = this.poll_dialog.as_mut() {
                                        dialog.confirming_discard = false;
                                    }
                                    cx.notify();
                                })),
                        ),
                );
        }
        panel =
            panel.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("poll-create")
                            .label("Create poll")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_poll_dialog(window, cx);
                            })),
                    )
                    .child(Button::new("poll-cancel").label("Cancel").ghost().on_click(
                        cx.listener(|this, _, _, cx| {
                            this.request_close_poll_dialog(cx);
                        }),
                    )),
            );
        Some(panel.into_any_element())
    }

    /// Slice B3: the poll dialog's toggle row. Quiz mode forces
    /// single-answer and no revoting, mirroring Telegram X's
    /// `CreatePollController` quiz toggle.
    pub(super) fn poll_toggle_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (is_quiz, anonymous, multiple, revoting, shuffle) = match &self.poll_dialog {
            Some(dialog) => (
                dialog.is_quiz,
                dialog.is_anonymous,
                dialog.allows_multiple_answers,
                dialog.allows_revoting,
                dialog.shuffle_options,
            ),
            None => (false, true, false, true, false),
        };
        let (add_options, hide_results, subscribers, has_deadline) = match &self.poll_dialog {
            Some(dialog) => (
                dialog.allow_adding_options,
                dialog.hide_results_until_closes,
                dialog.members_only,
                dialog.deadline.is_some(),
            ),
            None => (false, false, false, false),
        };
        // tdesktop offers "Restrict to Subscribers" in broadcast channels only.
        let in_channel = self
            .session()
            .and_then(|s| s.open_chat.and_then(|id| s.chats.get(&id.0)))
            .is_some_and(|chat| chat.is_channel());
        // Phase 6: kit Checkbox (was: ghost buttons with ☑/☐ labels).
        // Controlled: the requested value is written, not flipped.
        let checkbox =
            |id: &'static str, label: &'static str, on: bool, set: fn(&mut PollDialog, bool)| {
                Checkbox::new(id)
                    .checked(on)
                    .label(label)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        if let Some(dialog) = this.poll_dialog.as_mut() {
                            set(dialog, on);
                        }
                        cx.notify();
                    }))
            };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(checkbox(
                        "poll-toggle-quiz",
                        "Quiz mode",
                        is_quiz,
                        |dialog, on| {
                            dialog.is_quiz = on;
                            if on {
                                dialog.allows_multiple_answers = false;
                                dialog.allows_revoting = false;
                            }
                        },
                    ))
                    .child(checkbox(
                        "poll-toggle-anonymous",
                        "Anonymous voting",
                        anonymous,
                        |dialog, on| dialog.is_anonymous = on,
                    ))
                    .child(checkbox(
                        "poll-toggle-multiple",
                        "Multiple answers",
                        multiple,
                        |dialog, on| {
                            // Quizzes are single-answer; the toggle is inert in quiz mode.
                            if !dialog.is_quiz {
                                dialog.allows_multiple_answers = on;
                            }
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(checkbox(
                        "poll-toggle-revoting",
                        "Allow revoting",
                        revoting,
                        |dialog, on| {
                            // Quizzes force revoting off; the toggle is inert in quiz mode.
                            if !dialog.is_quiz {
                                dialog.allows_revoting = on;
                            }
                        },
                    ))
                    .child(checkbox(
                        "poll-toggle-shuffle",
                        "Shuffle options",
                        shuffle,
                        |dialog, on| dialog.shuffle_options = on,
                    )),
            )
            // B15: tdesktop's remaining creation switches.
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(checkbox(
                        "poll-toggle-add-options",
                        "Allow adding options",
                        add_options && !is_quiz,
                        |dialog, on| {
                            // Quizzes have fixed options.
                            if !dialog.is_quiz {
                                dialog.allow_adding_options = on;
                            }
                        },
                    ))
                    .child(checkbox(
                        "poll-toggle-hide-results",
                        "Hide results until closed",
                        hide_results,
                        |dialog, on| dialog.hide_results_until_closes = on,
                    ))
                    .when(in_channel, |this| {
                        this.child(checkbox(
                            "poll-toggle-subscribers",
                            "Subscribers only",
                            subscribers,
                            |dialog, on| dialog.members_only = on,
                        ))
                    }),
            )
            .child(
                Checkbox::new("poll-toggle-deadline")
                    .checked(has_deadline)
                    .label("Close at a set date and time")
                    .on_click(cx.listener(|this, &on: &bool, window, cx| {
                        if let Some(dialog) = this.poll_dialog.as_mut() {
                            dialog.deadline = on.then(|| poll_deadline_picker(window, cx));
                        }
                        cx.notify();
                    })),
            )
    }
}
