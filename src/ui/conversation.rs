//! conversation header, typing indicators.

use super::app::{PaneMode, QuillApp, pane_placeholder};
use super::chat_row::description_snippet;
use super::chat_row::{chat_avatar, compact_count};
use super::group_panels::SupergroupHeaderExtras;
use super::history::HistoryShared;
use super::history::{album_history_row, history_skeleton, session_history_row};
use super::pressable::PressableDiv;
use super::recording::RecordMode;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::message_scroller::MessageScroller;
use gpui_kit::component::*;
use gpui_kit::gpui::StyleRefinement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::AttachmentKind;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::poll::chat_allows_polls;
use quill::state::{
    ChatSearchJump, ChatSummary, HistoryMessage, InfoPanelTarget, OutboxReceipt, Session,
};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ChatKind, MessageContent, ParsedFile, format_ttl_setting};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
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
        let (chat_id, muted, forever, archived) =
            actions.unwrap_or((ChatId(0), false, false, false));
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
            let (supergroup_id, is_channel) = match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel,
                } => (supergroup_id, is_channel),
                _ => return None,
            };
            let full = session.supergroup_full_infos.get(&supergroup_id).cloned();
            let roots = self.media_display_roots();
            let photo = session
                .chat_photo_path(chat_id)
                .and_then(|path| sandboxed_display_path(path, &roots));
            let username = session
                .supergroup_username(supergroup_id)
                .filter(|name| !name.is_empty())
                .map(|name| name.to_string());
            Some(SupergroupHeaderExtras {
                is_channel,
                photo,
                username,
                member_count: full.as_ref().map(|info| info.member_count),
                description_snippet: full.as_ref().and_then(|info| {
                    let snippet = description_snippet(&info.description, 120);
                    (!snippet.is_empty()).then_some(snippet)
                }),
                discussion_chat_id: session.discussion_chat_id(chat_id),
            })
        });
        let discuss_chat_id = extras.as_ref().and_then(|ex| ex.discussion_chat_id);
        let title_text = title.to_string();
        let muted_fg = cx.theme().muted_foreground;
        // Phase B4: chat-level auto-delete / self-destruct timer status
        // (`chat.message_auto_delete_time`, schema 1.8.67 lines 3616 /
        // 3627) — shown under the title when a timer is set.
        let ttl_line: Option<String> = actions.and_then(|(chat_id, _, _, _)| {
            self.session()
                .and_then(|s| s.chats.get(&chat_id.0))
                .and_then(|chat| chat.ttl_status_line())
        });
        // Slice S17: peer activity label — "choosing a sticker…" wins over
        // "typing…" while a peer picks a sticker (`chatActionChoosingSticker`,
        // schema 1.8.67 line 6380).
        let activity_label: Option<&'static str> = actions.and_then(|(chat_id, _, _, _)| {
            self.session()
                .and_then(|s| s.chats.get(&chat_id.0))
                .and_then(|chat| chat.peer_activity_label())
        });
        // S17: widen the gate — sticker-picking sets no typing senders, so
        // `typing` alone would hide the "choosing a sticker…" label.
        let typing = typing || activity_label.is_some();
        // Status lines kept for every chat kind (typing / muted).
        let identity: AnyElement = match (info_target, extras) {
            (Some(InfoPanelTarget::Supergroup(supergroup_id)), Some(ex)) => {
                let mut meta: Vec<String> = Vec::new();
                if let Some(username) = &ex.username {
                    meta.push(format!("@{username}"));
                }
                if let Some(count) = ex.member_count.filter(|count| *count > 0) {
                    let noun = if ex.is_channel {
                        "subscribers"
                    } else {
                        "members"
                    };
                    meta.push(format!("{} {noun}", compact_count(count)));
                }
                let meta_line = meta.join(" · ");
                div()
                    .id("conversation-identity")
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_supergroup_panel(supergroup_id, window, cx);
                    }))
                    .child(chat_avatar(&title_text, ex.photo.as_deref(), 40.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().font_semibold().child(title_text))
                            .when(!meta_line.is_empty(), |this| {
                                this.child(
                                    div()
                                        .id("conversation-meta")
                                        .text_xs()
                                        .text_color(muted_fg)
                                        .child(meta_line),
                                )
                            })
                            .when_some(ex.description_snippet, |this, snippet| {
                                this.child(
                                    div()
                                        .id("conversation-description")
                                        .text_xs()
                                        .text_color(muted_fg)
                                        .child(snippet),
                                )
                            })
                            .when(typing, |this| {
                                this.child(
                                    div()
                                        .id("peer-typing")
                                        .text_sm()
                                        .text_color(cx.theme().accent)
                                        .child(activity_label.unwrap_or("typing…")),
                                )
                            })
                            .when(muted && !typing, |this| {
                                this.child(div().text_xs().text_color(muted_fg).child(if forever {
                                    "Muted forever"
                                } else {
                                    "Muted"
                                }))
                            })
                            // Phase B4: chat-level auto-delete / self-destruct timer
                            // status line (hidden when no timer is set).
                            .when_some(ttl_line.clone(), |this, line| {
                                this.child(
                                    div()
                                        .id("conversation-ttl")
                                        .text_xs()
                                        .text_color(muted_fg)
                                        .child(line),
                                )
                            }),
                    )
                    .into_any_element()
            }
            (Some(InfoPanelTarget::User(user_id)), _) => div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .id("conversation-title")
                        .font_semibold()
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_user_panel(user_id, window, cx);
                        }))
                        .child(title_text),
                )
                // Phase S1: pending secret chats show the TGX
                // `AwaitingEncryption` subtitle under the title.
                .when_some(self.secret_pending_subtitle(chat_id), |this, line| {
                    this.child(
                        div()
                            .id("secret-pending-subtitle")
                            .text_xs()
                            .text_color(cx.theme().accent)
                            .child(line),
                    )
                })
                .when(typing, |this| {
                    this.child(
                        div()
                            .id("peer-typing")
                            .text_sm()
                            .text_color(cx.theme().accent)
                            .child(activity_label.unwrap_or("typing…")),
                    )
                })
                .when(muted && !typing, |this| {
                    this.child(div().text_xs().text_color(muted_fg).child(if forever {
                        "Muted forever"
                    } else {
                        "Muted"
                    }))
                })
                // Phase B4: chat-level auto-delete / self-destruct timer
                // status line (hidden when no timer is set).
                .when_some(ttl_line.clone(), |this, line| {
                    this.child(
                        div()
                            .id("conversation-ttl")
                            .text_xs()
                            .text_color(muted_fg)
                            .child(line),
                    )
                })
                .into_any_element(),
            _ => div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(div().font_semibold().child(title_text))
                .when(typing, |this| {
                    this.child(
                        div()
                            .id("peer-typing")
                            .text_sm()
                            .text_color(cx.theme().accent)
                            .child(activity_label.unwrap_or("typing…")),
                    )
                })
                .when(muted && !typing, |this| {
                    this.child(div().text_xs().text_color(muted_fg).child(if forever {
                        "Muted forever"
                    } else {
                        "Muted"
                    }))
                })
                // Phase B4: chat-level auto-delete / self-destruct timer
                // status line (hidden when no timer is set).
                .when_some(ttl_line.clone(), |this, line| {
                    this.child(
                        div()
                            .id("conversation-ttl")
                            .text_xs()
                            .text_color(muted_fg)
                            .child(line),
                    )
                })
                .into_any_element(),
        };
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
                // Phase B1: secret chats get a "Close secret chat" action
                // (`closeSecretChat`, schema 1.8.67 line 15242) instead of
                // the folder picker — closing is permanent.
                let is_secret = self
                    .session()
                    .and_then(|s| s.chats.get(&chat_id.0))
                    .is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }));
                // Phase B4: the self-destruct timer picker is only usable
                // in Ready secret chats — Pending/Closed chats can't send
                // (`can_post` is false there), and the driver would
                // reject the request.
                let ttl_ready = self
                    .session()
                    .and_then(|s| s.chats.get(&chat_id.0))
                    .is_some_and(|c| matches!(c.kind, ChatKind::Secret { .. }) && c.can_post());
                let ttl_button_label = self
                    .session()
                    .and_then(|s| s.chats.get(&chat_id.0))
                    .map(|c| format!("⏱ {}", format_ttl_setting(c.message_auto_delete_time)))
                    .unwrap_or_else(|| "⏱ Off".to_string());
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
                        .gap_1()
                        .when(voice_ok, |this| {
                            this.child(
                                Button::new("chat-voice-chat")
                                    .label(if voice_live {
                                        "🔊 Voice chat"
                                    } else {
                                        "Start voice chat"
                                    })
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.start_or_join_video_chat(chat_id, window, cx);
                                    })),
                            )
                        })
                        // Phase B1: close a secret chat (confirm banner
                        // below, like delete confirm).
                        .when(is_secret, |this| {
                            this.child(
                                Button::new("chat-close-secret")
                                    .label("Close secret chat")
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_close_secret_chat_confirm(chat_id, cx);
                                    })),
                            )
                        })
                        // Phase B4: self-destruct timer picker for Ready
                        // secret chats (`setChatMessageAutoDeleteTime`,
                        // schema 1.8.67 line 13454).
                        .when(ttl_ready, |this| {
                            this.child(
                                Button::new("chat-ttl")
                                    .label(ttl_button_label.clone())
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.ttl_picker_open = !this.ttl_picker_open;
                                        cx.notify();
                                    })),
                            )
                        })
                        // Parity slice: jump to the linked discussion group
                        // (`linked_chat_id`) when the channel has one.
                        .when_some(discuss_chat_id, |this, discussion_id| {
                            this.child(
                                Button::new("chat-discuss")
                                    .label("Discuss")
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_listed_chat(ChatId(discussion_id), window, cx);
                                    })),
                            )
                        })
                        .child(
                            Button::new("chat-mute")
                                .label(if muted { "Unmute" } else { "Mute" })
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if muted {
                                        this.apply_chat_mute(chat_id, 0, cx);
                                    } else if this.mute_menu_open {
                                        this.close_mute_menu(cx);
                                    } else {
                                        this.open_mute_menu(cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("chat-archive")
                                .label(if archived { "Unarchive" } else { "Archive" })
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_archive(chat_id, cx);
                                })),
                        )
                        // Parity slice: per-chat folder picker — destinations
                        // come from `getChatListsToAddChat`, as the schema
                        // intends; removals go through `editChatFolder`
                        // (there is no `removeChatFromList` in 1.8.67).
                        .child(
                            Button::new("chat-folders")
                                .label("Folders")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.folder_menu_open {
                                        this.folder_menu_open = false;
                                        cx.notify();
                                    } else {
                                        this.open_folder_menu(chat_id, cx);
                                    }
                                })),
                        )
                        // Slice media-shared-gallery: the per-chat shared
                        // media gallery (Media / Files / Music / Links /
                        // Voice / GIFs tabs). Live only — demo sessions have
                        // no TDLib to fetch pages from (the screenshot
                        // fixture seeds `Session::shared_media` directly).
                        .when(self.live.is_some(), |this| {
                            this.child(
                                Button::new("chat-shared-media")
                                    .label("Media")
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_shared_media_ui(cx);
                                    })),
                            )
                        }),
                )
            })
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
                "Chat list and sending unlock after authorization is Ready. This window is not showing a fake inbox.",
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
                    Some("Select a supported cloud chat to send.".to_string())
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
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(history)
            // Phase C2i: busy-decline banner — the calls that arrived
            // while another call was active were declined with
            // `discardCall` (TDLib has no hold/swap API). Dismissible.
            .when_some(self.call_busy_banner(cx), |this, banner| this.child(banner))
            .when(composer.is_some(), |this| {
                let show_attach = matches!(mode, PaneMode::Ready) && self.pending_edit.is_none();
                let chips: Vec<String> = if show_attach {
                    self.pending_attachments
                        .iter()
                        .map(|att| match att.kind {
                            AttachmentKind::Photo => format!("Photo · {}", att.file_name),
                            AttachmentKind::Document => format!("Document · {}", att.file_name),
                            AttachmentKind::Video => format!("Video · {}", att.file_name),
                            AttachmentKind::VideoNote => format!("Video note · {}", att.file_name),
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                this.child(
                    div()
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .when(self.recording_active(), |this| {
                            this.child(self.record_bar(cx))
                        })
                        // kit Phase 5: the attach menu — the attach options
                        // live behind the paperclip icon button in the
                        // input row instead of a permanent button row.
                        .when(show_attach && self.attach_menu_open, |box_| {
                            box_.child(
                                div()
                                    .id("composer-attach-menu")
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Button::new("attach-photo").label("Attach photo").on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.attach_local(AttachmentKind::Photo, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new("attach-file").label("Attach file").on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.attach_local(AttachmentKind::Document, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new("attach-video").label("Attach video").on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.attach_local(AttachmentKind::Video, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new("attach-video-note")
                                            .label("Video note")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.attach_local(AttachmentKind::VideoNote, cx);
                                            })),
                                    )
                                    .child(if polls_allowed {
                                        Button::new("open-poll-dialog")
                                            .label("Poll")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_poll_dialog(window, cx);
                                            }))
                                            .into_any_element()
                                    } else {
                                        // B4: clear notice when the server
                                        // disallows polls
                                        // (`chatPermissions.can_send_polls`,
                                        // schema 1.8.67 line 1070).
                                        div()
                                            .text_xs()
                                            .text_color(warning_orange())
                                            .child("Polls restricted in this chat")
                                            .into_any_element()
                                    })
                                    .child(
                                        Button::new("open-gifs")
                                            .label(if self.gif_panel_open() {
                                                "GIFs open"
                                            } else {
                                                "GIFs"
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_gif_panel(cx);
                                            })),
                                    )
                                    .when(!self.pending_attachments.is_empty(), |row| {
                                        row.child(
                                            Button::new("clear-attach").label("Clear").on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.clear_attachment(cx);
                                                }),
                                            ),
                                        )
                                    })
                                    // MED1: album grouping toggle + "remember
                                    // grouping" (TGX `RememberAlbumSetting`).
                                    .when(self.pending_attachments.len() >= 2, |row| {
                                        let grouped = self.composer_group_media_effective();
                                        let remember = self
                                            .session()
                                            .map(|s| s.media_prefs.remember_media_grouping)
                                            .unwrap_or(false);
                                        row.child(
                                            Button::new("composer-group-media")
                                                .label(if grouped {
                                                    "Grouped ✓"
                                                } else {
                                                    "Ungrouped"
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_composer_group_media(cx);
                                                })),
                                        )
                                        .child(
                                            Button::new("composer-remember-grouping")
                                                .label(if remember {
                                                    "Remember: on"
                                                } else {
                                                    "Remember: off"
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_remember_media_grouping(cx);
                                                })),
                                        )
                                    })
                                    // Phase B3: self-destruct timer picker —
                                    // only for photo/video in private chats
                                    // (the only combination TDLib accepts
                                    // `self_destruct_type` for).
                                    .when(self.self_destruct_picker_visible(), |row| {
                                        let label = self.self_destruct_button_label();
                                        row.child(
                                            Button::new("self-destruct-cycle")
                                                .label(label)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.cycle_composer_self_destruct(cx);
                                                })),
                                        )
                                    }),
                            )
                        })
                        .when(!chips.is_empty(), |this| {
                            let album = chips.len() >= 2;
                            let mut row =
                                div().id("composer-attach-chip").flex().flex_wrap().gap_2();
                            for (index, label) in chips.into_iter().enumerate() {
                                row = row.child(
                                    div()
                                        .id(("composer-attach-item", index as u64))
                                        .px_3()
                                        .py_2()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(text_muted())
                                        .bg(bg_subtle())
                                        .child(div().text_sm().font_medium().child(label))
                                        .child(div().text_xs().text_color(text_primary()).child(
                                            if album {
                                                "album · picked locally"
                                            } else {
                                                "ready to send · picked locally"
                                            },
                                        )),
                                );
                            }
                            this.child(row)
                        })
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
                        .when_some(self.command_menu_dropdown(cx), |this, panel| {
                            this.child(panel)
                        })
                        // Bots slice: `@bot` inline-results dropdown above
                        // the composer (mutually exclusive with the `/`
                        // menu — see `sync_command_menu`).
                        .when_some(self.inline_results_dropdown(cx), |this, panel| {
                            this.child(panel)
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
                        // M1: formatting toolbar + send options row, above the
                        // input; the schedule picker opens above the toolbar.
                        .when(self.schedule_popup_open, |this| {
                            this.child(self.schedule_popup(cx))
                        })
                        .child(self.format_toolbar(cx))
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
                                    row.child(
                                        Button::new("composer-attach")
                                            .icon(IconName::Paperclip)
                                            .ghost()
                                            .tooltip(if self.attach_menu_open {
                                                "Close attach menu"
                                            } else {
                                                "Attach"
                                            })
                                            .accessibility_label("Attach")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.attach_menu_open = !this.attach_menu_open;
                                                cx.notify();
                                            })),
                                    )
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
                                                this.toggle_sticker_panel(cx);
                                            })),
                                    )
                                })
                                .child(
                                    div().flex_1().min_w_0().child(
                                        Textarea::new(&self.composer)
                                            .appearance(false)
                                            .bordered(false)
                                            .aria_label("Message"),
                                    ),
                                )
                                .when(show_attach, |row| {
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
                                .child(
                                    // Right-click opens the schedule picker
                                    // (desktop parity with the ⏰ toolbar
                                    // button); the picker itself is
                                    // unchanged.
                                    div()
                                        .id("composer-send-wrap")
                                        .on_mouse_down(
                                            MouseButton::Right,
                                            cx.listener(|this, _, _, cx| {
                                                this.schedule_popup_open =
                                                    !this.schedule_popup_open;
                                                cx.notify();
                                            }),
                                        )
                                        .child(
                                            Button::new("composer-send")
                                                .icon(IconName::Send)
                                                .primary()
                                                .rounded_full()
                                                .tooltip("Send · right-click for schedule options")
                                                .accessibility_label("Send message")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    let text =
                                                        this.composer.read(cx).value().to_string();
                                                    // Same guard as
                                                    // Enter-to-send.
                                                    if !text.trim().is_empty() {
                                                        this.submit_composer(text, window, cx);
                                                    }
                                                })),
                                        ),
                                ),
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
        let messages: Vec<HistoryMessage> = open
            .and_then(|id| session.and_then(|s| s.histories.get(&id.0)))
            .map(|h| h.ordered().into_iter().cloned())
            .into_iter()
            .flatten()
            .collect();
        // kit Phase 3: owned — `history_message_list` takes `&mut self`,
        // so the chat summary can't borrow the session here.
        let chat: Option<ChatSummary> =
            open.and_then(|id| session.and_then(|s| s.chats.get(&id.0).cloned()));
        let files: HashMap<i32, ParsedFile> = session.map(|s| s.files.clone()).unwrap_or_default();
        let downloading: std::collections::HashSet<i32> =
            session.map(|s| s.downloading.clone()).unwrap_or_default();
        let failed: std::collections::HashSet<i32> = session
            .map(|s| s.failed_downloads.clone())
            .unwrap_or_default();
        let media_roots = self.media_display_roots();
        let sender_name = title.clone();
        let chat_search_open = session.is_some_and(|s| s.chat_search.open);
        let highlight_id = session.and_then(|s| match s.chat_search.jump {
            ChatSearchJump::Ready { message_id } | ChatSearchJump::Loading { message_id } => {
                Some(message_id)
            }
            ChatSearchJump::None | ChatSearchJump::Missing { .. } => None,
        });
        let pinned = session.and_then(|s| s.open_chat_pinned_message()).cloned();
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
        let open_topic = session.and_then(|s| s.open_topic);
        let topic_info = open.and_then(|id| session.and_then(|s| s.open_topic_info(id)));
        let topic_messages: Vec<HistoryMessage> = match (open, open_topic) {
            (Some(id), Some(topic_id)) => session
                .and_then(|s| s.topic_histories.get(&(id.0, topic_id)))
                .map(|h| h.ordered().into_iter().cloned().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        div()
            .id("conversation-history")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(self.conversation_header(&title, chat_actions, peer_typing, cx))
            .when_some(topic_info.clone(), |this, info| {
                this.child(self.forum_topic_strip(&info, cx))
            })
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
            .when_some(pinned, |this, message| {
                this.child(self.pinned_message_banner(&message, cx))
            })
            .when(self.forward_picker_open, |this| {
                this.child(self.forward_picker_panel(cx))
            })
            .when(self.pending_react.is_some(), |this| {
                this.child(self.reaction_picker_panel(cx))
            })
            .when(self.gif_panel_open(), |this| {
                this.child(self.gif_picker_panel(cx))
            })
            .when(self.sticker_panel_open(), |this| {
                this.child(self.sticker_picker_panel(cx))
            })
            .when(chat_search_open, |this| {
                this.child(self.chat_search_bar(cx))
            })
            .child(if let Some(reason) = gate {
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
                    "The main list is driven by loadChats + updateNewChat / updateChatPosition.",
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
            } else if is_forum && open_topic.is_none() {
                // Phase 5.1: opening a forum supergroup shows its topics.
                self.forum_topics_pane(open, cx).into_any_element()
            } else if is_forum {
                // Phase 5.1: per-topic history — same history component,
                // fed from the topic history store (`searchChatMessages`
                // with `topic_id`).
                if topic_messages.is_empty() {
                    pane_placeholder(
                        "No messages in this topic yet",
                        "Topic history arrives via searchChatMessages with topic_id.",
                        cx,
                    )
                    .into_any_element()
                } else {
                    self.history_message_list(
                        "topic-history",
                        &topic_messages,
                        chat.as_ref(),
                        &sender_name,
                        highlight_id,
                        files,
                        downloading,
                        failed,
                        media_roots,
                        cx,
                    )
                }
            } else if messages.is_empty() {
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
                let history_loading = open
                    .is_some_and(|id| session.is_some_and(|s| !s.histories.contains_key(&id.0)));
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
                self.history_message_list(
                    "session-history",
                    &messages,
                    chat.as_ref(),
                    &sender_name,
                    highlight_id,
                    files,
                    downloading,
                    failed,
                    media_roots,
                    cx,
                )
            })
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
        messages: &[HistoryMessage],
        chat: Option<&ChatSummary>,
        sender_name: &str,
        highlight_id: Option<MessageId>,
        files: HashMap<i32, ParsedFile>,
        downloading: std::collections::HashSet<i32>,
        failed: std::collections::HashSet<i32>,
        media_roots: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
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
        let groups = quill::album::group_media_albums(
            messages,
            |message| message.media_album_id,
            |message| message.is_outgoing,
            |message| quill::album::is_album_media(&message.content),
        );
        let mut rows: Vec<HistoryRow> = Vec::with_capacity(groups.len());
        // kit Phase 4: per-row sender chrome. Outgoing rows never had a
        // sender header — their delivery state moves into the in-bubble
        // footer. The sender name collapses for consecutive same-direction
        // rows. The chat title is the only sender identity plumbed: it
        // identifies the sender in 1:1/secret chats and channels, so the
        // avatar slot is filled there and left empty in groups (never
        // invented).
        let single_sender = matches!(
            chat.map(|summary| &summary.kind),
            Some(ChatKind::Private { .. } | ChatKind::Secret { .. })
                | Some(ChatKind::Supergroup {
                    is_channel: true,
                    ..
                })
        );
        let avatar_photo: Option<PathBuf> = chat
            .and_then(|summary| session.and_then(|live| live.chat_photo_path(summary.id)))
            .map(PathBuf::from);
        let mut prev_outgoing: Option<bool> = None;
        let mut row_chrome = |message: &HistoryMessage| {
            // Service rows (e.g. screenshot notices) must never collapse the
            // sender: the row chrome renders "{sender} took a screenshot",
            // and collapsing to None would show "Someone" instead.
            // Slice G9: the join-from-community row attributes the join
            // to the sender ("{name} joined the group from the
            // community"), so it keeps the sender too.
            let show_sender = prev_outgoing != Some(message.is_outgoing)
                || matches!(message.content, MessageContent::ScreenshotTaken)
                || matches!(
                    message.content,
                    MessageContent::ChatJoinFromCommunity { .. }
                );
            prev_outgoing = Some(message.is_outgoing);
            let sender = if !message.is_outgoing && show_sender {
                Some(sender_name.to_string())
            } else {
                None
            };
            let receipt = if message.is_outgoing {
                chat.map(|summary| summary.outbox_receipt(message))
                    .unwrap_or(OutboxReceipt::Sent)
            } else {
                OutboxReceipt::None
            };
            let sender_avatar = if !message.is_outgoing && single_sender {
                Some((sender_name.to_string(), avatar_photo.clone()))
            } else {
                None
            };
            (sender, receipt, sender_avatar)
        };
        for group in groups {
            match group {
                quill::album::HistoryGroup::Album { album_id, messages } => {
                    let album_messages: Vec<HistoryMessage> =
                        messages.iter().map(|message| (*message).clone()).collect();
                    // Same chrome rule as single rows, from the first item.
                    let (sender, receipt, sender_avatar) = album_messages
                        .first()
                        .map(&mut row_chrome)
                        .unwrap_or((None, OutboxReceipt::None, None));
                    rows.push(HistoryRow::Album {
                        album_id,
                        messages: album_messages,
                        sender,
                        receipt,
                        sender_avatar,
                    })
                }
                quill::album::HistoryGroup::Single(message) => {
                    let (sender, receipt, sender_avatar) = row_chrome(message);
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
                        message: message.clone(),
                        sender,
                        receipt,
                        sender_avatar,
                        highlighted: highlight_id == Some(message.id),
                        selected_forward: self
                            .pending_forward
                            .as_ref()
                            .is_some_and(|draft| draft.contains(message.id)),
                        quote_preview: session.and_then(|s| s.reply_quote_preview(message)),
                        forward_from: message
                            .forward_info
                            .as_ref()
                            .and_then(|info| session.map(|s| s.forward_from_label(info))),
                        reaction_open: self
                            .pending_react
                            .is_some_and(|(chat, id)| chat == message.chat_id && id == message.id),
                        seek_bar,
                        animation_playing,
                        animation_frame,
                        video_playing,
                        video_frame,
                        is_secret,
                    })));
                }
            }
        }
        // kit Phase 3: stash the per-render shared inputs, then sync the
        // scroller state with the new row list.
        self.history_shared = HistoryShared {
            files,
            downloading,
            failed,
            media_roots,
        };
        let count = rows.len();
        let (first, last) = (
            rows.first().and_then(HistoryRow::first_id),
            rows.last().and_then(HistoryRow::last_id),
        );
        if self.history_key != Some(history_key) {
            // New chat/topic (or first render): reset and show the tail.
            self.history_key = Some(history_key);
            // Message ids are chat-local: a stale highlight id in the new
            // chat must not suppress its search-jump scroll.
            self.last_highlight = None;
            self.history_scroller.update(cx, |state, cx| {
                state.reset(count, cx);
                state.scroll_to_end(cx);
            });
        } else if count != self.history_scroller.read(cx).item_count() {
            let prev_count = self.history_scroller.read(cx).item_count();
            match (first, last, self.history_ends) {
                (Some(_), _, Some((prev_first, _)))
                    if first == Some(prev_first) && count > prev_count =>
                {
                    // New messages appended at the tail — tail-follow keeps
                    // the view pinned when the user is at the bottom.
                    let added = count.saturating_sub(prev_count);
                    self.history_scroller.update(cx, |state, cx| {
                        state.append(added, cx);
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
        // kit Phase 3: only visible rows render. Row 0 becoming visible
        // pages older history (the driver dedupes in-flight requests and
        // reports exhaustion; the loader notifies only when a request was
        // actually sent, so this cannot notify-loop while pinned at top).
        let weak = cx.weak_entity();
        div()
            .id(id)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            // kit Phase 7: screen-reader landmark for the message history.
            .role(Role::Log)
            .aria_label(format!("Message history — {sender_name}"))
            // Settings → Appearance: chat wallpaper (solid color behind
            // the message list; None keeps the theme background).
            .when_some(self.appearance.wallpaper_rgb, |this, color| {
                this.bg(rgb(color))
            })
            .child(
                MessageScroller::new(id, self.history_scroller.clone(), move |ix, _window, cx| {
                    if ix == 0 {
                        let weak = weak.clone();
                        cx.defer(move |cx| {
                            let _ = weak.update(cx, |this, cx| this.maybe_auto_load_older(cx));
                        });
                    }
                    weak.update(cx, |this, cx| this.render_history_row(ix, cx))
                        .unwrap_or_else(|_| div().into_any_element())
                })
                // The kit's default row wrapper pads every non-last row with
                // pb_8 (32px); override to pb_1 to restore the old gap_1
                // density. The kit also supplies row px and list py, so the
                // outer div needs neither.
                .with_row_style(StyleRefinement::default().pb_1())
                .size_full()
                .min_h_0(),
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
        let shared = &self.history_shared;
        // Settings → Appearance: font size + bubble/plain style.
        let look = self.bubble_look(cx);
        match row {
            HistoryRow::Album {
                album_id,
                messages,
                sender,
                receipt,
                sender_avatar,
            } => {
                let refs: Vec<&HistoryMessage> = messages.iter().collect();
                album_history_row(
                    *album_id,
                    &refs,
                    &shared.files,
                    &shared.downloading,
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
                let row = session_history_row(
                    message,
                    &shared.files,
                    &shared.downloading,
                    &shared.failed,
                    &shared.media_roots,
                    inputs.sender.clone(),
                    inputs.receipt,
                    inputs.sender_avatar.clone(),
                    inputs.quote_preview.clone(),
                    inputs.forward_from.clone(),
                    inputs.selected_forward,
                    inputs.reaction_open,
                    inputs.seek_bar.clone(),
                    inputs.animation_playing,
                    inputs.animation_frame.clone(),
                    inputs.video_playing,
                    inputs.video_frame.clone(),
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
                let highlighted = inputs.highlighted;
                let selected_forward = inputs.selected_forward;
                let failed = message.failed;
                div()
                    .when(highlighted || selected_forward, |this| {
                        this.rounded_lg()
                            .border_2()
                            .border_color(if selected_forward {
                                success()
                            } else {
                                accent()
                            })
                            .px_1()
                    })
                    // M1: failed sends get a red outline so the retry
                    // affordance is visible (`updateMessageSendFailed`).
                    .when(failed, |this| {
                        this.rounded_lg().border_1().border_color(danger()).px_1()
                    })
                    // M1: right-click opens the message context menu at
                    // the click position (window coordinates).
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.message_menu = Some(MessageMenuState {
                                chat_id: row_chat,
                                message_id: row_msg,
                                position: event.position,
                            });
                            cx.notify();
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
                    .into_any_element()
            }
        }
    }

    /// kit Phase 3: automatic older-history paging — called (deferred) when
    /// the top row of the virtualized history becomes visible. Notifies the
    /// frame only when a request was actually sent (the driver dedupes
    /// in-flight `getChatHistory` requests and stops at `loaded_complete`).
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
