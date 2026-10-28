//! conversation header, typing indicators.
#![deny(clippy::indexing_slicing)]

use super::app::{PaneMode, QuillApp, pane_placeholder};
use super::group_panels::SupergroupHeaderExtras;
use super::history::HistoryShared;
use super::history::{album_history_row, history_skeleton, session_history_row};
use super::synthetic::BubbleLook;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::message_scroller::MessageScroller;
use gpui_kit::component::*;
use gpui_kit::gpui::StyleRefinement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::poll::chat_allows_polls;
use quill::state::{
    ChatSearchJump, ChatSummary, HistoryMessage, InfoPanelTarget, OutboxReceipt, Session,
};
use quill::subsection_tabs::SubsectionTabsMode;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ChatKind, MessageContent, MessageSender};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// Rows before the end of a window that stops short of the latest
/// message at which the next newer page is requested.
const NEWER_PREFETCH_ROWS: usize = 8;
/// More rows than this arriving at once (a page, not a message) just appear.
const NEW_ROW_REVEAL_MAX: usize = 3;
pub(super) fn apply_ready_typing(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let json = r#"{"@type":"updateChatAction","chat_id":11,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":11},"action":{"@type":"chatActionTyping"}}"#;
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

impl QuillApp {
    pub(super) fn conversation_header(
        &self,
        title: &str,
        actions: Option<(ChatId, bool, bool, bool)>,
        typing: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (chat_id, muted, forever, _) = actions.unwrap_or((ChatId(0), false, false, false));
        // Phase 6: the header title opens the info panel for private chats
        // (user profile) and supergroups/channels (group info). Other chat
        // kinds keep the plain title.
        let info_target = actions.and_then(|(chat_id, _, _, _)| {
            self.session().and_then(|s| s.chat_info_target(chat_id))
        });
        // A comment / reply thread titles the header "Discussion" with the
        // reply count as its status line (tdesktop `TopBarWidget` in
        // `Section::Replies`: `lng_manage_discussion_group` plus the custom
        // title); the info panel does not apply.
        let thread_header: Option<(String, String, bool)> =
            actions.and_then(|(chat_id, _, _, _)| {
                let session = self.session()?;
                let thread = session.thread_for_chat(chat_id)?;
                Some((
                    tr(keys::Q_CHAT_DISCUSSION).to_string(),
                    thread.subtitle(),
                    thread.is_comments(),
                ))
            });
        // The info column is the main window's; a chat window's header
        // leaves its toggle out.
        let in_window = self.windows.in_window();
        let info_target = info_target.filter(|_| thread_header.is_none() && !in_window);
        // Parity slice: channel/supergroup header extras — photo,
        // description snippet, primary @username, subscriber/member count,
        // and the linked discussion chat ("Discuss").
        let extras: Option<SupergroupHeaderExtras> = actions.and_then(|(chat_id, _, _, _)| {
            let session = self.session()?;
            let chat = session.chats.get(&chat_id.0)?;
            let (members, online) = session.group_member_counts(chat)?;
            let (is_channel, username) = match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel,
                } => (
                    is_channel,
                    session
                        .supergroup_username(supergroup_id)
                        .filter(|name| !name.is_empty())
                        .map(|name| name.to_string()),
                ),
                _ => (false, None),
            };
            Some(SupergroupHeaderExtras {
                is_channel,
                username,
                member_count: (members > 0).then_some(members),
                online_count: online,
            })
        });
        // Your own chat reads "Saved Messages" with a bookmark and no
        // presence line, as in Telegram Desktop.
        let saved = actions.is_some_and(|(chat_id, _, _, _)| {
            self.session().is_some_and(|s| s.is_saved_messages(chat_id))
        });
        // Subsection tabs: an open tab topic titles the header with its
        // name and message count, as Telegram Desktop does.
        let topic_header = actions.and_then(|(chat_id, _, _, _)| {
            self.session()
                .and_then(|s| s.subsection_topic_header(chat_id))
        });
        // Saved Messages: a sublist titles the header with its source chat,
        // a tag filter with the tag.
        let saved_header = self.saved_header();
        let title_text = if let Some((name, _, _)) = &thread_header {
            name.clone()
        } else if let Some((name, _)) = &saved_header {
            name.clone()
        } else if saved {
            tr(keys::SAVED_MESSAGES).to_string()
        } else if let Some((name, _)) = &topic_header {
            name.clone()
        } else {
            title.to_string()
        };
        // Phase B4: chat-level auto-delete / self-destruct timer status
        // (`chat.message_auto_delete_time`, schema 1.8.67 lines 3616 /
        // 3627) — shown under the title when a timer is set.
        let ttl_line: Option<String> = actions.and_then(|(chat_id, _, _, _)| {
            self.session()
                .and_then(|s| s.chats.get(&chat_id.0))
                .and_then(|chat| chat.ttl_status_line())
        });
        // Peer activity line: typing wins over other actions, as in
        // tdesktop's `SendActionPainter`.
        let activity_line: Option<quill::state::ActivityLine> =
            actions.and_then(|(chat_id, _, _, _)| {
                self.session()
                    .and_then(|s| s.chats.get(&chat_id.0))
                    .and_then(|chat| chat.peer_activity())
            });
        // Non-typing activity (recording, uploading...) sets no typing
        // senders, so `typing` alone would hide it.
        let typing = typing || activity_line.is_some();
        // One identity block for every chat kind: avatar, title, and a
        // single status line — activity wins, then secret-chat state, then
        // presence or member count, then the muted / timer notes.
        let session = self.session();
        let private_user = match info_target {
            Some(InfoPanelTarget::User(user_id)) => Some(user_id),
            _ => None,
        };
        let presence = private_user.filter(|_| !saved).and_then(|user_id| {
            let user = session?.user(user_id)?;
            if user.is_bot {
                Some(("bot".to_string(), false))
            } else {
                let line = user.status.display();
                (!line.is_empty()).then(|| (line, user.status.is_online()))
            }
        });
        let meta_line = extras.as_ref().and_then(|ex| {
            let mut meta: Vec<String> = Vec::new();
            if let Some(count) = ex.member_count.filter(|count| *count > 0) {
                let line = quill::inline_members::member_status_line(
                    count,
                    ex.is_channel,
                    ex.online_count,
                );
                meta.push(line);
            }
            if let Some(username) = &ex.username {
                meta.push(format!("@{username}"));
            }
            (!meta.is_empty()).then(|| meta.join(" · "))
        });
        let secret_line = self.secret_pending_subtitle(chat_id);
        // The conversation's animation layer ticks the dots on its own.
        if typing && super::anim_layer::current().is_none() {
            self.request_animation_tick(super::activity_indicator::FPS, cx);
        }
        let status_indicator = if typing {
            Some(
                activity_line
                    .as_ref()
                    .map_or(quill::state::ActivityIndicator::Dots, |l| l.indicator),
            )
        } else {
            None
        };
        let (status_line, status_accent): (Option<String>, bool) = if typing {
            (
                Some(activity_line.map_or_else(|| "typing".to_string(), |l| l.text)),
                true,
            )
        } else if let Some((_, line, _)) = thread_header.clone() {
            (Some(line), false)
        } else if let Some((_, line)) = saved_header.clone() {
            (Some(line), false)
        } else if let Some((_, line)) = topic_header.filter(|(_, line)| !line.is_empty()) {
            (Some(line), false)
        } else if let Some(line) = secret_line {
            (Some(line), true)
        } else if let Some((line, online)) = presence {
            (Some(line), online)
        } else if let Some(line) = meta_line {
            (Some(line), false)
        } else if muted {
            (
                Some(
                    if forever {
                        tr(keys::Q_CHAT_MUTED_FOREVER)
                    } else {
                        "muted"
                    }
                    .to_string(),
                ),
                false,
            )
        } else {
            (ttl_line.clone(), false)
        };
        // Verified check, Premium status or star, SCAM / FAKE chip after the
        // title. Threads, Saved Messages and topic views title the header
        // with something else, so they carry none.
        let badges = if thread_header.is_some() || saved || saved_header.is_some() {
            Vec::new()
        } else {
            actions
                .and_then(|(chat_id, _, _, _)| {
                    let session = self.session()?;
                    let chat = session.chats.get(&chat_id.0)?;
                    Some(session.chat_header_badges(chat))
                })
                .unwrap_or_default()
        };
        let chat_id_for_status = actions.map_or(0, |(chat_id, _, _, _)| chat_id.0);
        let verify_icon = if thread_header.is_some() || saved || saved_header.is_some() {
            None
        } else {
            actions.and_then(|(chat_id, _, _, _)| {
                let chat = self.session()?.chats.get(&chat_id.0)?;
                self.bot_verify_icon_path(chat)
            })
        };
        let identity = self.conversation_identity(
            header_view::HeaderIdentity {
                title: title_text,
                verify_icon,
                badges,
                status_owner: chat_id_for_status,
                status_line,
                status_accent,
                status_indicator,
                info_target,
            },
            cx,
        );
        let can_call = private_user.is_some_and(|user_id| {
            session.is_some_and(|s| Self::can_start_secret_chat_with(s, user_id))
        });
        let header = div()
            .id("conversation-header")
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                // The title truncates; the action buttons keep their size.
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .gap_1()
                    .min_w_0()
                    .children(self.sidebar_toggle_button(cx))
                    .when(thread_header.is_some(), |this| {
                        this.child(self.thread_back_button(cx))
                    })
                    .when(saved_header.is_some(), |this| {
                        this.child(self.saved_back_button(cx))
                    })
                    .when(thread_header.is_none() && saved_header.is_none(), |this| {
                        this.children(self.chat_back_button(cx))
                    })
                    .child(identity),
            )
            .when(actions.is_some(), |this| {
                this.child(self.conversation_header_actions(
                    chat_id,
                    thread_header.as_ref(),
                    private_user.filter(|_| can_call),
                    info_target,
                    cx,
                ))
            });
        self.with_selection_bar(chat_id, header, cx)
    }

    /// Whether the open chat shows a composer at all.
    pub(super) fn composer_available(&self, mode: PaneMode) -> bool {
        match mode {
            PaneMode::Synthetic => true,
            PaneMode::Connecting => false,
            PaneMode::Ready => {
                let session = self.session();
                // A frozen account is read-only everywhere.
                if session.is_some_and(|s| s.is_frozen()) {
                    return false;
                }
                let open = session.and_then(|s| s.open_chat);
                let chat = open.and_then(|id| session.and_then(|s| s.chats.get(&id.0)));
                // Parity slice 4: posting into a forum topic is supported —
                // `sendMessage` carries `topic_id = messageTopicForum`
                // (schema 1.8.67, lines 12200 / 3004). Closed topics and
                // chats without the basic send permission keep the composer
                // hidden.
                let topic = open.and_then(|id| session.and_then(|s| s.open_topic_info(id)));
                let in_topic = session.is_some_and(|s| s.open_topic.is_some());
                // A group that lets the viewer send nothing swaps the
                // composer for the reason (`composer_restriction`).
                let restricted = self.composer_restriction().is_some();
                match (chat, topic) {
                    (Some(c), Some(t)) => {
                        c.can_post() && !t.is_closed && c.can_send_basic_messages && !restricted
                    }
                    // Saved sublists and tag filters are read-only views.
                    (Some(_), None) if self.saved_readonly() => false,
                    (Some(c), None) if !in_topic => {
                        c.can_post() && !restricted && self.bottom_action().is_none()
                    }
                    // In a topic whose info hasn't loaded yet: hide the
                    // composer until it arrives (the note says "Loading
                    // topic…").
                    _ => false,
                }
            }
        }
    }
}

