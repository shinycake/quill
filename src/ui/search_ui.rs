//! global + in-chat search UI and shared-media browser UI.

/// A search-row preview is one elided line, like tdesktop's dialog rows:
/// every line break (and other control whitespace) becomes a space. GPUI
/// lays a literal `\n` out as a second line even under `truncate()`.
pub(super) fn one_line_preview(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

use super::app::{PaneMode, QuillApp};
use super::pressable::PressableDiv;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::draft_text_to_store;
use quill::connect::{ChatSearchQueryOutcome, SEARCH_DEBOUNCE, SearchQueryOutcome};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::search_filters::{SearchChatType, SearchDateRange, SearchMediaKind};
use quill::state::{ChatSearchJump, FromPicker, RequestPurpose, SearchStatus, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChatDraft;
use quill::telegram::envelope::MessageSender;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// One search hit: avatar, title with the date (message hits), and the
/// preview with the query's matches highlighted — the chat-row anatomy.
#[allow(clippy::too_many_arguments)]
pub(super) fn search_result_row(
    id: (&'static str, u64),
    title: String,
    preview: String,
    date: Option<i32>,
    query: &str,
    photo: Option<std::path::PathBuf>,
    cx: &mut Context<QuillApp>,
    on_pick: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
) -> impl IntoElement {
    let preview = one_line_preview(&preview);
    let stamp = date.filter(|date| *date > 0).map(|date| {
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        quill::local_time::chat_list_stamp(&quill::local_time::civil_local(i64::from(date)), &now)
    });
    let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = match_ranges(&preview, query)
        .into_iter()
        .map(|range| {
            (
                range,
                HighlightStyle {
                    color: Some(cx.theme().foreground),
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                },
            )
        })
        .collect();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_2()
        .py_1p5()
        .rounded_md()
        .role(gpui_kit::Role::Button)
        .aria_label(format!("{title} · {preview}"))
        .tab_index(0)
        .cursor_pointer()
        .pressable(cx.theme())
        .on_click(cx.listener(move |this, _, window, cx| on_pick(this, window, cx)))
        .child(super::chat_row::chat_avatar(&title, photo.as_deref(), 40.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_medium()
                                .child(super::bidi_line::one_line_plain(title)),
                        )
                        .when_some(stamp, |this, stamp| {
                            this.child(
                                div()
                                    .flex_none()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(stamp),
                            )
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(cx.theme().muted_foreground)
                        .child(super::bidi_line::one_line(preview, highlights, Vec::new())),
                ),
        )
}

/// One in-chat search hit: the message snippet with matches highlighted
/// and its date; the selected hit (the one the history jumped to) is
/// tinted. Click jumps to it.
fn chat_search_hit_row(
    message_id: MessageId,
    preview: String,
    date: i32,
    query: &str,
    selected: bool,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let preview = one_line_preview(&preview);
    let stamp = (date > 0).then(|| {
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        quill::local_time::chat_list_stamp(&quill::local_time::civil_local(i64::from(date)), &now)
    });
    let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = match_ranges(&preview, query)
        .into_iter()
        .map(|range| {
            (
                range,
                HighlightStyle {
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                },
            )
        })
        .collect();
    div()
        .id(("chat-search-hit", message_id.0 as u64))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded_md()
        .role(gpui_kit::Role::Button)
        .aria_label(preview.clone())
        .aria_selected(selected)
        .tab_index(0)
        .cursor_pointer()
        .pressable(cx.theme())
        .when(selected, |this| this.bg(cx.theme().selection))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.jump_chat_search_message(message_id, cx);
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .truncate()
                .child(super::bidi_line::one_line(preview, highlights, Vec::new())),
        )
        .when_some(stamp, |this, stamp| {
            this.child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(stamp),
            )
        })
}

/// Byte ranges in `text` matching `query` case-insensitively. Empty when the
/// query is empty or lowercasing would shift byte offsets (no highlight is
/// better than a misplaced one).
fn match_ranges(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let haystack = text.to_lowercase();
    let needle = query.to_lowercase();
    if haystack.len() != text.len() {
        return Vec::new();
    }
    haystack
        .match_indices(&needle)
        .map(|(start, matched)| start..start + matched.len())
        .filter(|range| text.is_char_boundary(range.start) && text.is_char_boundary(range.end))
        .collect()
}

pub(super) fn chat_search_jump_note(session: &Session) -> String {
    match session.chat_search.jump {
        ChatSearchJump::None => String::new(),
        ChatSearchJump::Loading { .. } => "Loading message…".into(),
        ChatSearchJump::Ready { .. } => String::new(),
        ChatSearchJump::Missing { .. } => "This message is unavailable.".into(),
    }
}

pub(super) fn apply_ready_search(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_search();
    let search_gen = session.search.begin_query("hello");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let jsons = [
        format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
            chats_extra.0
        ),
        format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}}}]}}"#,
            messages_extra.0
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(super) fn apply_ready_search_in_chat(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    assert!(session.open_chat_search());
    let search_gen = session.chat_search.begin_query("hello");
    let extra =
        session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), search_gen);
    let json = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":90,"messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}}},{{"id":103,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Two more waiting.","entities":[]}}}}}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
    let _ = session.begin_chat_search_jump(MessageId(101));
}

impl QuillApp {
    pub(super) fn search_is_open(&self) -> bool {
        self.session().is_some_and(|session| session.search.open)
    }

    pub(super) fn chat_search_is_open(&self) -> bool {
        self.session()
            .is_some_and(|session| session.chat_search.open)
    }

    pub(super) fn open_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.open_search() {
                Ok(Some(_)) => self.status_note = "searching…".into(),
                Ok(None) => self.status_note = "search chats and messages".into(),
                Err(_) => self.status_note = "could not search".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_search();
            self.status_note = "search chats and messages".into();
        }
        self.search_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn close_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_search();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.close_search();
        }
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        window.focus(&self.focus_sidebar, cx);
        self.status_note = "search closed".into();
        cx.notify();
    }

    pub(super) fn cancel_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.message_menu.is_some() || self.chat_menu.is_some() {
            self.message_menu = None;
            self.chat_menu = None;
            cx.notify();
            return;
        }
        // The profile layer fades out before anything beneath it.
        if self.profile_modal_active() {
            self.close_profile_modal(cx);
            return;
        }
        // The composer's link dialog closes first.
        if self.composer_link_dialog.is_some() {
            self.close_composer_link_dialog(window, cx);
            return;
        }
        // Slice CL: the peek preview is the most transient layer —
        // Escape dismisses it before anything else.
        if self.chat_preview.is_some() || self.preview_press.is_some() {
            self.close_chat_preview(cx);
            return;
        }
        // Phase 9.3: the story composer is the topmost overlay — Escape
        // closes it before the story viewer.
        if self.story_composer.open {
            self.close_story_composer(cx);
            return;
        }
        // Phase 9.1: the story viewer is the topmost overlay — Escape
        // closes it before the media viewer.
        if self.story_viewer.is_open() {
            self.close_story_viewer(cx);
            return;
        }
        if self.media_viewer.is_open() {
            self.close_media_viewer(cx);
            return;
        }
        if self.poll_dialog.is_some() {
            self.request_close_poll_dialog(cx);
            return;
        }
        // Slice P1: Esc closes the checkout and receipt dialogs.
        if self.payment_dialog.is_some() {
            self.close_payment_dialog(cx);
            return;
        }
        if self
            .session()
            .is_some_and(|session| session.payment_receipt_open)
        {
            self.close_payment_receipt(cx);
            return;
        }
        // MED2: Esc never discards a recording silently. With the confirm
        // row open, Esc dismisses it and keeps recording; otherwise Esc
        // opens the confirm row (locked recordings ignore Esc entirely).
        if self.record_discard_confirm {
            self.record_discard_confirm = false;
            cx.notify();
            return;
        }
        if self.recording_active() {
            if self.record_locked {
                self.status_note = "recording is locked — unlock it or use Cancel".into();
                cx.notify();
            } else {
                self.request_discard_recording(cx);
            }
            return;
        }
        if self.gif_panel_open() {
            self.close_gif_panel(cx);
            return;
        }
        if self.sticker_panel_open() {
            self.close_sticker_panel(cx);
            return;
        }
        if self.notification_defaults_open {
            self.notification_defaults_open = false;
            self.defaults_sound_picker = None;
            self.defaults_exceptions_scope = None;
            cx.notify();
            return;
        }
        // Settings → Appearance: Esc closes the dialog (backdrop click
        // also closes; changes already applied live).
        if self.appearance_open {
            self.close_appearance();
            cx.notify();
            return;
        }
        // Slice A3: Esc on the sessions overlay cancels a pending
        // terminate confirmation first, then closes the overlay.
        if self.sessions_open {
            self.close_sessions(cx);
            return;
        }
        if self.mute_menu_open {
            self.close_mute_menu(cx);
            return;
        }
        // Phase B4: Esc closes the TTL picker too.
        if self.ttl_picker_open {
            self.ttl_picker_open = false;
            cx.notify();
            return;
        }
        if self
            .session()
            .is_some_and(|session| session.sponsored_report.is_some())
        {
            self.dismiss_sponsored_report_ui(cx);
            return;
        }
        if self.forward_picker_open {
            self.close_forward_picker(window, cx);
            return;
        }
        if self.pending_delete.is_some() {
            self.cancel_delete(cx);
            return;
        }
        // B4: Escape cancels the stop-poll confirm too.
        if self.pending_stop_poll.is_some() {
            self.cancel_stop_poll(cx);
            return;
        }
        // Slice media-shared-gallery: Escape dismisses the gallery like any
        // other transient panel; it sits above the chat-search layer (the
        // row-click jump closes the gallery into a chat search).
        if self
            .session()
            .is_some_and(|session| session.shared_media.open)
        {
            self.close_shared_media_ui(cx);
            return;
        }
        if self.chat_search_is_open() {
            self.close_chat_search_ui(window, cx);
            return;
        }
        if self.search_is_open() {
            let query = self.search_input.read(cx).value().to_string();
            if query.trim().is_empty() {
                self.close_search_ui(window, cx);
            } else {
                self.search_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.sync_search_query("", cx);
            }
            return;
        }
        if self.pending_reply.is_some() {
            self.clear_reply(cx);
            return;
        }
        if self.pending_edit.is_some() {
            self.clear_edit(window, cx);
            return;
        }
        if self.pending_forward.is_some() {
            self.clear_forward(window, cx);
            return;
        }
        if self.forward_result.is_some() {
            self.forward_result = None;
            self.status_note = "forward result dismissed".into();
            cx.notify();
        }
    }

    /// Slice CL2: "Clear" on the Recent searches heading —
    /// `clearRecentlyFoundChats` (schema 1.8.67, line 11671); the
    /// recents clear optimistically (TGX `SearchManager` clears
    /// locally too, lines 788-796).
    pub(super) fn clear_search_recents(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.clear_recently_found_chats() {
                self.status_note = format!("clear recents failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.chat_ids.clear();
            session.search.status = SearchStatus::Idle;
        }
        cx.notify();
    }

    /// Slice (communities-search-filter): route a community-filter chip click
    /// to the driver (re-runs the current query with the new filter); in demo
    /// mode just records the selection for the panel to display.
    pub(super) fn set_search_community_filter(
        &mut self,
        community_id: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_search_community_filter(community_id) {
                self.status_note = format!("community filter failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.community_filter = community_id;
        }
        cx.notify();
    }

    pub(super) fn open_chat_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready {
            return;
        }
        let opened = if let Some(live) = self.live.as_mut() {
            match live.driver.open_chat_search() {
                Ok(true) => {
                    self.status_note = "search in chat".into();
                    true
                }
                Ok(false) => {
                    self.status_note = "select a chat to search in conversation".into();
                    false
                }
                Err(_) => {
                    self.status_note = "could not search in chat".into();
                    false
                }
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            if session.open_chat_search() {
                self.status_note = "search in chat".into();
                true
            } else {
                self.status_note = "select a chat to search in conversation".into();
                false
            }
        } else {
            false
        };
        if opened {
            self.chat_search_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    pub(super) fn close_chat_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_chat_search();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.close_chat_search();
        }
        self.chat_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.status_note = "in-chat search closed".into();
        cx.notify();
    }

    pub(super) fn sync_chat_search_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready || !self.chat_search_is_open() {
            return;
        }
        // While choosing a "From:" member the field filters the members.
        if self
            .session()
            .is_some_and(|s| s.chat_search.from_picker.is_some())
        {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.search_from_members(query.trim());
            }
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.set_chat_search_query(query) {
                Ok(ChatSearchQueryOutcome::Sent(_)) => {
                    self.status_note = "searching in chat…".into()
                }
                Ok(ChatSearchQueryOutcome::Debounced { token }) => {
                    self.schedule_chat_search_commit(token, cx);
                }
                Ok(ChatSearchQueryOutcome::Unchanged) if query.trim().is_empty() => {
                    self.status_note = "search in chat".into();
                }
                Ok(ChatSearchQueryOutcome::Unchanged) => {}
                Err(_) => self.status_note = "could not search in chat".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let trimmed = query.trim();
            if session.chat_search.open
                && session.chat_search.query == trimmed
                && !matches!(session.chat_search.status, SearchStatus::Closed)
            {
                cx.notify();
                return;
            }
            session.apply_local_chat_search_filter(query);
        }
        cx.notify();
    }

    pub(super) fn schedule_chat_search_commit(&mut self, token: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if let Some(live) = this.live.as_mut() {
                    match live.driver.commit_debounced_chat_search(token) {
                        Ok(Some(_)) => this.status_note = "searching in chat…".into(),
                        Ok(None) => {}
                        Err(_) => this.status_note = "could not search in chat".into(),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn jump_selected_chat_search_hit(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.jump_selected_chat_search_hit() {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(id) = session.chat_search.selected_hit().map(|hit| hit.message_id) {
                let _ = session.begin_chat_search_jump(id);
            }
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    pub(super) fn jump_chat_search_message(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.jump_to_chat_search_message(message_id) {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    pub(super) fn chat_search_newer(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.chat_search_newer() {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(id) = session.chat_search.select_newer() {
                let _ = session.begin_chat_search_jump(id);
            }
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    pub(super) fn chat_search_older(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.chat_search_older() {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(id) = session.chat_search.select_older() {
                let _ = session.begin_chat_search_jump(id);
            }
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    pub(super) fn chat_search_pick_sender(
        &mut self,
        sender: Option<MessageSender>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.set_chat_search_sender(sender).is_err() {
                self.status_note = "could not search in chat".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.chat_search.sender = sender;
            session.close_from_picker();
        }
        // The field's text was the member filter while choosing: start the
        // message query from scratch.
        self.chat_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.chat_search_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn chat_search_pick_media(
        &mut self,
        media: SearchMediaKind,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.set_chat_search_media(media).is_err() {
                self.status_note = "could not search in chat".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.chat_search.media = media;
        }
        cx.notify();
    }

    pub(super) fn toggle_chat_search_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .session()
            .is_some_and(|s| s.chat_search.from_picker.is_some());
        if open {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.close_from_picker();
            } else if let Some(session) = self.demo_session.as_mut() {
                session.close_from_picker();
            }
        } else if let Some(live) = self.live.as_mut() {
            if live.driver.open_chat_search_from_picker().is_err() {
                self.status_note = "members are not available here".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_from_picker();
        }
        // The field now filters members (or messages again): start empty.
        self.chat_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.chat_search_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn set_search_filters(
        &mut self,
        filters: quill::search_filters::GlobalSearchFilters,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_search_filters(filters) {
                self.status_note = format!("could not apply search filters: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.filters = filters;
        }
        cx.notify();
    }

    /// Small quiet toggle used by the search filter rows.
    fn filter_chip(
        id: impl Into<ElementId>,
        label: &str,
        selected: bool,
        on_click: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Button::new(id)
            .label(label.to_string())
            .ghost()
            .xsmall()
            .selected(selected)
            .toggled(selected)
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }

    /// The members list that replaces the hits while a "From:" is chosen.
    fn chat_search_picker(&self, picker: &FromPicker, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let mut list = div()
            .id("chat-search-from-list")
            .flex()
            .flex_col()
            .gap_0p5();
        let rows: Vec<(MessageSender, String)> = picker
            .members
            .iter()
            .map(|m| {
                (
                    *m,
                    session.map_or_else(|| "member".into(), |s| s.sender_label(*m)),
                )
            })
            .collect();
        if rows.is_empty() {
            let text = if picker.request.is_some() {
                "Loading members…"
            } else {
                "No members found."
            };
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(text),
            );
        }
        for (sender, name) in rows {
            let photo = match sender {
                MessageSender::User { user_id } => self.chat_photo_for_row(ChatId(user_id)),
                MessageSender::Chat { chat_id } => self.chat_photo_for_row(ChatId(chat_id)),
            };
            let key = match sender {
                MessageSender::User { user_id } => user_id as u64,
                MessageSender::Chat { chat_id } => chat_id as u64 ^ (1 << 62),
            };
            list = list.child(
                div()
                    .id(("chat-search-from", key))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("Search messages from {name}"))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.chat_search_pick_sender(Some(sender), window, cx);
                    }))
                    .child(super::chat_row::chat_avatar(&name, photo.as_deref(), 28.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .child(super::bidi_line::one_line_plain(name)),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .child("Search messages from"),
            )
            .child(list.max_h(px(240.)).overflow_y_scroll())
    }

    pub(super) fn chat_search_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let status = session
            .map(|s| s.chat_search.status)
            .unwrap_or(SearchStatus::Closed);
        let query = session
            .map(|s| s.chat_search.query.clone())
            .unwrap_or_default();
        let position = session
            .map(|s| s.chat_search.position_label())
            .unwrap_or_default();
        let jump_note = session.map(chat_search_jump_note).unwrap_or_default();
        let can_pick = session.is_some_and(|s| s.chat_search_can_pick_sender());
        let sender_label =
            session.and_then(|s| s.chat_search.sender.map(|sender| s.sender_label(sender)));
        let media = session.map(|s| s.chat_search.media).unwrap_or_default();
        let picker = session.and_then(|s| s.chat_search.from_picker.clone());
        let hits: Vec<(MessageId, String, i32, bool)> = session
            .map(|s| {
                s.chat_search
                    .hits
                    .iter()
                    .enumerate()
                    .map(|(i, hit)| {
                        (
                            hit.message_id,
                            hit.preview.clone(),
                            hit.date,
                            s.chat_search.selected == Some(i),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let subject = if !query.is_empty() {
            format!("“{query}”")
        } else if let Some(name) = &sender_label {
            format!("messages from {name}")
        } else {
            media.label().to_lowercase()
        };
        let caption = match status {
            SearchStatus::Idle => "Type to search this chat.".to_string(),
            SearchStatus::Searching => format!("Searching {subject}…"),
            SearchStatus::Ready => {
                if jump_note.is_empty() {
                    format!("Results for {subject}")
                } else {
                    format!("Results for {subject} · {jump_note}")
                }
            }
            SearchStatus::Empty => format!("No messages match {subject}."),
            SearchStatus::Failed => "Search in chat failed.".to_string(),
            SearchStatus::Closed => String::new(),
        };
        let picker_open = picker.is_some();
        let mut media_chips = Vec::new();
        for kind in SearchMediaKind::ALL {
            media_chips.push(
                Self::filter_chip(
                    ("chat-search-media", kind as u64),
                    kind.label(),
                    media == kind,
                    move |this, _, cx| this.chat_search_pick_media(kind, cx),
                    cx,
                )
                .into_any_element(),
            );
        }
        div()
            .id("chat-search")
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                // One row: field, position, older/newer, calendar, close.
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div().id("chat-search-field").flex_1().child(
                            Textarea::new(&self.chat_search_input)
                                .aria_label(if picker_open {
                                    "Search members"
                                } else {
                                    "Search this conversation"
                                })
                                .h(px(36.)),
                        ),
                    )
                    .when(!position.is_empty() && !picker_open, |this| {
                        this.child(
                            div()
                                .px_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(position),
                        )
                    })
                    .child(
                        Button::new("chat-search-older")
                            .icon(gpui_kit::assets::IconName::ChevronUp)
                            .ghost()
                            .tooltip("Older match")
                            .accessibility_label("Older match")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.chat_search_older(cx);
                            })),
                    )
                    .child(
                        Button::new("chat-search-newer")
                            .icon(gpui_kit::assets::IconName::ChevronDown)
                            .ghost()
                            .tooltip("Newer match")
                            .accessibility_label("Newer match")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.chat_search_newer(cx);
                            })),
                    )
                    .child(
                        Button::new("chat-search-calendar")
                            .icon(gpui_kit::assets::IconName::Calendar)
                            .ghost()
                            .tooltip("Jump to date")
                            .accessibility_label("Jump to date")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_jump_date_ui(cx);
                            })),
                    )
                    .child(
                        Button::new("chat-search-close")
                            .icon(gpui_kit::assets::IconName::X)
                            .tooltip("Close search")
                            .accessibility_label("Close search")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_chat_search_ui(window, cx);
                            })),
                    ),
            )
            .child(
                // Filters: who wrote it, what kind of message.
                div()
                    .id("chat-search-filters")
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .when(can_pick, |this| {
                        let label = sender_label
                            .as_ref()
                            .map_or_else(|| "From…".to_string(), |name| format!("From: {name}"));
                        this.child(Self::filter_chip(
                            "chat-search-from",
                            &label,
                            sender_label.is_some() || picker_open,
                            |this, window, cx| {
                                if this
                                    .session()
                                    .is_some_and(|s| s.chat_search.sender.is_some())
                                {
                                    this.chat_search_pick_sender(None, window, cx);
                                } else {
                                    this.toggle_chat_search_picker(window, cx);
                                }
                            },
                            cx,
                        ))
                    })
                    .children(media_chips),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if picker_open { String::new() } else { caption }),
            )
            .when_some(picker, |this, picker| {
                this.child(self.chat_search_picker(&picker, cx))
            })
            .when(!hits.is_empty() && !picker_open, |this| {
                let mut list = div().id("chat-search-hits").flex().flex_col().gap_1();
                for (message_id, preview, date, selected) in hits {
                    list = list.child(chat_search_hit_row(
                        message_id, preview, date, &query, selected, cx,
                    ));
                }
                this.child(list)
            })
    }

    pub(super) fn sync_search_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.set_search_query(query) {
                Ok(SearchQueryOutcome::Sent(_)) => self.status_note = "searching…".into(),
                Ok(SearchQueryOutcome::Debounced { token }) => {
                    self.schedule_search_commit(token, cx);
                }
                Ok(SearchQueryOutcome::Unchanged) if query.trim().is_empty() => {
                    self.status_note = "search chats and messages".into();
                }
                Ok(SearchQueryOutcome::Unchanged) => {}
                Err(_) => self.status_note = "could not search".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let trimmed = query.trim();
            if session.search.open
                && session.search.query == trimmed
                && !matches!(session.search.status, SearchStatus::Closed)
            {
                cx.notify();
                return;
            }
            session.apply_local_search_filter(query);
        }
        cx.notify();
    }

    pub(super) fn schedule_search_commit(&mut self, token: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if let Some(live) = this.live.as_mut() {
                    match live.driver.commit_debounced_search(token) {
                        Ok(Some(_)) => this.status_note = "searching…".into(),
                        Ok(None) => {}
                        Err(_) => this.status_note = "could not search".into(),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn activate_first_search_result(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat = self
            .session()
            .and_then(|session| session.search.chat_ids.first().copied());
        let message = self.session().and_then(|session| {
            session
                .search
                .messages
                .first()
                .map(|hit| (hit.chat_id, hit.message_id))
        });
        if let Some(chat_id) = chat {
            self.select_search_chat(chat_id, window, cx);
        } else if let Some((chat_id, message_id)) = message {
            self.select_search_message(chat_id, message_id, window, cx);
        }
    }

    pub(super) fn select_search_chat(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, reply, now_ms) = self.leaving_draft_parts(cx);
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live
                .driver
                .select_search_chat(chat_id, &text, reply, now_ms)
            {
                Ok(_) => "chat selected".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(prev) = session.open_chat {
                if prev != chat_id {
                    let stored = draft_text_to_store(&text, reply.is_some()).map(str::to_string);
                    let reply_to = stored.as_ref().and(reply);
                    let draft = stored.map(|body| ChatDraft {
                        text: body,
                        reply_to_message_id: reply_to.as_ref().map(|reply| reply.message_id),
                        quote: reply_to.and_then(|reply| reply.quote),
                    });
                    session.store_composer_draft(prev, draft);
                }
            }
            session.close_search();
            session.open_chat(chat_id);
            self.status_note = "chat selected".into();
        }
        self.dismiss_cross_chat_state(chat_id, cx);
        self.restore_open_draft(window, cx);
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(super) fn select_search_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, reply, now_ms) = self.leaving_draft_parts(cx);
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live
                .driver
                .select_search_message(chat_id, message_id, &text, reply, now_ms)
            {
                Ok(_) => "opened chat".into(),
                Err(_) => "could not open chat".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(prev) = session.open_chat {
                if prev != chat_id {
                    let stored = draft_text_to_store(&text, reply.is_some()).map(str::to_string);
                    let reply_to = stored.as_ref().and(reply);
                    let draft = stored.map(|body| ChatDraft {
                        text: body,
                        reply_to_message_id: reply_to.as_ref().map(|reply| reply.message_id),
                        quote: reply_to.and_then(|reply| reply.quote),
                    });
                    session.store_composer_draft(prev, draft);
                }
            }
            session.promote_search_message(chat_id, message_id);
            session.close_search();
            session.open_chat(chat_id);
            self.status_note = "opened chat".into();
        }
        self.dismiss_cross_chat_state(chat_id, cx);
        self.restore_open_draft(window, cx);
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(super) fn sidebar_search_field(
        &self,
        story_stack: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("sidebar-search")
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("sidebar-search-field")
                    .flex_1()
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.search_is_open() {
                            this.open_search_ui(window, cx);
                        }
                    }))
                    .child(
                        Textarea::new(&self.search_input)
                            .aria_label("Search")
                            .h(px(40.)),
                    ),
            )
            .children(story_stack)
            .when(self.search_is_open(), |this| {
                this.child(
                    Button::new("search-clear")
                        .icon(gpui_kit::assets::IconName::X)
                        .ghost()
                        .small()
                        .tooltip("Close search")
                        .accessibility_label("Close search")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_search(window, cx);
                        })),
                )
            })
    }

    /// Slice (communities-search-filter): community filter chips at the top of
    /// the typed-search panel — "All chats" (null filter) plus one chip per
    /// accessible community from `SessionState.communities` (fed by
    /// `updateCommunity`; TDLib 1.8.67 has no list-communities method, and
    /// `searchMessagesChatTypeFilterCommunity` requires a `community_id`,
    /// schema line 6344). Omitted when there is nothing to filter by.
    /// Same kit-Button chip pattern as the event-log filters.
    pub(super) fn search_community_filter_chips(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let selected = session.search.community_filter;
        let mut communities: Vec<(i64, String)> = session
            .communities
            .iter()
            .filter(|(_, community)| community.have_access)
            .map(|(id, community)| (*id, community.name.clone()))
            .collect();
        if communities.is_empty() {
            return None;
        }
        communities.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        let mut chips = div()
            .id("search-community-filter")
            .flex()
            .flex_wrap()
            .items_center()
            .w_full()
            .gap_1();
        let all_label = if selected.is_none() {
            "✓ All chats"
        } else {
            "All chats"
        };
        chips = chips.child(
            Button::new("search-community-filter-all")
                .label(all_label)
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_search_community_filter(None, cx);
                })),
        );
        for (id, name) in communities {
            let label = if selected == Some(id) {
                format!("✓ {name}")
            } else {
                name
            };
            chips = chips.child(
                Button::new(("search-community-filter", id as u64))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_search_community_filter(Some(id), cx);
                    })),
            );
        }
        Some(chips.into_any_element())
    }

    /// Global search narrowing, shown once there is a query: chat type
    /// (tdesktop `lng_search_filter_*`), content tab and date window.
    pub(super) fn search_filter_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if session.search.query.trim().is_empty() {
            return None;
        }
        let filters = session.search.filters;
        let mut types = Vec::new();
        for kind in SearchChatType::ALL {
            types.push(
                Self::filter_chip(
                    ("search-filter-type", kind as u64),
                    kind.label(),
                    filters.chat_type == kind,
                    move |this, _, cx| {
                        let mut next = this.session().map(|s| s.search.filters).unwrap_or_default();
                        next.chat_type = kind;
                        this.set_search_filters(next, cx);
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        let mut media = Vec::new();
        for kind in SearchMediaKind::ALL {
            media.push(
                Self::filter_chip(
                    ("search-filter-media", kind as u64),
                    kind.label(),
                    filters.media == kind,
                    move |this, _, cx| {
                        let mut next = this.session().map(|s| s.search.filters).unwrap_or_default();
                        next.media = kind;
                        this.set_search_filters(next, cx);
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        let mut dates = Vec::new();
        for range in SearchDateRange::ALL {
            dates.push(
                Self::filter_chip(
                    ("search-filter-date", range as u64),
                    range.label(),
                    filters.date == range,
                    move |this, _, cx| {
                        let mut next = this.session().map(|s| s.search.filters).unwrap_or_default();
                        next.date = range;
                        this.set_search_filters(next, cx);
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        let muted = cx.theme().muted_foreground;
        let group = |label: &'static str, chips: Vec<AnyElement>| {
            div()
                .flex()
                .items_start()
                .gap_1()
                .child(
                    div()
                        .w(px(56.))
                        .flex_none()
                        .py_1()
                        .text_xs()
                        .text_color(muted)
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_1()
                        .children(chips),
                )
        };
        Some(
            div()
                .id("search-filters")
                .flex()
                .flex_col()
                .gap_1()
                .child(group("Chats", types))
                .child(group("Content", media))
                .child(group("Date", dates))
                .into_any_element(),
        )
    }

    pub(super) fn search_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let status = session
            .map(|s| s.search.status)
            .unwrap_or(SearchStatus::Closed);
        let recents = session.is_some_and(|s| s.search.recents);
        let query = session.map(|s| s.search.query.clone()).unwrap_or_default();
        let chat_ids: Vec<ChatId> = session
            .map(|s| s.search.chat_ids.clone())
            .unwrap_or_default();
        let messages: Vec<(ChatId, MessageId, String, String, i32)> = session
            .map(|s| {
                s.search
                    .messages
                    .iter()
                    .map(|hit| {
                        let title = s
                            .chats
                            .get(&hit.chat_id.0)
                            .map(|c| c.title.clone())
                            .unwrap_or_else(|| format!("chat {}", hit.chat_id.0));
                        (
                            hit.chat_id,
                            hit.message_id,
                            title,
                            hit.preview.clone(),
                            hit.date,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let chats: Vec<(ChatId, String, String)> = session
            .map(|s| {
                chat_ids
                    .into_iter()
                    .map(|id| {
                        s.chats
                            .get(&id.0)
                            .map(|chat| (chat.id, chat.title.clone(), chat.sidebar_preview()))
                            .unwrap_or_else(|| (id, format!("chat {}", id.0), String::new()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Phase 7.2: `searchPublicChats` hits (public username/title lookup).
        // TDLib excludes known chats from these results, so they are shown
        // as their own section rather than merged into `Chats`.
        let public_chats: Vec<(ChatId, String, String)> = session
            .map(|s| {
                s.search
                    .public_chat_ids
                    .iter()
                    .map(|id| {
                        s.chats
                            .get(&id.0)
                            .map(|chat| (chat.id, chat.title.clone(), chat.sidebar_preview()))
                            .unwrap_or_else(|| (*id, format!("chat {}", id.0), String::new()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let has_results = !chats.is_empty() || !public_chats.is_empty() || !messages.is_empty();
        // Only states the results don't already show: still loading with
        // nothing yet, nothing found, failure.
        let hint = match status {
            SearchStatus::Idle => "Type to search chats and messages.".to_string(),
            SearchStatus::Searching if has_results => String::new(),
            SearchStatus::Searching if recents => "Loading recent chats…".to_string(),
            SearchStatus::Searching => format!("Searching “{query}”…"),
            SearchStatus::Ready => String::new(),
            SearchStatus::Empty => format!("No chats or messages match “{query}”."),
            SearchStatus::Failed => "Search failed.".to_string(),
            SearchStatus::Closed => String::new(),
        };
        let chat_heading = if recents { "Recent" } else { "Chats" };
        div()
            .id("search-results")
            .flex()
            .flex_col()
            .gap_2()
            .when_some(self.search_filter_bar(cx), |this, bar| this.child(bar))
            .when_some(self.search_community_filter_chips(cx), |this, chips| {
                this.child(chips)
            })
            .when(!hint.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(hint),
                )
            })
            .when(!chats.is_empty(), |this| {
                let mut block = div().id("search-chats").flex().flex_col().gap_1().child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_xs().font_semibold().child(chat_heading))
                        // Slice CL2: "Clear" on the Recent heading —
                        // `clearRecentlyFoundChats` (TGX
                        // `SearchManager.clearRecentlyFoundChats`).
                        .when(recents, |this| {
                            this.child(
                                Button::new("clear-search-recents")
                                    .label("Clear")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.clear_search_recents(cx);
                                    })),
                            )
                        }),
                );
                for (id, title, preview) in chats {
                    let photo = self.chat_photo_for_row(id);
                    block = block.child(search_result_row(
                        ("search-chat", id.0 as u64),
                        title,
                        preview,
                        None,
                        &query,
                        photo,
                        cx,
                        move |this, window, cx| this.select_search_chat(id, window, cx),
                    ));
                }
                this.child(block)
            })
            .when(!public_chats.is_empty(), |this| {
                let mut block = div()
                    .id("search-public-chats")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child("Public chats"));
                for (id, title, preview) in public_chats {
                    let photo = self.chat_photo_for_row(id);
                    block = block.child(search_result_row(
                        ("search-public-chat", id.0 as u64),
                        title,
                        preview,
                        None,
                        &query,
                        photo,
                        cx,
                        move |this, window, cx| this.select_search_chat(id, window, cx),
                    ));
                }
                this.child(block)
            })
            .when(!messages.is_empty(), |this| {
                let mut block = div()
                    .id("search-messages")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child("Messages"));
                for (chat_id, message_id, title, preview, date) in messages {
                    let photo = self.chat_photo_for_row(chat_id);
                    block = block.child(search_result_row(
                        ("search-msg", message_id.0 as u64),
                        title,
                        preview,
                        Some(date),
                        &query,
                        photo,
                        cx,
                        move |this, window, cx| {
                            this.select_search_message(chat_id, message_id, window, cx);
                        },
                    ));
                }
                this.child(block)
            })
    }
}

impl QuillApp {
    /// A chat's downloaded photo, sandboxed for display (search rows).
    fn chat_photo_for_row(&self, chat_id: ChatId) -> Option<std::path::PathBuf> {
        self.session()
            .and_then(|s| s.chat_photo_path(chat_id))
            .and_then(|path| {
                quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
            })
    }
}

#[cfg(test)]
mod match_tests {
    use super::match_ranges;

    #[test]
    fn matches_case_insensitively_and_skips_unsafe_text() {
        assert_eq!(match_ranges("Hello hello", "HELLO"), vec![0..5, 6..11]);
        assert!(match_ranges("Hello", "").is_empty());
        // Lowercasing İ changes the byte length: no highlight at all.
        assert!(match_ranges("İstanbul hello", "hello").is_empty());
    }
}

#[cfg(test)]
mod one_line_tests {
    use super::one_line_preview;

    #[test]
    fn line_breaks_become_spaces() {
        assert_eq!(one_line_preview("a\nb\r\nc\u{2028}d"), "a b  c d");
        assert_eq!(one_line_preview("🫠 Galaxy\nSamsung"), "🫠 Galaxy Samsung");
    }
}
