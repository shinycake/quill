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
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::draft_text_to_store;
use quill::connect::{ChatSearchQueryOutcome, SEARCH_DEBOUNCE, SearchQueryOutcome};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::search_filters::{
    SearchChatType, SearchDateRange, SearchMediaKind, SearchScope, tag_query,
};
use quill::state::{
    ChatSearchJump, FromPicker, RequestPurpose, SearchConfirm, SearchStatus, Session,
};
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
    let owner = cx.entity().downgrade();
    let row = div()
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
        });
    // Telegram Desktop's "Go To Message" on a result.
    div().w_full().child(row).context_menu(move |menu, _, _| {
        let owner = owner.clone();
        menu.item(
            gpui_kit::component::menu::PopupMenuItem::new("Go To Message")
                .icon(gpui_kit::assets::IconName::MessageSquare)
                .on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.jump_chat_search_message(message_id, cx);
                    });
                }),
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
    match session.search.chat_search.jump {
        ChatSearchJump::None => String::new(),
        ChatSearchJump::Loading { .. } => "Loading message…".into(),
        ChatSearchJump::Ready { .. } => String::new(),
        ChatSearchJump::Missing { .. } => "This message is unavailable.".into(),
    }
}

pub(super) fn apply_ready_search(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    session.open_search();
    let search_gen = session.search.search.begin_query("hello");
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
    let search_gen = session.search.chat_search.begin_query("hello");
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
        self.session()
            .is_some_and(|session| session.search.search.open)
    }

    pub(super) fn chat_search_is_open(&self) -> bool {
        self.session()
            .is_some_and(|session| session.search.chat_search.open)
    }

    pub(super) fn open_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            match live.driver.open_search() {
                Ok(Some(_)) => self.connection.status_note = "searching…".into(),
                Ok(None) => self.connection.status_note = "search chats and messages".into(),
                Err(_) => self.connection.status_note = "could not search".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_search();
            self.connection.status_note = "search chats and messages".into();
        }
        self.search_ui
            .input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn close_search_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_search();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.close_search();
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.session.clear_story_search();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.clear_story_search();
        }
        self.search_ui
            .input
            .update(cx, |input, cx| input.set_value("", window, cx));
        window.focus(&self.focus_sidebar, cx);
        self.connection.status_note = "search closed".into();
        cx.notify();
    }

    /// Escape: closes the topmost open layer (see `esc_stack`). The lock
    /// screen ignores it.
    pub(super) fn cancel_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_topmost_layer(window, cx);
    }

    /// Slice CL2: "Clear" on the Recent searches heading —
    /// `clearRecentlyFoundChats` (schema 1.8.67, line 11671); the
    /// recents clear optimistically (TGX `SearchManager` clears
    /// locally too, lines 788-796).
    pub(super) fn clear_search_recents(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.clear_recently_found_chats() {
                self.connection.status_note = format!("clear recents failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.search.chat_ids.clear();
            session.search.search.status = SearchStatus::Idle;
        }
        cx.notify();
    }

    /// "Remove from Recent" on one recent search (`removeRecentlyFoundChat`).
    pub(super) fn remove_recent_search(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.remove_recent_search(chat_id) {
                self.connection.status_note =
                    format!("could not remove the recent search: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.search.remove_recent(chat_id);
        }
        cx.notify();
    }

    /// "Remove from Recent" on a frequent contact (`removeTopChat`).
    pub(super) fn remove_top_chat(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.remove_top_chat(chat_id) {
                self.connection.status_note = format!("could not remove the contact: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.search.remove_top_chat(chat_id);
        }
        cx.notify();
    }

    /// "Suggest frequent contacts" (Privacy) and "Remove all & Disable".
    pub(super) fn set_top_chats_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_top_chats_disabled(disabled) {
                self.connection.status_note =
                    format!("could not change frequent contacts: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.search.top_chats_disabled = disabled;
            if disabled {
                session.search.search.top_chats.clear();
            }
        }
        cx.notify();
    }

    /// Show (or dismiss) an inline confirmation / the frequent-contact row.
    pub(super) fn set_search_prompt(
        &mut self,
        confirm: Option<SearchConfirm>,
        top_menu: Option<ChatId>,
        cx: &mut Context<Self>,
    ) {
        let search = if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.search.search)
        } else {
            self.demo_session.as_mut().map(|s| &mut s.search.search)
        };
        if let Some(search) = search {
            search.confirm = confirm;
            search.top_menu = top_menu;
        }
        cx.notify();
    }

    /// The user accepted the pending confirmation.
    pub(super) fn accept_search_confirm(&mut self, cx: &mut Context<Self>) {
        let confirm = self.session().and_then(|s| s.search.search.confirm);
        self.set_search_prompt(None, None, cx);
        match confirm {
            Some(SearchConfirm::ClearRecents) => self.clear_search_recents(cx),
            Some(SearchConfirm::DisableTopChats) => self.set_top_chats_disabled(true, cx),
            None => {}
        }
    }

    /// Hashtag in another scope (tdesktop's My Messages / This Chat / Public
    /// Posts tabs): leaves the in-chat search for the global panel.
    pub(super) fn search_tag_in_scope(
        &mut self,
        tag: &str,
        scope: SearchScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_search_is_open() {
            self.close_chat_search_ui(window, cx);
        }
        self.open_search_ui(window, cx);
        let query = tag.to_string();
        self.search_ui
            .input
            .update(cx, |input, cx| input.set_value(&query, window, cx));
        if let Some(live) = self.live.as_mut() {
            match live.driver.search_hashtag(tag, scope) {
                Ok(_) => self.connection.status_note = "searching…".into(),
                Err(_) => self.connection.status_note = "could not search".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.search.filters.scope = scope;
            session.apply_local_search_filter(tag);
        }
        cx.notify();
    }

    /// "This chat" tab of the global search: the same query, inside the
    /// open chat.
    pub(super) fn search_query_in_this_chat(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_search_ui(window, cx);
        self.open_chat_search_ui(window, cx);
        let text = query.to_string();
        self.search_ui
            .chat_input
            .update(cx, |input, cx| input.set_value(&text, window, cx));
        self.sync_chat_search_query(query, cx);
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
                self.connection.status_note = format!("community filter failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.search.community_filter = community_id;
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
                    self.connection.status_note = "search in chat".into();
                    true
                }
                Ok(false) => {
                    self.connection.status_note = "select a chat to search in conversation".into();
                    false
                }
                Err(_) => {
                    self.connection.status_note = "could not search in chat".into();
                    false
                }
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            if session.open_chat_search() {
                self.connection.status_note = "search in chat".into();
                true
            } else {
                self.connection.status_note = "select a chat to search in conversation".into();
                false
            }
        } else {
            false
        };
        if opened {
            self.search_ui
                .chat_input
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
        self.search_ui
            .chat_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.connection.status_note = "in-chat search closed".into();
        cx.notify();
    }

    pub(super) fn sync_chat_search_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.pane_mode() != PaneMode::Ready || !self.chat_search_is_open() {
            return;
        }
        // While choosing a "From:" member the field filters the members.
        if self
            .session()
            .is_some_and(|s| s.search.chat_search.from_picker.is_some())
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
                    self.connection.status_note = "searching in chat…".into()
                }
                Ok(ChatSearchQueryOutcome::Debounced { token }) => {
                    self.schedule_chat_search_commit(token, cx);
                }
                Ok(ChatSearchQueryOutcome::Unchanged) if query.trim().is_empty() => {
                    self.connection.status_note = "search in chat".into();
                }
                Ok(ChatSearchQueryOutcome::Unchanged) => {}
                Err(_) => self.connection.status_note = "could not search in chat".into(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let trimmed = query.trim();
            if session.search.chat_search.open
                && session.search.chat_search.query == trimmed
                && !matches!(session.search.chat_search.status, SearchStatus::Closed)
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
                        Ok(Some(_)) => this.connection.status_note = "searching in chat…".into(),
                        Ok(None) => {}
                        Err(_) => this.connection.status_note = "could not search in chat".into(),
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
            self.connection.status_note = match live.driver.jump_selected_chat_search_hit() {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(id) = session
                .search
                .chat_search
                .selected_hit()
                .map(|hit| hit.message_id)
            {
                let _ = session.begin_chat_search_jump(id);
            }
            self.connection.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    pub(super) fn jump_chat_search_message(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.jump_to_chat_search_message(message_id)
            {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.connection.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    pub(super) fn chat_search_newer(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.chat_search_newer() {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(id) = session.search.chat_search.select_newer() {
                let _ = session.begin_chat_search_jump(id);
            }
            self.connection.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    pub(super) fn chat_search_older(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.chat_search_older() {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(id) = session.search.chat_search.select_older() {
                let _ = session.begin_chat_search_jump(id);
            }
            self.connection.status_note = chat_search_jump_note(session);
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
                self.connection.status_note = "could not search in chat".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.chat_search.sender = sender;
            session.close_from_picker();
        }
        // The field's text was the member filter while choosing: start the
        // message query from scratch.
        self.search_ui
            .chat_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.search_ui
            .chat_input
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
                self.connection.status_note = "could not search in chat".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.chat_search.media = media;
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
            .is_some_and(|s| s.search.chat_search.from_picker.is_some());
        if open {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.close_from_picker();
            } else if let Some(session) = self.demo_session.as_mut() {
                session.close_from_picker();
            }
        } else if let Some(live) = self.live.as_mut() {
            if live.driver.open_chat_search_from_picker().is_err() {
                self.connection.status_note = "members are not available here".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_from_picker();
        }
        // The field now filters members (or messages again): start empty.
        self.search_ui
            .chat_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.search_ui
            .chat_input
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
                self.connection.status_note = format!("could not apply search filters: {err:?}");
            }
            if filters.scope == SearchScope::Apps {
                let _ = live.driver.fetch_grossing_web_app_bots();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.search.search.filters = filters;
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
}

impl QuillApp {
    /// A chat's downloaded photo, sandboxed for display (search rows).
    pub(super) fn chat_photo_for_row(&self, chat_id: ChatId) -> Option<std::path::PathBuf> {
        self.session()
            .and_then(|s| s.chat_photo_path(chat_id))
            .and_then(|path| {
                quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
            })
    }
}

mod chat_search_bar;
mod search_results;

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
