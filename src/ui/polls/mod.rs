//! poll dialogs, voting, voters.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::country_box::CountryHost;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::dialogs::poll::QuizLock;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::date_picker::DatePicker;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::poll::{
    POLL_OPTIONS_MAX, POLL_OPTIONS_MIN, PollDraft, chat_allows_polls, voter_count_label,
};
use quill::state::{PollStatsFetch, PollVotersFetch};
use quill::telegram::envelope::{MessageContent, MessageSender};
use std::cell::RefCell;
use std::rc::Rc;

mod option_attach;
pub(in crate::ui) use option_attach::AttachTarget;
mod option_media;
pub(in crate::ui) mod option_thumb;

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
        self.message_ui.pending_stop_poll = Some((chat_id, message_id, is_quiz));
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_MEDIA_CONFIRM_STOP_POLL)));
        cx.notify();
    }

    pub(super) fn cancel_stop_poll(&mut self, cx: &mut Context<Self>) {
        self.message_ui.pending_stop_poll = None;
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_MEDIA_STOP_CANCELLED)));
        cx.notify();
    }

    /// B4: confirm `stopPoll` (schema 1.8.67 line 12953). The closed
    /// poll arrives via `updatePoll`; demo mode flips it locally.
    pub(super) fn confirm_stop_poll(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, message_id, _)) = self.message_ui.pending_stop_poll.take() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.stop_poll(chat_id, message_id);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_MEDIA_STOPPING_POLL)),
                Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_STOP_POLL)),
            });
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
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MEDIA_POLL_STOPPED_DEMO)));
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
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_MEDIA_VOTER_LISTS_ARE_UNAVAILABLE_IN,
            )));
            cx.notify();
            return;
        }
        self.message_ui.poll_voters_dialog = Some(PollVotersDialog {
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
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_MEDIA_POLL_STATS_ARE_UNAVAILABLE_IN,
            )));
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
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_LOAD_POLL_STATS,
            )));
            cx.notify();
            return;
        }
        self.message_ui.poll_voters_dialog = Some(PollVotersDialog {
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
        self.message_ui.poll_voters_dialog = None;
        cx.notify();
    }

    /// B4: switch the voters dialog to another option (first page).
    pub(super) fn select_poll_voters_option(
        &mut self,
        option_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.message_ui.poll_voters_dialog.as_mut() else {
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
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_LOAD_VOTERS)));
        }
        cx.notify();
    }

    /// B4: next `getPollVoters` page for the dialog's selected option.
    pub(super) fn load_more_poll_voters(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.message_ui.poll_voters_dialog.as_ref() else {
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
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_LOAD_MORE_VOTERS,
            )));
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
                .unwrap_or_else(|| {
                    tr_with(keys::Q_MEDIA_USER, &[("user_id", &user_id.to_string())])
                }),
            MessageSender::Chat { chat_id } => self
                .session()
                .and_then(|session| session.chats.get(chat_id))
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| {
                    tr_with(keys::Q_MEDIA_CHAT_2, &[("chat_id", &chat_id.to_string())])
                }),
        }
    }

    /// B4: the poll voters dialog body — option rows, then the selected
    /// option's voters with a "Load more" button while the server
    /// reports more than loaded.
    pub(super) fn poll_voters_dialog_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.message_ui.poll_voters_dialog.as_ref() else {
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
                        .messages
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
                    .child(tr(keys::Q_MEDIA_SELECT_AN_OPTION_TO_SEE)),
            ),
            Some(PollVotersFetch::Loading) => body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(text_muted())
                    .child(Spinner::new().small())
                    .child(tr(keys::Q_MEDIA_LOADING_VOTERS)),
            ),
            Some(PollVotersFetch::Failed(reason)) => body
                .child(div().text_xs().text_color(danger()).child(reason))
                .child(
                    Button::new("poll-voters-retry")
                        .label(tr(keys::Q_MEDIA_RETRY))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let index = this
                                .message_ui
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
                            .child(tr(keys::Q_MEDIA_NO_VOTERS_YET)),
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
                            .label(tr_with(
                                keys::Q_MEDIA_SHOW_MORE,
                                &[("left", &left.to_string())],
                            ))
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
            .and_then(|session| session.messages.poll_stats.get(&(chat_id.0, message_id.0)))
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
                    .child(tr(keys::Q_MEDIA_LOADING_STATS)),
            ),
            Some(PollStatsFetch::Failed(reason)) => section
                .child(div().text_xs().text_color(danger()).child(reason))
                .child(
                    Button::new("poll-stats-retry")
                        .label(tr(keys::Q_MEDIA_RETRY))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_poll_stats_dialog(chat_id, message_id, cx);
                        })),
                ),
            Some(PollStatsFetch::Loaded(graph)) => {
                match super::statistics::stats_graph_row(
                    tr(keys::Q_MEDIA_VOTES_OVER_TIME),
                    &graph,
                    cx,
                ) {
                    Some(row) => section.child(row),
                    None => section.child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child(tr(keys::Q_MEDIA_NO_VOTE_STATISTICS_YET)),
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
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_MEDIA_VOTING)),
                Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_VOTE)),
            });
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_poll_vote(chat_id, message_id, option_index);
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MEDIA_VOTE_UPDATED_DEMO)));
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
            self.connection
                .set_note(match live.driver.retract_poll_vote(chat_id, message_id) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MEDIA_VOTE_RETRACTED)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_RETRACT_THE_VOTE)),
                });
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
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MEDIA_VOTE_RETRACTED_DEMO)));
        }
        cx.notify();
    }

    /// Phase 4.2: open the poll creation dialog above the composer.
    pub(super) fn open_poll_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_poll_dialog_locked(QuizLock::Free, window, cx);
    }

    /// The poll box, with the type fixed when a bot's request-poll button
    /// asked for a quiz or a regular poll (tdesktop `kDefaultPollCreateFlags`
    /// with `disabled |= Quiz`).
    pub(super) fn open_poll_dialog_locked(
        &mut self,
        lock: QuizLock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MEDIA_POLLS_ARE_RESTRICTED_IN_THIS,
            )));
            cx.notify();
            return;
        }
        if self.deny_send(quill::send_rights::SendKind::Polls, cx) {
            return;
        }
        let in_channel = self
            .session()
            .and_then(|s| s.open_chat.and_then(|id| s.chats.get(&id.0)))
            .is_some_and(|chat| chat.is_channel());
        self.composer_ui.poll_dialog = Some(PollDialog::new(window, cx, in_channel, lock));
        if let Some(dialog) = &self.composer_ui.poll_dialog {
            dialog
                .question_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// Keep the poll panel's height cap in step with the window.
    pub(super) fn sync_poll_dialog_height(&mut self, window: &Window) {
        if let Some(dialog) = self.composer_ui.poll_dialog.as_mut() {
            dialog.viewport_height = window.viewport_size().height;
        }
    }

    pub(super) fn close_poll_dialog(&mut self, cx: &mut Context<Self>) {
        self.composer_ui.poll_dialog = None;
        cx.notify();
    }

    /// Slice B3: Cancel/Esc on the poll dialog. With unsent input this arms
    /// the inline discard confirmation instead of closing silently.
    pub(super) fn request_close_poll_dialog(&mut self, cx: &mut Context<Self>) {
        let dirty = self
            .composer_ui
            .poll_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.is_dirty(cx));
        if dirty {
            if let Some(dialog) = self.composer_ui.poll_dialog.as_mut() {
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
                        .message_ui
                        .poll_voters_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.show_stats)
                    {
                        tr(keys::POLLS_STATS_TITLE)
                    } else {
                        tr(keys::Q_MEDIA_POLL_VOTERS)
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
        let Some(dialog) = self.composer_ui.poll_dialog.as_mut() else {
            return;
        };
        if dialog.option_inputs.len() >= POLL_OPTIONS_MAX {
            return;
        }
        let index = dialog.option_inputs.len();
        dialog.option_inputs.push(cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr_with(
                    keys::Q_MEDIA_OPTION,
                    &[("index_1", &(index + 1).to_string())],
                ))
                .auto_grow(1, 2)
                .submit_on_enter(false)
        }));
        dialog.attach.push(Default::default());
        cx.notify();
    }

    pub(super) fn remove_poll_option_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(dialog) = self.composer_ui.poll_dialog.as_mut() else {
            return;
        };
        if dialog.option_inputs.len() <= POLL_OPTIONS_MIN || index >= dialog.option_inputs.len() {
            return;
        }
        dialog.option_inputs.remove(index);
        // The media, its menu, link field and sticker choice go with the row.
        if index < dialog.attach.len() {
            dialog.attach.remove(index);
        }
        // Keep the marked correct-answer rows pointing at the same options.
        dialog.forget_correct_row(index);
        cx.notify();
    }

    /// Phase 4.2: freeze the dialog, validate, and send `inputMessagePoll`
    /// through the driver (same `sendMessage` path as the composer). The
    /// pending reply (if any) is attached like a normal send.
    pub(super) fn submit_poll_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let close_max = self.poll_close_max_secs();
        let draft = match self.composer_ui.poll_dialog.as_ref() {
            Some(dialog) => PollDraft {
                close_max,
                ..dialog.draft(cx)
            },
            None => return,
        };
        // A custom deadline that passed while the box was open.
        if draft.close_date > 0 && draft.close_date <= quill::local_time::now_unix() {
            self.connection
                .set_note(StatusNote::error(tr(keys::POLLS_CREATE_DEADLINE_EXPIRED)));
            cx.notify();
            return;
        }
        if self
            .composer_ui
            .poll_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.limit_by_country && dialog.country_codes.is_empty())
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::POLLS_CREATE_CHOOSE_COUNTRY)));
            cx.notify();
            return;
        }
        if let Some(reason) = draft.validate() {
            self.connection
                .set_note(StatusNote::error(reason.to_string()));
            cx.notify();
            return;
        }
        if self
            .composer_ui
            .poll_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.media_preparing())
        {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_MEDIA_WAIT_FOR_THE_VIDEO_TO)));
            cx.notify();
            return;
        }
        let plan = self.session().and_then(|session| {
            let chat_id = session.open_chat?;
            let chat = session.chats.get(&chat_id.0)?;
            chat.can_post().then_some(chat_id)
        });
        let Some(chat_id) = plan else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_MEDIA_SELECT_A_CHAT_TO_SEND)));
            cx.notify();
            return;
        };
        // Phase A1: slow-mode gate applies to polls too.
        if self.slow_mode_blocked(chat_id, cx) {
            return;
        }
        // Slice G1: the poll carries the composer's quote, if any.
        let reply_to = self.send_reply_for(chat_id, cx);
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.send_poll_draft(chat_id, &draft, reply_to);
            match result {
                Ok(_) => {
                    self.composer_ui.poll_dialog = None;
                    self.composer_ui.set_reply(None);
                    self.connection
                        .set_note(StatusNote::silent(tr(keys::Q_MEDIA_SENDING_POLL)));
                }
                Err(_) => {
                    self.connection
                        .set_note(StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_SEND_POLL)));
                }
            }
            cx.notify();
            return;
        }
        // Screenshot demos have no live driver; close the dialog honestly.
        let _ = window;
        self.composer_ui.poll_dialog = None;
        self.connection.set_note(StatusNote::info(tr(
            keys::Q_MEDIA_POLLS_NEED_A_LIVE_CONNECTION,
        )));
        cx.notify();
    }

    /// B4: "Stop poll" / "Stop quiz" confirm banner, styled like the
    /// delete confirm. TGX `StopPollWarn` / `StopQuizWarn`: nobody can
    /// vote afterwards, and the action can't be undone.
    pub(super) fn stop_poll_confirm_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_quiz = self
            .message_ui
            .pending_stop_poll
            .is_some_and(|(_, _, is_quiz)| is_quiz);
        let title = if is_quiz {
            tr(keys::Q_MEDIA_STOP_THIS_QUIZ)
        } else {
            tr(keys::Q_MEDIA_STOP_THIS_POLL)
        };
        let warning = if is_quiz {
            tr(keys::Q_MEDIA_IF_YOU_STOP_THIS_QUIZ)
        } else {
            tr(keys::POLLS_STOP_WARNING)
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
                            .label(tr(keys::CANCEL))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_stop_poll(cx);
                            })),
                    )
                    .child(
                        Button::new("confirm-stop-poll")
                            .label(if is_quiz {
                                tr(keys::Q_MEDIA_STOP_QUIZ)
                            } else {
                                tr(keys::POLLS_STOP)
                            })
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
        self.composer_ui.poll_dialog.as_ref()?;
        self.poll_dialog_panel_body(cx)
    }

    /// `poll_dialog_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn poll_dialog_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.composer_ui.poll_dialog.as_ref()?;
        if dialog.country_box.is_some() {
            return self.poll_country_panel(cx);
        }
        if dialog.deadline_box.is_some() {
            return self.poll_deadline_panel(cx);
        }
        let title = if dialog.is_quiz {
            tr(keys::Q_MEDIA_NEW_QUIZ)
        } else {
            tr(keys::POLLS_CREATE_TITLE)
        };
        // The fields and settings scroll when the window is short; the
        // title and the Create / Cancel buttons stay in view.
        let mut panel = div()
            .id("poll-dialog-body")
            .flex()
            .flex_col()
            .gap_2()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                Textarea::new(&dialog.description_input)
                    .aria_label(tr(keys::Q_MEDIA_POLL_DESCRIPTION))
                    .h(px(40.)),
            )
            .child(
                Textarea::new(&dialog.question_input)
                    .aria_label(tr(keys::Q_MEDIA_POLL_QUESTION))
                    .h(px(64.)),
            );
        let is_quiz = dialog.is_quiz;
        let quiz_correct_rows = dialog.quiz_correct_rows.clone();
        for (index, input) in dialog.option_inputs.iter().enumerate() {
            let mut row = div()
                .id(("poll-dialog-option", index as u64))
                .flex()
                .items_center()
                .gap_2();
            if is_quiz {
                let marked = quiz_correct_rows.contains(&index);
                row = row.child(
                    // Phase 6: kit Checkbox, not Radio — a correct answer
                    // is toggleable (clicking the marked option clears it),
                    // and kit Radio cannot deselect itself. With "Allow
                    // Multiple Answers" on, several may be marked.
                    Checkbox::new(format!("poll-quiz-correct-{index}"))
                        .checked(marked)
                        .accessibility_label(tr_with(
                            keys::Q_LEFTOVERS_MARK_OPTION_CORRECT,
                            &[("number", &(index + 1).to_string())],
                        ))
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if let Some(dialog) = this.composer_ui.poll_dialog.as_mut() {
                                dialog.set_correct_row(index, on);
                            }
                            cx.notify();
                        })),
                );
            }
            row = row.child(
                div().flex_1().child(
                    Textarea::new(input)
                        .aria_label(tr(keys::Q_MEDIA_POLL_ANSWER))
                        .token(self.field_token_renderer(input, cx))
                        .h(px(40.)),
                ),
            );
            row = row.child(self.poll_option_emoji_button(AttachTarget::Row(index), cx));
            row = row.child(self.poll_option_attach_button(AttachTarget::Row(index), cx));
            if dialog.option_inputs.len() > POLL_OPTIONS_MIN {
                row = row.child(
                    Button::new(format!("poll-remove-option-{index}"))
                        .label("✕")
                        .ghost()
                        .tooltip(tr(keys::Q_MEDIA_REMOVE_OPTION))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_poll_option_row(index, cx);
                        })),
                );
            }
            panel = panel.child(row);
            if let Some(extra) = self.poll_option_attach_area(AttachTarget::Row(index), cx) {
                panel = panel.child(extra);
            }
        }
        if dialog.option_inputs.len() < POLL_OPTIONS_MAX {
            panel = panel.child(
                Button::new("poll-add-option")
                    .label(tr(keys::Q_MEDIA_ADD_OPTION))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_poll_option_row(window, cx);
                    })),
            );
        }
        if is_quiz {
            panel = panel.child(
                Textarea::new(&dialog.explanation_input)
                    .aria_label(tr(keys::Q_MEDIA_QUIZ_EXPLANATION))
                    .h(px(64.)),
            );
        }
        if let Some(settings) = self.poll_settings(cx) {
            panel = panel.child(settings);
        }
        let mut outer = div()
            .id("poll-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .min_h_0()
            .max_h((dialog.viewport_height - px(160.)).max(px(240.)))
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(div().text_sm().font_semibold().child(title))
            .child(panel);
        if dialog.confirming_discard {
            outer = outer.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_sm().child(tr(keys::Q_MEDIA_DISCARD_THIS_POLL)))
                    .child(
                        Button::new("poll-discard-yes")
                            .label(tr(keys::Q_MEDIA_DISCARD))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_poll_dialog(cx);
                            })),
                    )
                    .child(
                        Button::new("poll-discard-no")
                            .label(tr(keys::Q_MEDIA_KEEP_EDITING))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(dialog) = this.composer_ui.poll_dialog.as_mut() {
                                    dialog.confirming_discard = false;
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        outer = outer.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("poll-create")
                        .label(tr(keys::POLLS_CREATE))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_poll_dialog(window, cx);
                        })),
                )
                .child(
                    Button::new("poll-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.request_close_poll_dialog(cx);
                        })),
                ),
        );
        Some(outer.into_any_element())
    }
}

crate::ui::shell::register_dialogs! {
    PollVoters => DialogSpec::new(
        5500,
        |app| app.message_ui.poll_voters_dialog.is_some(),
        QuillApp::build_poll_voters_dialog,
    ),
}

mod demo;
mod poll_deadline;
mod poll_settings;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod settings_tests;
