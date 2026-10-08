//! conversation header, typing indicators.

use super::app::{PaneMode, QuillApp, pane_placeholder};
use super::chat_row::compact_count;
use super::group_panels::SupergroupHeaderExtras;
use super::history::HistoryShared;
use super::history::{album_history_row, history_skeleton, session_history_row};
use super::pressable::PressableDiv;
use super::recording::RecordMode;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::message_scroller::MessageScroller;
use gpui_kit::component::*;
use gpui_kit::gpui::StyleRefinement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::poll::chat_allows_polls;
use quill::state::{
    ChatSearchJump, ChatSummary, HistoryMessage, InfoPanelTarget, OutboxReceipt, Session,
};
use quill::subsection_tabs::SubsectionTabsMode;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ChatKind, MessageContent, MessageSender};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// Everything the history rows are derived from, compared across renders
/// so unchanged rows aren't rebuilt (typing in the composer or a scroll
/// re-render used to snapshot and rebuild every loaded message).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct HistoryRowsKey {
    list: &'static str,
    chat: Option<i64>,
    topic: Option<i32>,
    /// Any change to the loaded messages (`HistoryMessages::revision`).
    messages_revision: u64,
    /// Any applied TDLib update (chat read state, users, quoted messages).
    session_revision: u64,
    window_epoch: u64,
    unread_anchor: Option<MessageId>,
    highlight: Option<MessageId>,
    ui: u64,
    /// Local day: "Today"/"Yesterday" labels roll over at midnight.
    today: i64,
}