/// Full-width "N Unread Messages" bar above the first unread message. In
/// the wide layout the label is centred in the wide column while the bar
/// stays full width (tdesktop `UnreadBar::paint`).
fn unread_divider(count: i32, wide: bool, a11y_focused: bool, cx: &App) -> impl IntoElement {
    let text = SharedString::from(quill::state::unread_bar_text(count));
    div()
        .id("unread-divider")
        // Screen-reader focus on the bar (`history_a11y`).
        .when(a11y_focused, |this| this.aria_active_descendant())
        .w_full()
        .my_2()
        .py_1()
        .bg(super::chat_palette::service_bg(
            cx.theme().secondary.opacity(0.7),
        ))
        .text_xs()
        .font_medium()
        .text_color(cx.theme().secondary_foreground)
        .role(Role::Heading)
        .aria_label(text.clone())
        .child(super::chat_wide_ui::wide_column(
            wide,
            div().w_full().flex().justify_center().child(text),
        ))
}

/// Centered local-day pill between history rows ("Today", "12 March").
/// Clicking it opens the "Jump to date" calendar, like tdesktop.
fn day_separator(label: &str, cx: &mut Context<QuillApp>) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .justify_center()
        .pt_3()
        .pb_1()
        .child(date_pill_button(label, cx))
}