/// Rows before the end of a window that stops short of the latest
/// message at which the next newer page is requested.
const NEWER_PREFETCH_ROWS: usize = 8;
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
            self.session()
                .and_then(|s| s.info_panel_target_for_chat(chat_id))
        });
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
        let title_text = if saved {
            "Saved Messages".to_string()
        } else if let Some((name, _)) = &topic_header {
            name.clone()
        } else {
            title.to_string()
        };
        let muted_fg = cx.theme().muted_foreground;
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
                let noun = match (ex.is_channel, count) {
                    (true, 1) => "subscriber",
                    (true, _) => "subscribers",
                    (false, 1) => "member",
                    (false, _) => "members",
                };
                let mut line = format!("{} {noun}", compact_count(count));
                // Groups: "N members, M online" (yourself alone isn't news).
                if !ex.is_channel && ex.online_count > 1 {
                    line.push_str(&format!(", {} online", compact_count(ex.online_count)));
                }
                meta.push(line);
            }
            if let Some(username) = &ex.username {
                meta.push(format!("@{username}"));
            }
            (!meta.is_empty()).then(|| meta.join(" · "))
        });
        let secret_line = self.secret_pending_subtitle(chat_id);
        if typing {
            self.request_animation_tick(12, cx);
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
                Some(if forever { "muted forever" } else { "muted" }.to_string()),
                false,
            )
        } else {
            (ttl_line.clone(), false)
        };
        let identity = div()
            .id("conversation-identity")
            .flex()
            .items_center()
            .gap_3()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(div().font_semibold().truncate().child(title_text))
                    .when_some(status_line, |this, line| {
                        this.child(
                            div()
                                .id("conversation-status")
                                .flex()
                                .items_center()
                                .text_xs()
                                .text_color(if status_accent {
                                    cx.theme().primary
                                } else {
                                    muted_fg
                                })
                                .when_some(status_indicator, |this, indicator| {
                                    this.child(super::activity_indicator::activity_indicator(
                                        indicator,
                                        cx.theme().primary,
                                        "header-activity".into(),
                                    ))
                                })
                                .child(div().min_w_0().truncate().child(line)),
                        )
                    }),
            )
            .when_some(info_target, |this, target| {
                this.role(gpui_kit::Role::Button)
                    .aria_label("Open conversation information")
                    .tab_index(0)
                    .cursor_pointer()
                    .rounded_md()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_info_panel_target(target, window, cx);
                    }))
            })
            .into_any_element();
        let can_call = private_user.is_some_and(|user_id| {
            session.is_some_and(|s| Self::can_start_secret_chat_with(s, user_id))
        });
        let chat_search_open = self.chat_search_is_open();
        div()
            .id("conversation-header")
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(identity)
            .when(actions.is_some(), |this| {
                // Phase C3a: voice-chat affordance for groups/channels —
                // join the live voice chat, or start one when none is
                // live. Signaling only (no audio transport yet).
                let (voice_ok, voice_live) = self
                    .session()
                    .and_then(|s| s.chats.get(&chat_id.0))
                    .map(|c| {
                        let kind_ok = matches!(
                            c.kind,
                            ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. }
                        );
                        (kind_ok, kind_ok && c.video_chat.is_some())
                    })
                    .unwrap_or((false, false));
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .when_some(private_user.filter(|_| can_call), |this, user_id| {
                            this.child(
                                Button::new("chat-call")
                                    .icon(gpui_kit::assets::IconName::Phone)
                                    .ghost()
                                    .tooltip("Call")
                                    .accessibility_label("Call")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.start_call_for_user(user_id, false, cx);
                                    })),
                            )
                        })
                        .child(
                            Button::new("chat-find")
                                .icon(if chat_search_open {
                                    gpui_kit::assets::IconName::X
                                } else {
                                    gpui_kit::assets::IconName::Search
                                })
                                .ghost()
                                .tooltip(if chat_search_open {
                                    "Close search"
                                } else {
                                    "Search this chat"
                                })
                                .accessibility_label("Search this chat")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if this.chat_search_is_open() {
                                        this.close_chat_search_ui(window, cx);
                                    } else {
                                        this.open_chat_search_ui(window, cx);
                                    }
                                })),
                        )
                        .when(voice_ok, |this| {
                            this.child(
                                Button::new("chat-voice-chat")
                                    .icon(gpui_kit::assets::IconName::AudioLines)
                                    .ghost()
                                    .when(voice_live, |button| button.selected(true))
                                    .tooltip(if voice_live {
                                        "Join voice chat"
                                    } else {
                                        "Start voice chat"
                                    })
                                    .accessibility_label(if voice_live {
                                        "Join voice chat"
                                    } else {
                                        "Start voice chat"
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.start_or_join_video_chat(chat_id, window, cx);
                                    })),
                            )
                        })
                        // Telegram Desktop's third-column toggle: shows or
                        // hides the chat's info panel.
                        .when_some(info_target, |this, target| {
                            let open = self
                                .session()
                                .is_some_and(|s| s.open_info_panel == Some(target));
                            this.child(
                                Button::new("chat-info-toggle")
                                    .icon(gpui_kit::assets::IconName::PanelRight)
                                    .ghost()
                                    .selected(open)
                                    .tooltip(if open { "Hide info" } else { "Show info" })
                                    .accessibility_label("Toggle chat info")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if open {
                                            this.close_info_panel(cx);
                                        } else {
                                            this.open_info_panel_target(target, window, cx);
                                        }
                                    })),
                            )
                        })
                        .child(self.chat_navigation_menu(cx)),
                )
            })
    }

    /// `parity:platform-chat-export` — start exporting a chat's history.
    /// The driver pages `getChatHistory` in the background; completion (or
    /// failure) surfaces as a status note from `poll_live`.
    pub(super) fn start_chat_export(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let title = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "chat".to_string());
        let started = self
            .live
            .as_mut()
            .is_some_and(|live| live.driver.start_chat_export(chat_id, title).is_ok());
        self.status_note = if started {
            "Exporting chat history…".into()
        } else {
            "Could not start the export (another export is running).".into()
        };
        cx.notify();
    }

    pub(super) fn conversation(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.pane_mode();
        let history = match mode {
            PaneMode::Synthetic => div()
                .id("conversation-history")
                .flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .role(Role::Log)
                .aria_label("Message history")
                .child(self.chat.clone())
                .into_any_element(),
            PaneMode::Connecting => pane_placeholder(
                "Not signed in",
                "Sign in to see your chats and messages.",
                cx,
            )
            .into_any_element(),
            PaneMode::Ready => self.session_history(cx).into_any_element(),
        };
        let composer = match mode {
            PaneMode::Synthetic => Some(true),
            PaneMode::Connecting => None,
            PaneMode::Ready => {
                let session = self.session();
                let open = session.and_then(|s| s.open_chat);
                let chat = open.and_then(|id| session.and_then(|s| s.chats.get(&id.0)));
                // Parity slice 4: posting into a forum topic is supported —
                // `sendMessage` carries `topic_id = messageTopicForum`
                // (schema 1.8.67, lines 12200 / 3004). Closed topics and
                // chats without the basic send permission keep the composer
                // hidden.
                let topic = open.and_then(|id| session.and_then(|s| s.open_topic_info(id)));
                let in_topic = session.is_some_and(|s| s.open_topic.is_some());
                let can_post = match (chat, topic) {
                    (Some(c), Some(t)) => c.can_post() && !t.is_closed && c.can_send_basic_messages,
                    (Some(c), None) if !in_topic => c.can_post(),
                    // In a topic whose info hasn't loaded yet: hide the
                    // composer until it arrives (the note says "Loading
                    // topic…").
                    _ => false,
                };
                if can_post { Some(true) } else { None }
            }
        };
        let composer_note: Option<String> = match mode {
            PaneMode::Connecting => Some("Sign in to send messages.".to_string()),
            PaneMode::Ready if composer.is_none() => {
                let open = self.session().and_then(|s| s.open_chat);
                let in_topic = self.session().is_some_and(|s| s.open_topic.is_some());
                let is_channel = open.is_some_and(|id| {
                    self.session()
                        .and_then(|s| s.chats.get(&id.0).map(|c| c.is_channel()))
                        .unwrap_or(false)
                });
                if is_channel {
                    // The join/leave footer replaces the plain note for channels.
                    None
                } else if in_topic {
                    // Parity slice 4: closed topics and a missing send
                    // permission hide the composer with an explanatory note.
                    let topic_closed = open.is_some_and(|id| {
                        self.session()
                            .and_then(|s| s.open_topic_info(id))
                            .is_some_and(|t| t.is_closed)
                    });
                    let send_allowed = open.is_some_and(|id| {
                        self.session()
                            .and_then(|s| s.chats.get(&id.0))
                            .is_some_and(|c| c.can_post() && c.can_send_basic_messages)
                    });
                    if topic_closed {
                        Some("This topic is closed — new messages are disabled.".to_string())
                    } else if !send_allowed {
                        Some("You don't have permission to post in this topic.".to_string())
                    } else {
                        // The topic's info hasn't loaded yet; the composer
                        // appears once it arrives.
                        Some("Loading topic…".to_string())
                    }
                } else if open.is_none() {
                    Some("Select a chat to start messaging.".to_string())
                } else {
                    // Phase B1: secret chats that can't send yet explain why
                    // instead of the generic unsupported note.
                    self.secret_composer_note()
                        .or_else(|| {
                            self.session()
                                .and_then(|s| {
                                    open.and_then(|id| {
                                        s.chats.get(&id.0).and_then(|c| c.kind.gate_reason())
                                    })
                                })
                                .map(str::to_string)
                        })
                        .or(Some(
                            "This conversation type is not supported yet.".to_string(),
                        ))
                }
            }
            _ => None,
        };
        // B4: `chatPermissions.can_send_polls` (schema 1.8.67 line 1070)
        // gates the Poll composer entry; absent permissions = unknown =
        // allowed.
        let polls_allowed = self.session().and_then(|s| s.open_chat).is_none_or(|id| {
            self.session()
                .and_then(|s| s.chats.get(&id.0))
                .is_none_or(|c| chat_allows_polls(c.permissions.as_ref()))
        });
        let dust = self.vanish_overlay();
        let call_bar = self.call_bar(cx).or_else(|| self.group_call_bar(cx));
        let capture_notice = self.capture_notice(cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .children(call_bar)
            .children(capture_notice)
            .child(history)
            .children(self.subsection_tabs_strip(SubsectionTabsMode::Bottom, cx))
            // Phase C2i: busy-decline banner — the calls that arrived
            // while another call was active were declined with
            // `discardCall` (TDLib has no hold/swap API). Dismissible.
            .when_some(self.call_busy_banner(cx), |this, banner| this.child(banner))
            .when(composer.is_some(), |this| {
                let show_attach = matches!(mode, PaneMode::Ready) && self.pending_edit.is_none();
                // Something to send (text, an attachment, an edit): the
                // composer shows Send instead of the mic.
                let sendable = !show_attach
                    || !self.pending_attachments.is_empty()
                    || !self.composer.read(cx).value().trim().is_empty();
                this.child(
                    div()
                        .id("composer-file-drop")
                        .relative()
                        // The emoji / sticker / GIF popover floats above the
                        // composer, anchored to its left edge.
                        .when(
                            self.media_panel_open() && self.media_panel.reaction.is_none(),
                            |this| {
                                let panel = self.media_panel(cx);
                                this.child(
                                    div()
                                        .absolute()
                                        .left(px(8.))
                                        .bottom(relative(1.))
                                        .pb_1()
                                        .child(panel),
                                )
                            },
                        )
                        // tdesktop shows the recording video message as a
                        // camera circle over the chat.
                        .when_some(self.round_record_overlay(cx), |this, circle| {
                            this.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .bottom(relative(1.))
                                    .pb(px(24.))
                                    .flex()
                                    .justify_center()
                                    .child(circle),
                            )
                        })
                        .when(
                            show_attach && !self.rich_editor_open && !self.recording_active(),
                            |this| {
                                this.on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                                    this.attach_dropped_files(paths.paths(), cx);
                                }))
                            },
                        )
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .when(self.recording_active(), |this| {
                            this.child(self.record_bar(cx))
                        })
                        .when(
                            show_attach && !self.pending_attachments.is_empty(),
                            |this| this.child(self.composer_attachment_tray(cx)),
                        )
                        .when_some(self.forward_result.clone(), |this, result| {
                            this.child(self.forward_success_banner(&result, cx))
                        })
                        .when(
                            self.pending_forward.is_some() && !self.forward_picker_open,
                            |this| {
                                this.when_some(self.pending_forward.clone(), |this, draft| {
                                    this.child(self.forward_selection_banner(&draft, cx))
                                })
                            },
                        )
                        .when_some(self.pending_delete.clone(), |this, _| {
                            this.child(self.delete_confirm_banner(cx))
                        })
                        // B4: stop-poll / stop-quiz confirm banner.
                        .when_some(self.pending_stop_poll, |this, _| {
                            this.child(self.stop_poll_confirm_banner(cx))
                        })
                        // Phase B1: close-secret-chat confirm banner.
                        .when_some(self.pending_close_secret_chat, |this, _| {
                            this.child(self.close_secret_chat_confirm_banner(cx))
                        })
                        // Phase S2: inline-bot warning banner for secret chats.
                        // Gated on the open chat still being one: a pending
                        // alert from a previous chat never renders elsewhere.
                        .when(
                            self.pending_inline_bot_alert.is_some() && self.open_chat_is_secret(),
                            |this| this.child(self.inline_bot_alert_banner(cx)),
                        )
                        .when_some(self.pending_edit.clone(), |this, edit| {
                            this.child(self.composer_edit_banner(&edit, cx))
                        })
                        .when_some(self.pending_reply.clone(), |this, reply| {
                            this.child(self.composer_reply_banner(&reply, cx))
                        })
                        // Phase 4.2: poll creation dialog above the composer.
                        .when_some(self.poll_dialog_panel(cx), |this, panel| this.child(panel))
                        // Phase D3a: invite-link creation dialog above the composer.
                        .when_some(self.invite_link_dialog_panel(cx), |this, panel| {
                            this.child(panel)
                        })
                        // Phase D3b: admin-management dialog above the composer.
                        .when_some(self.admin_dialog_panel(cx), |this, panel| this.child(panel))
                        // B1: bot custom keyboard (`replyMarkupShowKeyboard`)
                        // above the composer.
                        .when_some(self.custom_keyboard_panel(cx), |this, panel| {
                            this.child(panel)
                        })
                        // Force-reply keyboard bar (`replyMarkupForceReply`)
                        // above the composer.
                        .when_some(self.force_reply_panel(cx), |this, panel| this.child(panel))
                        // Phase 3.3: `/` command menu above the composer.
                        .when_some(self.mention_menu_dropdown(cx), |this, panel| {
                            this.child(panel)
                        })
                        .when_some(self.command_menu_dropdown(cx), |this, panel| {
                            this.child(panel)
                        })
                        // Bots slice: `@bot` inline-results dropdown above
                        // the composer (mutually exclusive with the `/`
                        // menu — see `sync_command_menu`).
                        .when_some(self.inline_results_dropdown(cx), |this, panel| {
                            this.child(panel)
                        })
                        .when_some(self.sticker_suggestions_row(cx), |this, row| {
                            this.child(row)
                        })
                        .when_some(self.animated_emoji_suggestion(cx), |this, row| {
                            this.child(row)
                        })
                        // Phase A1: slow-mode countdown. The composer stays
                        // usable (typing is fine) but sends are blocked
                        // until the wait expires; `ensure_slow_mode_tick`
                        // re-renders every second so the number counts down.
                        .when_some(self.slow_mode_wait_secs(), |this, wait| {
                            this.child(
                                div()
                                    .id("slow-mode-banner")
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(text_muted())
                                    .bg(bg_subtle())
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .child(format!("Slow mode · wait {wait}s")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(text_primary())
                                            .child("sending is paused until the timer expires"),
                                    ),
                            )
                        })
                        // Non-default send options as clearable chips; the
                        // schedule picker opens above the input.
                        // Pickers open directly above the input, next to the
                        // buttons that summon them.
                        .when(self.schedule_popup_open, |this| {
                            this.child(self.schedule_popup(cx))
                        })
                        .when_some(self.composer_options_row(cx), |this, row| this.child(row))
                        // MED4: detected-URL chip (send-time preview
                        // control) and caption bar ("Add a caption…",
                        // above/below toggle, n / max counter).
                        .when_some(self.preview_chip(cx), |this, chip| this.child(chip))
                        .when_some(self.caption_bar(cx), |this, bar| this.child(bar))
                        // kit Phase 5: the composer input row — attach and
                        // emoji/sticker pickers, the borderless growing
                        // kit Textarea (auto_grow(2, 6) on the state sizes
                        // it; no fixed height, no custom focus ring or
                        // placeholder machinery), the voice/video record
                        // button, and the round send button.
                        .child(
                            div()
                                .id("composer-input-row")
                                .flex()
                                .items_end()
                                .gap_2()
                                .when(show_attach, |row| {
                                    row.when_some(self.bot_menu_button(cx), |row, button| {
                                        row.child(button)
                                    })
                                    .child(self.attach_menu_button(polls_allowed, cx))
                                    .child(
                                        Button::new("composer-emoji")
                                            .icon(IconName::FaceSlightlySmiling)
                                            .ghost()
                                            .tooltip(if self.sticker_panel_open() {
                                                "Close stickers"
                                            } else {
                                                "Stickers"
                                            })
                                            .accessibility_label("Stickers")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_media_panel(
                                                    super::media_panel::PanelTab::Emoji,
                                                    cx,
                                                );
                                            })),
                                    )
                                    .child(self.format_menu_button(cx))
                                })
                                .child(
                                    div()
                                        .relative()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            Textarea::new(&self.composer)
                                                .appearance(false)
                                                .bordered(false)
                                                .aria_label("Message")
                                                // codex:spellcheck-native:
                                                // suggestions / Add to
                                                // Dictionary / Ignore on a
                                                // misspelled word.
                                                .context_menu({
                                                    let owner = cx.entity().downgrade();
                                                    move |_, window, cx| {
                                                        QuillApp::composer_context_menu(
                                                            owner.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    }
                                                }),
                                        )
                                        // Red wavy underlines over the
                                        // misspelled words.
                                        .when_some(
                                            self.spellcheck_underlines(cx),
                                            |wrap, lines| wrap.child(lines),
                                        ),
                                )
                                // Telegram Desktop shows the mic while there's
                                // nothing to send, and Send once there is.
                                .when(show_attach && !sendable, |row| {
                                    row.child(
                                        // MED2: click records in the current
                                        // mode; right-click flips audio/video
                                        // mode (TGX tap-to-switch,
                                        // desktop-mapped).
                                        div()
                                            .id("record-mode-wrap")
                                            .on_mouse_down(
                                                MouseButton::Right,
                                                cx.listener(|this, _, _, cx| {
                                                    this.toggle_record_mode(cx);
                                                }),
                                            )
                                            .child(
                                                Button::new("record-voice")
                                                    .icon(match self.record_mode() {
                                                        RecordMode::Audio => IconName::Mic,
                                                        RecordMode::Video => IconName::Video,
                                                    })
                                                    .ghost()
                                                    .tooltip(self.record_mode().hint())
                                                    .accessibility_label("Record")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.start_recording(cx);
                                                    })),
                                            ),
                                    )
                                })
                                .when(sendable, |row| {
                                    row.child(
                                        // Right-click: send options (silent,
                                        // schedule, link preview).
                                        div()
                                            .id("composer-send-wrap")
                                            .context_menu({
                                                let owner = cx.entity().downgrade();
                                                move |menu, _, cx| {
                                                    QuillApp::send_options_menu(
                                                        owner.clone(),
                                                        menu,
                                                        cx,
                                                    )
                                                }
                                            })
                                            .child(
                                                Button::new("composer-send")
                                                    .icon(IconName::Send)
                                                    .primary()
                                                    .rounded_full()
                                                    .tooltip("Send · right-click for options")
                                                    .accessibility_label("Send message")
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            let text = this
                                                                .composer
                                                                .read(cx)
                                                                .value()
                                                                .to_string();
                                                            // Same guard as
                                                            // Enter-to-send: text, or
                                                            // attachments without a
                                                            // caption.
                                                            if !text.trim().is_empty()
                                                                || !this
                                                                    .pending_attachments
                                                                    .is_empty()
                                                            {
                                                                this.submit_composer(
                                                                    text, window, cx,
                                                                );
                                                            }
                                                        },
                                                    )),
                                            ),
                                    )
                                }),
                        )
                        // M2: rich editor block bar + live block preview
                        // under the textarea while the editor is open.
                        .when(self.rich_editor_open, |this| {
                            this.child(self.rich_editor_bar(cx))
                        }),
                )
            })
            .when_some(composer_note, |this, note| {
                this.child(
                    div()
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(note),
                )
            })
            .when_some(self.channel_footer(cx), |this, footer| this.child(footer))
            // A deleted message's dust drifts over everything.
            .children(dust)
    }

    pub(super) fn session_history(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open = session.and_then(|s| s.open_chat);
        let title = open
            .and_then(|id| session.and_then(|s| s.chats.get(&id.0).map(|c| c.title.clone())))
            .unwrap_or_else(|| "No chat selected".into());
        let supported = open
            .and_then(|id| session.and_then(|s| s.chats.get(&id.0).map(|c| c.supported())))
            .unwrap_or(false);
        let gate = open.and_then(|id| {
            session.and_then(|s| s.chats.get(&id.0).and_then(|c| c.kind.gate_reason()))
        });
        let main_history = open.and_then(|id| session.and_then(|s| s.histories.get(&id.0)));
        let main_empty = main_history.is_none_or(|h| h.messages.is_empty());
        // kit Phase 3: owned — `history_message_list` takes `&mut self`,
        // so the chat summary can't borrow the session here.
        let chat: Option<ChatSummary> =
            open.and_then(|id| session.and_then(|s| s.chats.get(&id.0).cloned()));
        let media_roots = self.media_display_roots();
        let sender_name = title.clone();
        let chat_search_open = session.is_some_and(|s| s.chat_search.open);
        let highlight_id = session.and_then(|s| match s.chat_search.jump {
            ChatSearchJump::Ready { message_id } | ChatSearchJump::Loading { message_id } => {
                Some(message_id)
            }
            ChatSearchJump::None | ChatSearchJump::Missing { .. } => None,
        });
        let chat_actions = open.and_then(|id| {
            session.and_then(|s| s.chats.get(&id.0)).and_then(|chat| {
                chat.supported().then_some((
                    chat.id,
                    chat.is_muted(),
                    chat.notification_settings.is_muted_forever(),
                    chat.in_archive,
                ))
            })
        });
        let peer_typing = chat.as_ref().is_some_and(|c| c.is_peer_typing());
        // Phase 5.1: forum supergroups render a topic list instead of the
        // general history; a selected topic renders its own history.
        let is_forum = chat.as_ref().is_some_and(|c| c.is_forum_chat());
        // Subsection tabs: bots with topics and forums with tabs show "All"
        // (the chat) plus topic tabs instead of the topic list.
        let has_topics = open.is_some_and(|id| session.is_some_and(|s| s.chat_has_topics(id)));
        let tabs_used =
            open.is_some_and(|id| session.is_some_and(|s| s.subsection_tabs_used_for(id)));
        let tabs_left = self.subsection_tabs_column(cx);
        let open_topic = session.and_then(|s| s.open_topic);
        let topic_info = open.and_then(|id| session.and_then(|s| s.open_topic_info(id)));
        let topic_history = match (open, open_topic) {
            (Some(id), Some(topic_id)) => {
                session.and_then(|s| s.topic_histories.get(&(id.0, topic_id)))
            }
            _ => None,
        };
        let topic_empty = topic_history.is_none_or(|h| h.messages.is_empty());
        // Rebuild the history rows only when something that feeds them
        // changed; otherwise skip snapshotting the messages altogether.
        let ui_hash = self.history_rows_ui_hash();
        let today = quill::local_time::civil_local(quill::local_time::now_unix()).day_number();
        let rows_input = |list: &'static str, history: Option<&quill::state::HistoryState>| {
            let key = ui_hash.map(|ui| HistoryRowsKey {
                list,
                chat: open.map(|chat| chat.0),
                topic: open_topic,
                messages_revision: history.map_or(0, |h| h.messages.revision()),
                session_revision: session.map_or(0, |s| s.revision),
                window_epoch: history.map_or(0, |h| h.window_epoch),
                unread_anchor: history.and_then(|h| h.unread_anchor),
                highlight: highlight_id,
                ui,
                today,
            });
            let reuse = key.is_some() && key == self.history_rows_key;
            let messages = (!reuse).then(|| {
                history
                    .map(|h| h.ordered().into_iter().cloned().collect())
                    .unwrap_or_default()
            });
            (key, messages)
        };
        let (main_key, main_messages) = rows_input("session-history", main_history);
        // Topic views page their own history type; they always rebuild.
        let topic_key: Option<HistoryRowsKey> = None;
        let topic_messages = Some(
            topic_history
                .map(|h| h.ordered().into_iter().cloned().collect())
                .unwrap_or_default(),
        );
        div()
            .id("conversation-history")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(self.conversation_header(&title, chat_actions, peer_typing, cx))
            .when_some(topic_info.clone().filter(|_| !tabs_used), |this, info| {
                this.child(self.forum_topic_strip(&info, cx))
            })
            .children(self.subsection_tabs_strip(SubsectionTabsMode::Top, cx))
            .when_some(self.bot_info_panel(cx), |this, panel| this.child(panel))
            .when(self.mute_menu_open, |this| {
                this.child(self.mute_menu_panel(cx))
            })
            // Phase B4: self-destruct / auto-delete timer picker below
            // the header.
            .when(self.ttl_picker_open, |this| {
                this.child(self.ttl_picker_panel(cx))
            })
            // Parity slice: per-chat folder picker below the header.
            .when(self.folder_menu_open, |this| {
                this.child(self.folder_menu_panel(cx))
            })
            .children(open.and_then(|chat_id| self.pinned_message_banner(chat_id, cx)))
            .children(
                open.filter(|_| self.pinned_list_open)
                    .and_then(|chat_id| self.pinned_list_panel(chat_id, cx)),
            )
            .when(self.forward_picker_open, |this| {
                this.child(self.forward_picker_panel(cx))
            })
            .when_some(self.pending_bot_reply(cx), |this, reply| this.child(reply))
            .when(chat_search_open, |this| {
                this.child(self.chat_search_bar(cx))
            })
            .child(super::subsection_tabs::with_left_column(
                tabs_left,
                if let Some(reason) = gate {
                    if self.sponsored_demo {
                        // Fixture/proof surface only: the demo channel renders its
                        // sponsored rows. The live path renders history normally.
                        self.sponsored_rows_pane(cx).into_any_element()
                    } else {
                        pane_placeholder("Unsupported chat", reason, cx).into_any_element()
                    }
                } else if open.is_none() {
                    pane_placeholder(
                        "Select a chat",
                        "Choose a conversation from the sidebar to start messaging.",
                        cx,
                    )
                    .into_any_element()
                } else if !supported {
                    pane_placeholder(
                        "Unsupported chat",
                        "This conversation type is not supported yet.",
                        cx,
                    )
                    .into_any_element()
                } else if is_forum && open_topic.is_none() && !tabs_used {
                    // Phase 5.1: opening a forum supergroup shows its topics.
                    self.forum_topics_pane(open, cx).into_any_element()
                } else if has_topics && open_topic.is_some() {
                    // Phase 5.1: per-topic history — same history component,
                    // fed from the topic history store (`searchChatMessages`
                    // with `topic_id`).
                    if topic_empty {
                        pane_placeholder(
                            "No messages in this topic yet",
                            "No messages in this topic yet.",
                            cx,
                        )
                        .into_any_element()
                    } else {
                        let list = self.history_message_list(
                            "topic-history",
                            topic_messages,
                            chat.as_ref(),
                            &sender_name,
                            highlight_id,
                            media_roots,
                            cx,
                        );
                        self.history_rows_key = topic_key;
                        list
                    }
                } else if main_empty {
                    // Phase S1: an empty secret chat shows TGX's end-to-end
                    // encryption explainer (MessagesHolder TYPE_SECRET_CHAT_INFO:
                    // "Secret Chats" + EncryptedDescription1-4) instead of the
                    // generic placeholder.
                    let is_secret = chat
                        .as_ref()
                        .is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }));
                    // kit Phase 9: no history entry yet means the first
                    // getChatHistory batch is still in flight — show skeleton
                    // message rows instead of the empty placeholder.
                    let history_loading =
                        open.is_some_and(|id| session.is_some_and(|s| s.history_loading(id)));
                    if history_loading {
                        history_skeleton().into_any_element()
                    } else if is_secret {
                        self.secret_empty_explainer(cx).into_any_element()
                    } else {
                        pane_placeholder(
                            "No messages yet",
                            "History arrives via getChatHistory and updates.",
                            cx,
                        )
                        .into_any_element()
                    }
                } else {
                    let list = self.history_message_list(
                        "session-history",
                        main_messages,
                        chat.as_ref(),
                        &sender_name,
                        highlight_id,
                        media_roots,
                        cx,
                    );
                    self.history_rows_key = main_key;
                    list
                },
            ))
    }

    /// The part of the history row inputs that lives in the app rather than
    /// the session (forward selection, paused playback positions, playback
    /// settings), hashed for `HistoryRowsKey`. `None` while something plays
    /// — rows then carry per-frame state and rebuild every render.
    fn history_rows_ui_hash(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        // Demo fixtures mutate chats and users without `Session::apply`
        // (no revision bump): they always rebuild, except the stress
        // fixture used for profiling.
        if self.live.is_none() && super::demo::demo_stress_size().is_none() {
            return None;
        }
        if self.active_playback_id().is_some()
            || self.playing_animation.is_some()
            || self.playing_video.is_some()
        {
            return None;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.pending_forward
            .as_ref()
            .map(|draft| (draft.from_chat_id.0, &draft.message_ids))
            .hash(&mut hasher);
        let mut positions: Vec<(i64, u64)> = self
            .playback_positions
            .iter()
            .map(|(id, secs)| (id.0, secs.to_bits()))
            .collect();
        positions.sort_unstable();
        positions.hash(&mut hasher);
        self.playback_speed.to_bits().hash(&mut hasher);
        self.playback_volume.to_bits().hash(&mut hasher);
        self.playback_error.hash(&mut hasher);
        // A bot's streaming reply grows the last row every frame.
        self.stream_rows_hash().hash(&mut hasher);
        self.vanish_rows_hash().hash(&mut hasher);
        Some(hasher.finish())
    }

    /// kit Phase 3: the shared history row list used by both chat history
    /// and per-topic history, virtualized through the kit `MessageScroller`
    /// — only visible rows build elements. `messages` are oldest-first.
    /// Per-row inputs are snapshotted into `history_rows` each render;
    /// `render_history_row` rebuilds the visible window from them, so all
    /// existing row behavior (albums, replies, reactions, media, swipe,
    /// context menu, failed sends) is unchanged.
    pub(super) fn history_message_list(
        &mut self,
        id: &'static str,
        // `None`: nothing feeding the rows changed since the last build
        // (`HistoryRowsKey`) — keep `history_rows` and the scroller as is.
        messages: Option<Vec<HistoryMessage>>,
        chat: Option<&ChatSummary>,
        sender_name: &str,
        highlight_id: Option<MessageId>,
        media_roots: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let jump_serial = session.map_or(0, |s| s.chat_search.jump_serial);
        // Phase B4: secret chats word the timer-change service row as
        // "Self-destruct".
        let is_secret = chat.is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }));
        // kit Phase 3: the scroller resets when the open chat/topic
        // changes. Read the key up front — the `session` borrow must end
        // before the state below is assigned.
        let history_key = (
            session
                .and_then(|s| s.open_chat)
                .map(|chat| chat.0)
                .unwrap_or(-1),
            session.and_then(|s| s.open_topic),
        );
        // The main history's loaded window (topic views page their own
        // history and have none of this).
        let (unread_anchor, has_newer, window_epoch) = session
            .filter(|s| s.open_topic.is_none())
            .and_then(|s| s.open_chat.and_then(|chat| s.histories.get(&chat.0)))
            .map(|h| (h.unread_anchor, h.has_newer, h.window_epoch))
            .unwrap_or((None, false, 0));
        let unread_count = chat.map_or(0, |chat| chat.unread_count);
        if let Some(mut messages) = messages {
            // Deleted messages dissolve in place before they go.
            self.merge_vanishing(&mut messages, cx);
            let mut divider_placed = false;
            let mut first_unread = |message: &HistoryMessage| {
                let first = !divider_placed
                    && !message.is_outgoing
                    && unread_anchor.is_some_and(|anchor| message.id.0 > anchor.0);
                divider_placed |= first;
                first
            };
            let groups = quill::album::group_media_albums(
                &messages,
                |message| message.media_album_id,
                |message| message.is_outgoing,
                |message| quill::album::is_album_media(&message.content),
            );
            let mut rows: Vec<HistoryRow> = Vec::with_capacity(groups.len());
            let is_group = matches!(
                chat.map(|summary| &summary.kind),
                Some(
                    ChatKind::BasicGroup { .. }
                        | ChatKind::Supergroup {
                            is_channel: false,
                            ..
                        }
                )
            );
            // Sender of each row (first item for albums): a run is consecutive
            // rows from one sender. The name heads a run; the avatar sits
            // beside its last row, with a spacer keeping earlier rows aligned.
            let identities: Vec<(bool, Option<MessageSender>)> = groups
                .iter()
                .map(|group| match group {
                    quill::album::HistoryGroup::Album { messages, .. } => messages
                        .first()
                        .map(|m| (m.is_outgoing, m.sender))
                        .unwrap_or((false, None)),
                    quill::album::HistoryGroup::Single(message) => {
                        (message.is_outgoing, message.sender)
                    }
                })
                .collect();
            // Each group's span over `messages` (album id, item count): the
            // rows below take the messages by value instead of cloning them.
            let spans: Vec<(Option<i64>, usize)> = groups
                .iter()
                .map(|group| match group {
                    quill::album::HistoryGroup::Album { album_id, messages } => {
                        (Some(*album_id), messages.len())
                    }
                    quill::album::HistoryGroup::Single(_) => (None, 1),
                })
                .collect();
            drop(groups);
            let mut owned = messages.into_iter();
            let mut previous = None;
            let mut row_chrome = |message: &HistoryMessage, continues: bool| {
                let identity = (message.is_outgoing, message.sender);
                let service = matches!(
                    message.content,
                    MessageContent::ScreenshotTaken | MessageContent::ChatJoinFromCommunity { .. }
                );
                let show_sender = previous != Some(identity);
                previous = Some(identity);
                let accent = match message.sender {
                    Some(MessageSender::User { user_id }) => session
                        .and_then(|s| s.user(user_id))
                        .map(|u| u.accent_color_id),
                    // Chats carry no parsed name color: derive one from the id
                    // the way Telegram assigns defaults.
                    Some(MessageSender::Chat { chat_id }) => Some(chat_id.rem_euclid(7) as i32),
                    None => None,
                };
                let name = match message.sender {
                    Some(MessageSender::User { user_id }) => session
                        .and_then(|s| s.user(user_id))
                        .map(|u| u.display_name()),
                    Some(MessageSender::Chat { chat_id }) => session
                        .and_then(|s| s.chats.get(&chat_id))
                        .map(|c| c.title.clone()),
                    None => None,
                }
                .unwrap_or_else(|| {
                    if is_group {
                        "Member".into()
                    } else {
                        sender_name.to_string()
                    }
                });
                let sender =
                    (!message.is_outgoing && (service || (is_group && show_sender))).then(|| {
                        SenderLabel {
                            name: name.clone(),
                            accent: accent.filter(|_| is_group),
                        }
                    });
                let receipt = if message.is_outgoing {
                    chat.map(|summary| summary.outbox_receipt(message))
                        .unwrap_or(OutboxReceipt::Sent)
                } else {
                    OutboxReceipt::None
                };
                let photo = match message.sender {
                    Some(MessageSender::User { user_id }) => {
                        session.and_then(|s| s.user_photo_path(user_id))
                    }
                    Some(MessageSender::Chat { chat_id }) => {
                        session.and_then(|s| s.chat_photo_path(ChatId(chat_id)))
                    }
                    None => None,
                }
                .and_then(|path| sandboxed_display_path(path, &media_roots));
                let sender_avatar = (!message.is_outgoing && is_group).then(|| {
                    if continues {
                        // Spacer: same column width, no avatar.
                        (String::new(), None)
                    } else {
                        (name, photo)
                    }
                });
                (sender, receipt, sender_avatar, show_sender)
            };
            let now = quill::local_time::civil_local(quill::local_time::now_unix());
            let mut previous_day: Option<i64> = None;
            let mut day_label = |date: i32| {
                if date <= 0 {
                    return None;
                }
                let civil = quill::local_time::civil_local(i64::from(date));
                let day = civil.day_number();
                (previous_day.replace(day) != Some(day))
                    .then(|| quill::local_time::day_label(&civil, &now))
            };
            for (index, (album, len)) in spans.into_iter().enumerate() {
                let continues = identities
                    .get(index + 1)
                    .is_some_and(|next| Some(next) == identities.get(index));
                match album {
                    Some(album_id) => {
                        let album_messages: Vec<HistoryMessage> =
                            owned.by_ref().take(len).collect();
                        // Same chrome rule as single rows, from the first item.
                        let (sender, receipt, sender_avatar, _) = album_messages
                            .first()
                            .map(|message| row_chrome(message, continues))
                            .unwrap_or((None, OutboxReceipt::None, None, false));
                        let day_label = album_messages.first().and_then(|m| day_label(m.date));
                        let unread_divider = album_messages.first().is_some_and(&mut first_unread);
                        rows.push(HistoryRow::Album {
                            album_id,
                            messages: album_messages,
                            sender,
                            receipt,
                            sender_avatar,
                            day_label,
                            unread_divider,
                        })
                    }
                    None => {
                        let Some(message) = owned.next() else {
                            break;
                        };
                        let (sender, receipt, sender_avatar, run_start) =
                            row_chrome(&message, continues);
                        // Phase 4.6: audio/voice rows get a seek-bar view model.
                        let seek_bar = match &message.content {
                            MessageContent::VoiceNote(note) => {
                                Some(self.seek_bar_view(message.id, f64::from(note.duration)))
                            }
                            MessageContent::Audio(audio) => {
                                Some(self.seek_bar_view(message.id, f64::from(audio.duration)))
                            }
                            _ => None,
                        };
                        let animation_playing = self.playing_animation == Some(message.id);
                        let animation_frame = if animation_playing {
                            self.animation_frames
                                .get(self.animation_frame)
                                .cloned()
                                .or_else(|| self.animation_frames.first().cloned())
                        } else {
                            None
                        };
                        let video_playing = self.playing_video == Some(message.id);
                        let video_frame = if video_playing {
                            self.video_frames
                                .get(self.video_frame)
                                .cloned()
                                .or_else(|| self.video_frames.first().cloned())
                        } else {
                            None
                        };
                        rows.push(HistoryRow::Single(Box::new(HistoryRowInputs {
                            sender,
                            receipt,
                            sender_avatar,
                            highlighted: highlight_id == Some(message.id),
                            run_start: run_start && index > 0,
                            day_label: day_label(message.date),
                            unread_divider: first_unread(&message),
                            selected_forward: self
                                .pending_forward
                                .as_ref()
                                .is_some_and(|draft| draft.contains(message.id)),
                            quote_preview: session.and_then(|s| s.reply_quote_preview(&message)),
                            forward_from: message
                                .forward_info
                                .as_ref()
                                .and_then(|info| session.map(|s| s.forward_from_label(info))),
                            seek_bar,
                            animation_playing,
                            animation_frame,
                            video_playing,
                            video_frame,
                            is_secret,
                            vanishing: self.vanish_progress(message.id),
                            message,
                        })));
                    }
                }
            }
            // A bot's streaming reply (`updatePendingMessage`) is its next
            // incoming message, growing as it arrives (Telegram Desktop).
            if let Some(message) = self.streaming_bot_message(cx) {
                let (sender, receipt, sender_avatar, run_start) = row_chrome(&message, true);
                rows.push(HistoryRow::Single(Box::new(HistoryRowInputs {
                    sender,
                    receipt,
                    sender_avatar,
                    highlighted: false,
                    run_start,
                    day_label: None,
                    unread_divider: false,
                    selected_forward: false,
                    quote_preview: None,
                    forward_from: None,
                    seek_bar: None,
                    animation_playing: false,
                    animation_frame: None,
                    video_playing: false,
                    video_frame: None,
                    is_secret: false,
                    vanishing: None,
                    message,
                })));
            }
            // kit Phase 3: sync the scroller state with the new row list.
            let count = rows.len();
            let (first, last) = (
                rows.first().and_then(HistoryRow::first_id),
                rows.last().and_then(HistoryRow::last_id),
            );
            // The list caches each row's measured height. A row can grow in
            // place — a photo finishes downloading (placeholder → image), a
            // message is edited, reactions arrive — and would then be clipped,
            // so remeasure rows whose inputs changed. File readiness affects
            // any media row, so a change there remeasures every row.
            let media_signature = self
                .session()
                .map(|s| {
                    (
                        s.files
                            .values()
                            .filter(|file| file.usable_path().is_some())
                            .count(),
                        s.downloading.len(),
                    )
                })
                .unwrap_or_default();
            if self.history_key == Some(history_key)
                && count == self.history_rows.len()
                && count == self.history_scroller.read(cx).item_count()
            {
                if media_signature != self.history_media_signature {
                    self.history_scroller
                        .update(cx, |state, cx| state.remeasure(cx));
                } else {
                    let changed: Vec<usize> = rows
                        .iter()
                        .zip(&self.history_rows)
                        .enumerate()
                        .filter(|(_, (new, old))| !new.renders_like(old))
                        .map(|(ix, _)| ix)
                        .collect();
                    if !changed.is_empty() {
                        self.history_scroller.update(cx, |state, cx| {
                            for ix in changed {
                                let _ = state.remeasure_items(ix..ix + 1, cx);
                            }
                        });
                    }
                }
            }
            self.history_media_signature = media_signature;
            if self.history_key != Some(history_key) || self.history_window_epoch != window_epoch {
                // A new or replaced window anchors once its rows are in.
                self.history_window_epoch = window_epoch;
                self.history_anchor_pending = true;
            }
            if self.history_key != Some(history_key) {
                // New chat/topic (or first render): reset; the anchor below
                // picks the position.
                self.history_key = Some(history_key);
                // Message ids are chat-local: a stale highlight id in the new
                // chat must not suppress its search-jump scroll.
                self.last_highlight = None;
                self.scroll_date = Default::default();
                self.scroll_top_probe.set(None);
                self.history_scroller.update(cx, |state, cx| {
                    state.reset(count, cx);
                });
            } else if count != self.history_scroller.read(cx).item_count() {
                let prev_count = self.history_scroller.read(cx).item_count();
                match (first, last, self.history_ends) {
                    (Some(_), _, Some((prev_first, _)))
                        if first == Some(prev_first) && count > prev_count =>
                    {
                        // New messages appended at the tail — tail-follow keeps
                        // the view pinned when the user is at the bottom. A
                        // newer page must not be skipped that way: keep the old
                        // last row in view and read on from there.
                        let added = count.saturating_sub(prev_count);
                        let newer_page = self.history_had_newer;
                        self.history_scroller.update(cx, |state, cx| {
                            let following = state.is_following_tail();
                            state.append(added, cx);
                            if newer_page && following {
                                state.scroll_to_item(prev_count.saturating_sub(1), cx);
                            }
                        });
                    }
                    (_, Some(_), Some((_, prev_last)))
                        if last == Some(prev_last) && count > prev_count =>
                    {
                        // Older history prepended — the visible anchor stays.
                        let added = count.saturating_sub(prev_count);
                        self.history_scroller.update(cx, |state, cx| {
                            state.prepend(added, cx);
                        });
                    }
                    _ => {
                        // Shrink or reorder (delete/edit): splice preserves the
                        // scroll anchor (reset() would yank to the top and arm
                        // tail-follow); stay at the tail only if the user was
                        // following it.
                        let follow = self.history_scroller.read(cx).is_following_tail();
                        self.history_scroller.update(cx, |state, cx| {
                            state.splice(0..prev_count, count, cx);
                            if follow {
                                state.scroll_to_end(cx);
                            }
                        });
                    }
                }
            } else if let (Some(first), Some(last)) = (first, last)
                && self.history_ends != Some((first, last))
            {
                // Same row count but row identity changed (e.g. a second album
                // photo turned a Single row into a taller Album row): cached
                // measured heights are stale, so remeasure. This converges —
                // history_ends is updated below — and must not run
                // unconditionally or remeasure's notify() would loop.
                self.history_scroller.update(cx, |state, cx| {
                    state.remeasure(cx);
                });
            }
            self.history_ends = match (first, last) {
                (Some(first), Some(last)) => Some((first, last)),
                _ => None,
            };
            self.history_rows = rows;
            self.history_had_newer = has_newer;
            // A new or replaced window: start at the jump target, else at the
            // "Unread messages" divider, else at the bottom.
            if self.history_anchor_pending && !self.history_rows.is_empty() {
                self.history_anchor_pending = false;
                let highlighted = highlight_id.and_then(|target| {
                    self.history_rows
                        .iter()
                        .position(|row| row.contains(target))
                });
                if highlighted.is_some() {
                    self.last_highlight = highlight_id;
                }
                let target = highlighted.or_else(|| {
                    self.history_rows
                        .iter()
                        .position(HistoryRow::unread_divider)
                        // A little context above the divider.
                        .map(|ix| ix.saturating_sub(1))
                });
                self.history_scroller.update(cx, |state, cx| match target {
                    Some(ix) => {
                        state.scroll_to_item(ix, cx);
                    }
                    None => state.scroll_to_end(cx),
                });
            }
            // Chat-search jump: scroll the highlight into view once per new
            // `highlight_id` (current code only outlined the message).
            if highlight_id != self.last_highlight {
                self.last_highlight = highlight_id;
                if let Some(target) = highlight_id
                    && let Some(ix) = self
                        .history_rows
                        .iter()
                        .position(|row| row.contains(target))
                {
                    self.history_scroller.update(cx, |state, cx| {
                        state.scroll_to_item(ix, cx);
                    });
                }
            }
        }
        self.history_shared = HistoryShared { media_roots };
        // A new jump (search hit, reply, pinned message) restarts the
        // highlight fade, even onto the message highlighted before.
        match (highlight_id, jump_serial) {
            (Some(_), serial) if self.highlight_fade.map(|f| f.0) != Some(serial) => {
                self.highlight_fade = Some((serial, std::time::Instant::now()));
            }
            (None, _) => self.highlight_fade = None,
            _ => {}
        }
        let count = self.history_rows.len();
        let corner_buttons = self.jump_corner_buttons(
            chat.as_ref().map_or(0, |c| c.unread_mention_count),
            chat.as_ref().map_or(0, |c| c.unread_reaction_count),
            cx,
        );
        // kit Phase 3: only visible rows render. Row 0 becoming visible
        // pages older history (the driver dedupes in-flight requests and
        // reports exhaustion; the loader notifies only when a request was
        // actually sent, so this cannot notify-loop while pinned at top).
        // Inline players for rows that don't render this pass stop.
        self.inline_videos.borrow_mut().begin_render();
        let weak = cx.weak_entity();
        let date_pill = self.scroll_date_pill(cx);
        let probe = self.scroll_probe.clone();
        let top_probe = self.scroll_top_probe.clone();
        super::selectable_text::selection_viewport(
            div()
                .id(id)
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .relative()
                // kit Phase 7: screen-reader landmark for the message history.
                .role(Role::Log)
                .aria_label(format!("Message history — {sender_name}"))
                // Settings → Appearance: chat wallpaper (solid color behind
                // the message list; None keeps the theme background).
                .when_some(self.appearance.wallpaper_rgb, |this, color| {
                    this.bg(rgb(color))
                })
                .child(
                    MessageScroller::new(
                        id,
                        self.history_scroller.clone(),
                        move |ix, _window, cx| {
                            let gif_view = weak.clone();
                            cx.defer(move |cx| {
                                let _ =
                                    gif_view.update(cx, |this, cx| this.maybe_autoplay_gif(ix, cx));
                            });
                            if ix == 0 {
                                let weak = weak.clone();
                                cx.defer(move |cx| {
                                    let _ =
                                        weak.update(cx, |this, cx| this.maybe_auto_load_older(cx));
                                });
                            }
                            // Prefetch the next newer page a few rows before the
                            // window's end so reading on rarely waits.
                            if has_newer && ix + NEWER_PREFETCH_ROWS >= count {
                                let weak = weak.clone();
                                cx.defer(move |cx| {
                                    let _ =
                                        weak.update(cx, |this, cx| this.maybe_auto_load_newer(cx));
                                });
                            }
                            weak.update(cx, |this, cx| this.render_history_row(ix, cx))
                                .unwrap_or_else(|_| div().into_any_element())
                        },
                    )
                    // The kit's default row wrapper pads every non-last row with
                    // pb_8 (32px); override to pb_1 to restore the old gap_1
                    // density. The kit also supplies row px and list py, so the
                    // outer div needs neither.
                    .with_row_style(StyleRefinement::default().pb_1())
                    // Jump to latest: shows the unread count, and replaces a
                    // window that stops short of the latest message instead of
                    // only scrolling to its end.
                    .with_jump_button_renderer({
                        let jump = cx.weak_entity();
                        move |button| {
                            button
                                .when(unread_count > 0, |button| {
                                    button.label(unread_count.to_string())
                                })
                                .on_click(move |_, _, cx| {
                                    let _ = jump
                                        .update(cx, |this, cx| this.jump_to_latest_messages(cx));
                                })
                        }
                    })
                    .size_full()
                    .min_h_0(),
                )
                // Paints after the rows: the first row reaching below the
                // top edge is what the floating date describes.
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, _, _| {
                            let mut rows = probe.borrow_mut();
                            let top = rows
                                .iter()
                                .filter(|(_, row, _)| row.bottom() > bounds.top())
                                .min_by_key(|(ix, _, _)| *ix)
                                .map(|(ix, row, has_day)| {
                                    (*ix, *has_day && row.top() >= bounds.top())
                                });
                            rows.clear();
                            if top.is_some() {
                                top_probe.set(top);
                            }
                        },
                    )
                    .absolute()
                    .inset_0()
                    .size_full(),
                )
                .children(date_pill)
                // tdesktop's corner "@" / heart buttons.
                .children(corner_buttons),
        )
        .into_any_element()
    }

    /// kit Phase 3: resolve one virtualized history row to its element —
    /// the per-render snapshot in `history_rows`/`history_shared`, with the
    /// row's highlight, selected-forward, failed-send and mouse behavior
    /// unchanged from the pre-virtualization list.
    pub(super) fn render_history_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.history_rows.get(ix) else {
            return div().into_any_element();
        };
        {
            let mut rendered = self.rendered_history_rows.borrow_mut();
            if !rendered.contains(&ix) {
                rendered.push(ix);
            }
        }
        // A row scrolled into view that hasn't been reported yet: render
        // the app once more so `report_visible_history` sees it promptly
        // (list scrolling alone doesn't re-render the app view).
        let unreported = self.live.is_some()
            && self.history_window_active
            && row.message_ids().iter().any(|id| {
                !self
                    .reported_visible
                    .as_ref()
                    .is_some_and(|(_, ids)| ids.contains(id))
            });
        if unreported {
            cx.notify();
        }
        let element = self.render_history_row_body(row, cx);
        let has_day = row.day_label().is_some();
        let probe = self.scroll_probe.clone();
        // Notes where the row is painted, for the floating date pill.
        let tracker = canvas(
            |_, _, _| {},
            move |bounds, _, _, _| probe.borrow_mut().push((ix, bounds, has_day)),
        )
        .absolute()
        .inset_0()
        .size_full();
        div()
            .relative()
            .flex()
            .flex_col()
            .child(tracker)
            .when_some(row.day_label(), |this, label| {
                this.child(day_separator(label, cx))
            })
            .when(row.unread_divider(), |this| {
                let unread_at_open = self
                    .session()
                    .and_then(|s| s.open_chat.and_then(|chat| s.histories.get(&chat.0)))
                    .map_or(0, |h| h.unread_at_open);
                this.child(unread_divider(unread_at_open, cx))
            })
            .child(element)
            .into_any_element()
    }

    fn render_history_row_body(&self, row: &HistoryRow, cx: &mut Context<Self>) -> AnyElement {
        let shared = &self.history_shared;
        // File state is read from the session at row-render time rather
        // than copied into a per-render snapshot.
        let no_files = HashMap::new();
        let no_ids = std::collections::HashSet::new();
        let session = self.session();
        let files = session.map_or(&no_files, |s| &s.files);
        let downloading = session.map_or(&no_ids, |s| &s.downloading);
        let failed = session.map_or(&no_ids, |s| &s.failed_downloads);
        // Settings → Appearance: font size + bubble/plain style.
        let look = self.bubble_look(cx);
        match row {
            HistoryRow::Album {
                album_id,
                messages,
                sender,
                receipt,
                sender_avatar,
                ..
            } => {
                let refs: Vec<&HistoryMessage> = messages.iter().collect();
                album_history_row(
                    *album_id,
                    &refs,
                    files,
                    downloading,
                    &shared.media_roots,
                    sender.clone(),
                    *receipt,
                    sender_avatar.clone(),
                    // Settings → Appearance: font size + bubble/plain style.
                    look,
                    cx,
                )
            }
            HistoryRow::Single(inputs) => {
                let message = &inputs.message;
                let inline = self.inline_frame(message, cx);
                // A covered spoiler's dust shimmers (gently: 20 fps).
                let spoiler = match &message.content {
                    MessageContent::Photo(photo) => photo.has_spoiler,
                    MessageContent::Video(video) => video.has_spoiler,
                    MessageContent::Animation(animation) => animation.has_spoiler,
                    _ => false,
                };
                let key = (message.chat_id.0, message.id.0 as u64, u64::MAX, false);
                // lib_ui draws spoiler frames every 33 ms; a reveal fades
                // the cover out.
                if spoiler
                    && (!self.spoiler_revealed.contains(&key)
                        || super::spoiler_fx::reveal_fade(key).is_some())
                {
                    self.request_animation_tick(30, cx);
                }
                let row = session_history_row(
                    message,
                    files,
                    downloading,
                    failed,
                    &shared.media_roots,
                    inputs.sender.clone(),
                    inputs.receipt,
                    inputs.sender_avatar.clone(),
                    inputs.quote_preview.clone(),
                    inputs.forward_from.clone(),
                    inputs.seek_bar.clone(),
                    inputs.animation_playing,
                    inputs.animation_frame.clone(),
                    match &message.content {
                        MessageContent::Sticker(sticker) => {
                            self.sticker_image(sticker.file_id, sticker.format, cx)
                        }
                        _ => None,
                    },
                    self.message_custom_emoji_frames(message, cx),
                    inputs.video_playing,
                    inputs.video_frame.clone(),
                    inline,
                    &self.spoiler_revealed,
                    inputs.is_secret,
                    self.session(),
                    // Settings → Appearance: font size + bubble/plain style.
                    look,
                    cx,
                );
                // M1: `cx.listener` closures must be `'static`, so the
                // row's ids are copied out of the message first.
                let (row_chat, row_msg) = (message.chat_id, message.id);
                let selection_overlay =
                    self.selection_overlay(row_chat, row_msg, message.pending, cx);
                let vanishing = inputs.vanishing;
                let highlighted = inputs.highlighted;
                let selected_forward = inputs.selected_forward;
                let failed = message.failed;
                let run_start = inputs.run_start;
                let element = div()
                    .relative()
                    .child(super::vanish::row_tracker(row_msg.0))
                    .when(run_start, |this| this.pt_2())
                    // Jump target, forward selection and failed sends tint
                    // the whole row instead of outlining it: no border or
                    // padding, so the row never shifts as the state flips.
                    .rounded_md()
                    .when_some(
                        highlighted.then(|| self.jump_highlight_alpha(cx)).flatten(),
                        |this, alpha| this.bg(cx.theme().primary.opacity(alpha)),
                    )
                    .when(selected_forward, |this| this.bg(cx.theme().selection))
                    // M1: failed sends stay visibly marked so the retry
                    // affordance is noticed (`updateMessageSendFailed`).
                    .when(failed, |this| this.bg(cx.theme().danger.opacity(0.08)))
                    // M1: right-click opens the message context menu at
                    // the click position (window coordinates).
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.open_message_menu(
                                MessageMenuState {
                                    chat_id: row_chat,
                                    message_id: row_msg,
                                    position: event.position,
                                },
                                window,
                                cx,
                            );
                        }),
                    )
                    // M1: swipe-to-reply — press on the row, release >24px
                    // to the left of the press point starts a reply.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.swipe_reply_start = Some((row_chat, row_msg, event.position.x));
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                            if let Some((chat_id, message_id, start_x)) =
                                this.swipe_reply_start.take()
                                && chat_id == row_chat
                                && message_id == row_msg
                                && start_x - event.position.x > px(24.)
                            {
                                this.begin_reply_from_message(chat_id, message_id, window, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .child(row)
                    // M1: explicit failed-send notice with a retry hint.
                    .when(failed, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(danger())
                                .child("⚠ Failed to send — right-click → Retry send"),
                        )
                    })
                    .children(selection_overlay)
                    .into_any_element();
                match vanishing {
                    Some(t) => self.ghost_row(row_msg, t, element),
                    None => element,
                }
            }
        }
    }

    /// kit Phase 3: automatic older-history paging — called (deferred) when
    /// the top row of the virtualized history becomes visible. Notifies the
    /// frame only when a request was actually sent (the driver dedupes
    /// in-flight `getChatHistory` requests and stops at `loaded_complete`).
    /// The window stops short of the latest message and its end is near:
    /// load the next newer page (the driver dedupes in-flight requests).
    pub(super) fn maybe_auto_load_newer(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.session.open_topic.is_none()
            && live.driver.fetch_history_newer().ok().flatten().is_some()
        {
            cx.notify();
        }
    }

    /// The jump-to-latest button: a window that stops short of the latest
    /// message is replaced by the newest page (the scroller re-anchors at
    /// the bottom when it lands); otherwise just scroll down.
    pub(super) fn jump_to_latest_messages(&mut self, cx: &mut Context<Self>) {
        let replaced = self.live.as_mut().is_some_and(|live| {
            live.driver
                .session
                .open_chat
                .and_then(|chat| live.driver.session.histories.get(&chat.0))
                .is_some_and(|h| h.has_newer)
                && live.driver.jump_to_latest().is_ok()
        });
        if !replaced {
            if let Some(live) = self.live.as_mut()
                && let Some(chat) = live.driver.session.open_chat
                && let Some(history) = live.driver.session.histories.get_mut(&chat.0)
            {
                history.unread_anchor = None;
            }
            self.history_scroller
                .update(cx, |state, cx| state.scroll_to_end(cx));
        }
        cx.notify();
    }

    pub(super) fn maybe_auto_load_older(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            // Phase 5.1: a topic view pages its own history.
            let sent = if live.driver.session.open_topic.is_some() {
                live.driver.fetch_topic_history()
            } else {
                live.driver.fetch_history()
            }
            .ok()
            .flatten()
            .is_some();
            if sent {
                cx.notify();
            }
        }
    }
}

/// Full-width "N Unread Messages" bar above the first unread message.
fn unread_divider(count: i32, cx: &App) -> impl IntoElement {
    let text = SharedString::from(quill::state::unread_bar_text(count));
    div()
        .id("unread-divider")
        .w_full()
        .my_2()
        .py_1()
        .flex()
        .justify_center()
        .bg(cx.theme().secondary.opacity(0.7))
        .text_xs()
        .font_medium()
        .text_color(cx.theme().secondary_foreground)
        .role(Role::Heading)
        .aria_label(text.clone())
        .child(text)
}

/// Centered local-day pill between history rows ("Today", "12 March").
fn day_separator(label: &str, cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .justify_center()
        .pt_3()
        .pb_1()
        .child(pill_label(label, cx))
}

/// The day pill itself, shared by the inline separators and the floating
/// date shown while scrolling.
pub(super) fn pill_label(label: &str, cx: &App) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("day-{label}")))
        .px_3()
        .py_0p5()
        .rounded_full()
        .bg(cx.theme().secondary)
        .text_xs()
        .font_medium()
        .text_color(cx.theme().secondary_foreground)
        .role(Role::Heading)
        .aria_label(SharedString::from(label.to_string()))
        .child(label.to_string())
}

impl QuillApp {
    /// Report the history rows the list rendered last frame as seen
    /// (`viewMessages` through the driver), so only messages actually on
    /// screen are marked read. Skipped while the window is inactive —
    /// nothing is "seen" behind another app — and when the set is
    /// unchanged; the driver also drops ids already viewed or in flight.
    pub(super) fn report_visible_history(&mut self, window_active: bool, cx: &mut Context<Self>) {
        self.history_window_active = window_active;
        let rendered = std::mem::take(&mut *self.rendered_history_rows.borrow_mut());
        if !window_active || rendered.is_empty() {
            return;
        }
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return;
        };
        let mut ids: Vec<MessageId> = rendered
            .into_iter()
            .filter_map(|ix| self.history_rows.get(ix))
            .flat_map(HistoryRow::message_ids)
            .collect();
        ids.sort_unstable_by_key(|id| id.0);
        ids.dedup();
        if self
            .reported_visible
            .as_ref()
            .is_some_and(|(chat, last)| *chat == chat_id && *last == ids)
        {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            // Recorded even when the send fails: the driver keeps the ids
            // queued for the next attempt, and re-reporting every frame
            // would only spin the render loop.
            let _ = live.driver.view_messages(chat_id, &ids);
            self.reported_visible = Some((chat_id, ids));
            cx.notify();
        }
    }
}