/// A day pill that opens the calendar box.
pub(super) fn date_pill_button(label: &str, cx: &mut Context<QuillApp>) -> impl IntoElement {
    pill_label(label, cx)
        .cursor_pointer()
        .on_click(cx.listener(|this, _, _, cx| this.open_jump_date_ui(cx)))
}

/// The day pill itself, shared by the inline separators and the floating
/// date shown while scrolling.
pub(super) fn pill_label(label: &str, cx: &App) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("day-{label}")))
        .px_3()
        .py_0p5()
        .rounded_full()
        .bg(super::chat_palette::service_bg(cx.theme().secondary))
        .text_xs()
        .font_medium()
        .text_color(cx.theme().secondary_foreground)
        .role(Role::Heading)
        .aria_label(SharedString::from(label.to_string()))
        .child(label.to_string())
}

impl QuillApp {
    /// Report the history rows whose bottom edge was on screen at the last
    /// paint as seen (`viewMessages` through the driver), so only messages
    /// the reader could see are marked read. Skipped while the window is inactive —
    /// nothing is "seen" behind another app — and when the set is
    /// unchanged; the driver also drops ids already viewed or in flight.
    pub(super) fn report_visible_history(&mut self, window_active: bool, cx: &mut Context<Self>) {
        self.history.window_active = window_active;
        if !window_active {
            return;
        }
        let Some(chat_id) = self.viewed_chat() else {
            return;
        };
        let ids = match self.history.seen.try_borrow().as_deref() {
            Ok(Some((chat, ids))) if *chat == chat_id && !ids.is_empty() => ids.clone(),
            _ => return,
        };
        if self
            .history
            .reported_visible
            .as_ref()
            .is_some_and(|(chat, last)| *chat == chat_id && *last == ids)
        {
            return;
        }
        // Before `viewMessages`, while the rows still count as unread: an
        // incoming lone emoji plays its interaction effect as it is read.
        let paused = quill::power_saving::on(quill::power_saving::Flag::EmojiChat);
        if let Some(live) = self.live.as_mut() {
            let now_ms = quill::state::unix_ms_now();
            let _ = live
                .driver
                .autoplay_emoji_effects(chat_id, &ids, paused, now_ms);
            // Recorded even when the send fails: the driver keeps the ids
            // queued for the next attempt, and re-reporting every frame
            // would only spin the render loop.
            let queued = matches!(live.driver.view_messages(chat_id, &ids), Ok(Some(_)));
            self.history.reported_visible = Some((chat_id, ids));
            if queued {
                self.notify_tick(crate::ui::app_slice::SliceKind::Conversation, cx);
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.autoplay_emoji_effects(chat_id, &ids, paused, rand::random());
            self.history.reported_visible = Some((chat_id, ids));
        }
    }
}

/// Which part of the conversation column `QuillApp::conversation` builds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ConversationPart {
    /// Call bars, history, bottom tabs and banners: all but the composer.
    Top,
    /// The composer with its popups, or the note / channel footer that
    /// replaces it.
    Bottom,
}

impl ConversationPart {
    pub(super) fn top(self) -> bool {
        self == Self::Top
    }

    pub(super) fn bottom(self) -> bool {
        self == Self::Bottom
    }
}

mod conversation_part1;
mod header_view;
mod history_message_list;
mod history_reads;
mod history_scroll_memory;
mod render_history_row_body;
mod row_build;
pub(super) mod row_chrome;
mod row_splice;
mod rows_cache;
pub(super) use rows_cache::{HistoryRowsKey, RowsInput};
