//! Methods moved out of `conversation.rs` to keep files under 1000 lines.

use super::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// One part of the open chat's column: the composer
    /// (`ConversationPart::Bottom`) renders as its own cached slice, so
    /// typing and the caret's blink don't rebuild the history
    /// (`app_slice`).
    pub(in crate::ui) fn conversation(
        &mut self,
        part: ConversationPart,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if part.top() {
            // One instant for the rows work and the dust of this frame, in
            // panes that build no rows too.
            self.pin_vanish_now();
        }
        let mode = self.pane_mode();
        let history = match mode {
            _ if !part.top() => Empty.into_any_element(),
            PaneMode::Synthetic => div()
                .id("conversation-history")
                .flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .role(Role::Log)
                .aria_label(tr(keys::Q_CHAT_MESSAGE_HISTORY))
                .child(self.chat.clone())
                .into_any_element(),
            PaneMode::Connecting => pane_placeholder(
                tr(keys::Q_CHAT_NOT_SIGNED_IN),
                tr(keys::Q_CHAT_SIGN_IN_TO_SEE_YOUR),
                cx,
            )
            .into_any_element(),
            PaneMode::Ready => self.session_history(cx).into_any_element(),
        };
        let composer = self.composer_available(mode).then_some(true);
        let composer_note: Option<String> = match mode {
            PaneMode::Connecting => Some(tr(keys::Q_CHAT_SIGN_IN_TO_SEND_MESSAGES).to_string()),
            PaneMode::Ready
                if composer.is_none() && self.session().is_some_and(|s| s.is_frozen()) =>
            {
                Some(tr(keys::Q_CHAT_YOUR_ACCOUNT_IS_FROZEN_AND_2).to_string())
            }
            PaneMode::Ready if composer.is_none() => {
                let open = self.session().and_then(|s| s.open_chat);
                let in_topic = self.session().is_some_and(|s| s.open_topic.is_some());
                let is_channel = open.is_some_and(|id| {
                    self.session()
                        .and_then(|s| s.chats.get(&id.0).map(|c| c.is_channel()))
                        .unwrap_or(false)
                });
                if is_channel || self.saved_readonly() || self.bottom_action().is_some() {
                    // The join/leave footer replaces the plain note for
                    // channels; Saved sublists and tag filters need none.
                    None
                } else if let Some(reason) = self.composer_restriction() {
                    Some(reason)
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
                        Some(tr(keys::Q_CHAT_THIS_TOPIC_IS_CLOSED_NEW).to_string())
                    } else if !send_allowed {
                        Some(tr(keys::Q_CHAT_YOU_DONT_HAVE_PERMISSION_TO).to_string())
                    } else {
                        // The topic's info hasn't loaded yet; the composer
                        // appears once it arrives.
                        Some(tr(keys::Q_CHAT_LOADING_TOPIC).to_string())
                    }
                } else if open.is_none() {
                    Some(tr(keys::Q_CHAT_SELECT_A_CHAT_TO_START).to_string())
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
                            tr(keys::Q_CHAT_THIS_CONVERSATION_TYPE_IS_NOT).to_string(),
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
        let top = part.top();
        let dust = top.then(|| self.vanish_overlay(cx)).flatten();
        let player_bar = top.then(|| self.player_bar(cx)).flatten();
        let call_bar = top
            .then(|| self.call_bar(cx).or_else(|| self.group_call_bar(cx)))
            .flatten();
        let capture_notice = top.then(|| self.capture_notice(cx)).flatten();
        div()
            .relative()
            .flex()
            .flex_col()
            .when(top, |this| this.flex_1().min_h_0())
            .min_w_0()
            .children(player_bar)
            .children(call_bar)
            .children(capture_notice)
            .when(top, |this| this.child(history))
            .children(
                top.then(|| self.subsection_tabs_strip(SubsectionTabsMode::Bottom, cx))
                    .flatten(),
            )
            // Phase C2i: busy-decline banner — the calls that arrived
            // while another call was active were declined with
            // `discardCall` (TDLib has no hold/swap API). Dismissible.
            .when_some(
                top.then(|| self.call_busy_banner(cx)).flatten(),
                |this, banner| this.child(banner),
            )
            .when(composer.is_some() && part.bottom(), |this| {
                let show_attach =
                    matches!(mode, PaneMode::Ready) && self.composer_ui.pending_edit.is_none();
                // Something to send (text, an attachment, an edit): the
                // composer shows Send instead of the mic.
                let sendable = !show_attach
                    || self.forward_bar_here()
                    || !self.pending_attachments().is_empty()
                    || !self.composer.read(cx).value().trim().is_empty();
                this.child(
                    div()
                        .id("composer-file-drop")
                        .relative()
                        // The emoji / sticker / GIF popover floats above the
                        // composer, anchored to its left edge.
                        .when(
                            self.media_panel_open()
                                && self.pickers.media_panel.reaction.is_none()
                                && self.pickers.media_panel.status.is_none()
                                && !self.story_reply_panel_active()
                                && !self.group_title_emoji_active(),
                            |this| {
                                let panel = self.media_panel(cx);
                                // Over the history: animations under it are
                                // drawn by the conversation (`anim_layer`).
                                this.child(
                                    div()
                                        .absolute()
                                        .left(px(8.))
                                        .bottom(relative(1.))
                                        .pb_1()
                                        .child(super::anim_layer::occluder(panel)),
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
                                    .child(super::anim_layer::occluder(circle)),
                            )
                        })
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .when(self.recording_here(), |this| {
                            this.child(self.record_bar(cx))
                        })
                        .when(
                            show_attach && !self.pending_attachments().is_empty(),
                            |this| this.child(self.composer_attachment_tray(cx)),
                        )
                        .when_some(self.share.forward_result.clone(), |this, result| {
                            this.child(self.forward_success_banner(&result, cx))
                        })
                        .when(
                            self.forward_draft().is_some()
                                && !self.share.forward_picker_open
                                && !self.forward_bar_here()
                                // Selecting here: the header carries the buttons.
                                && !self
                                    .session()
                                    .and_then(|s| s.open_chat)
                                    .is_some_and(|chat| self.selecting_in(chat)),
                            |this| {
                                this.when_some(self.forward_draft(), |this, draft| {
                                    this.child(self.forward_selection_banner(&draft, cx))
                                })
                            },
                        )
                        .when(self.forward_bar_here(), |this| {
                            this.child(self.forward_bar(cx))
                        })
                        .when(self.composer_ui.send_as_open, |this| {
                            this.child(self.send_as_panel(cx))
                        })
                        .when_some(self.message_ui.pending_delete.clone(), |this, _| {
                            this.child(self.delete_confirm_banner(cx))
                        })
                        // B4: stop-poll / stop-quiz confirm banner.
                        .when_some(self.message_ui.pending_stop_poll, |this, _| {
                            this.child(self.stop_poll_confirm_banner(cx))
                        })
                        // Phase B1: close-secret-chat confirm banner.
                        .when_some(self.chat_list.pending_close_secret_chat, |this, _| {
                            this.child(self.close_secret_chat_confirm_banner(cx))
                        })
                        // Phase S2: inline-bot warning banner for secret chats.
                        // Gated on the open chat still being one: a pending
                        // alert from a previous chat never renders elsewhere.
                        .when(
                            self.composer_ui.pending_inline_bot_alert.is_some()
                                && self.open_chat_is_secret(),
                            |this| this.child(self.inline_bot_alert_banner(cx)),
                        )
                        .when_some(self.composer_ui.pending_edit.clone(), |this, edit| {
                            this.child(self.composer_edit_banner(&edit, cx))
                        })
                        .when_some(self.composer_ui.reply().cloned(), |this, reply| {
                            this.child(self.composer_reply_banner(&reply, cx))
                        })
                        .when_some(self.suggest_bar_panel(cx), |this, bar| this.child(bar))
                        .when_some(self.reply_elsewhere_panel(cx), |this, panel| {
                            this.child(panel)
                        })
                        .when_some(self.reply_quote_panel(cx), |this, panel| this.child(panel))
                        // Phase 4.2: poll creation dialog above the composer.
                        .when_some(self.poll_dialog_panel(cx), |this, panel| this.child(panel))
                        // B15: checklist composer / "Add Tasks" and the poll
                        // "Add an Option" row above the composer.
                        .when_some(self.checklist_dialog_panel(cx), |this, panel| {
                            this.child(panel)
                        })
                        .when_some(self.poll_add_option_panel(cx), |this, panel| {
                            this.child(panel)
                        })
                        // The attach menu's Contact / Location panel.
                        .when_some(self.share_content_panel(cx), |this, panel| {
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
                        .when_some(self.suggest_menu_dropdown(cx), |this, panel| {
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
                        // until the wait expires; the send button shows the
                        // remaining time (`composer_send_button`) and
                        // `ensure_slow_mode_tick` re-renders every second.
                        .when_some(self.composer_link_dialog_panel(cx), |this, panel| {
                            this.child(panel)
                        })
                        .when_some(self.composer_date_dialog_panel(cx), |this, panel| {
                            this.child(panel)
                        })
                        .when_some(self.code_language_panel(cx), |this, panel| {
                            this.child(panel)
                        })
                        // Non-default send options as clearable chips; the
                        // schedule picker opens above the input.
                        // Pickers open directly above the input, next to the
                        // buttons that summon them.
                        .when(self.composer_ui.schedule_popup_open, |this| {
                            this.child(self.schedule_popup(cx))
                        })
                        .when_some(self.paid_messages_notice(cx), |this, row| this.child(row))
                        .when_some(self.composer_options_row(cx), |this, row| this.child(row))
                        // MED4: detected-URL chip (send-time preview
                        // control) and caption bar ("Add a caption…",
                        // above/below toggle, n / max counter).
                        .when_some(self.preview_chip(cx), |this, chip| this.child(chip))
                        .when_some(self.edit_replacement_chip(cx), |this, chip| {
                            this.child(chip)
                        })
                        .when_some(self.caption_bar(cx), |this, bar| this.child(bar))
                        .when_some(self.text_limit_bar(cx), |this, bar| this.child(bar))
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
                                    .child(
                                        self.ai_compose_attach_column(
                                            self.attach_menu_button(polls_allowed, cx)
                                                .into_any_element(),
                                            cx,
                                        ),
                                    )
                                    .child(
                                        Button::new("composer-emoji")
                                            .icon(IconName::FaceSlightlySmiling)
                                            .ghost()
                                            .tooltip(if self.sticker_panel_open() {
                                                tr(keys::Q_CHAT_CLOSE_STICKERS)
                                            } else {
                                                tr(keys::Q_CHAT_STICKERS)
                                            })
                                            .accessibility_label(tr(keys::Q_CHAT_STICKERS))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_media_panel(
                                                    super::media_panel::PanelTab::Emoji,
                                                    cx,
                                                );
                                            })),
                                    )
                                    .child(self.format_menu_button(cx))
                                })
                                .when(!show_attach, |row| row.children(self.ai_compose_button(cx)))
                                .child(
                                    div()
                                        .relative()
                                        .flex_1()
                                        .min_w_0()
                                        // Scopes Cmd/Ctrl+K to "edit link" here
                                        // (see `composer_bindings`).
                                        .key_context(super::keybindings::COMPOSER_CONTEXT)
                                        .child(
                                            Textarea::new(&self.composer)
                                                .appearance(false)
                                                .bordered(false)
                                                .aria_label(tr(keys::SR_CHAT_COLUMN_MESSAGE))
                                                // Custom emoji drawn inline
                                                // (codex:composer-input).
                                                .token(self.composer_token_renderer(cx))
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
                                        // The inline bot's hint after `@bot `.
                                        .when_some(
                                            self.inline_bot_hint_overlay(cx),
                                            |wrap, hint| wrap.child(hint),
                                        )
                                        // Red wavy underlines over the
                                        // misspelled words.
                                        .when_some(
                                            self.spellcheck_underlines(cx),
                                            |wrap, lines| wrap.child(lines),
                                        ),
                                )
                                // Scheduled-messages button while the chat has
                                // any (`updateChatHasScheduledMessages`).
                                .when_some(self.scheduled_messages_button(cx), |row, button| {
                                    row.child(button)
                                })
                                // Show / hide the bot's reply keyboard.
                                .when_some(self.keyboard_toggle_button(cx), |row, button| {
                                    row.child(button)
                                })
                                .when_some(self.suggest_post_toggle(cx), |row, button| {
                                    row.child(button)
                                })
                                // "Send as" identity of the chat
                                // (`chat.message_sender_id`).
                                .when_some(self.send_as_button(cx), |row, button| row.child(button))
                                // Telegram Desktop's round button: the mic
                                // while there's nothing to send, Send once
                                // there is, Save when editing, the slow-mode
                                // countdown while the chat is rate-limited.
                                .child(self.composer_send_button(show_attach, sendable, cx)),
                        )
                        // M2: rich editor block bar + live block preview
                        // under the textarea while the editor is open.
                        .when(self.composer_ui.rich_editor_open, |this| {
                            this.child(self.rich_editor_bar(cx))
                        }),
                )
            })
            .when_some(composer_note.filter(|_| part.bottom()), |this, note| {
                // A rights restriction is centered like tdesktop's
                // `TextErrorSendRestriction`; other notes stay left.
                let centered = self.composer_restriction().is_some();
                this.child(
                    div()
                        .id("composer-note")
                        .p_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .when(centered, |this| this.text_center())
                        .child(note),
                )
            })
            .when_some(
                (part.bottom() && !self.thread_pending())
                    .then(|| self.channel_footer(cx))
                    .flatten(),
                |this, footer| this.child(footer),
            )
            .when_some(
                (part.bottom() && !self.thread_pending())
                    .then(|| self.bottom_action_bar(cx))
                    .flatten(),
                |this, bar| this.child(bar),
            )
            // A deleted message's dust drifts over everything.
            .children(dust.map(super::anim_layer::occluder))
    }

    pub(in crate::ui) fn session_history(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open = session.and_then(|s| s.open_chat);
        let title = open
            .and_then(|id| session.and_then(|s| s.chats.get(&id.0).map(|c| c.title.clone())))
            .unwrap_or_else(|| tr(keys::Q_CHAT_NO_CHAT_SELECTED).into());
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
        let chat_search_open = session.is_some_and(|s| s.search.chat_search.open);
        let highlight_id = session.and_then(|s| match s.search.chat_search.jump {
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
                session.and_then(|s| s.threads.topic_histories.get(&(id.0, topic_id)))
            }
            _ => None,
        };
        let topic_empty = topic_history.is_none_or(|h| h.messages.is_empty());
        let topic_key_ids = open.zip(open_topic);
        let topic_loading = topic_empty
            && topic_key_ids.is_some_and(|(id, topic)| {
                session.is_some_and(|s| s.topic_history_loading(id, topic))
            });
        let topic_failed = topic_empty
            && topic_key_ids.is_some_and(|(id, topic)| {
                session.is_some_and(|s| s.topic_history_load_failed(id, topic))
            });
        // Channel comments / group reply thread of the open chat.
        let thread_open = open
            .and_then(|id| session.and_then(|s| s.thread_for_chat(id)))
            .filter(|thread| !matches!(thread.status, quill::state::ThreadStatus::Failed(_)));
        let thread_pending = session.is_some_and(Session::thread_unavailable);
        // Saved Messages: the sublist list, one sublist or a tag filter.
        let saved_mode = self.saved_mode();
        let saved_rows = self.saved_rows_input(highlight_id);
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
            .when_some(
                topic_info.clone().filter(|_| self.history.topic_info_open),
                |this, info| this.child(self.topic_info_card(&info, cx)),
            )
            .children(self.subsection_tabs_strip(SubsectionTabsMode::Top, cx))
            .children(self.saved_tags_bar(cx))
            .when_some(self.bot_info_panel(cx), |this, panel| this.child(panel))
            .when(self.notify.mute_menu_open, |this| {
                this.child(self.mute_menu_panel(cx))
            })
            // Phase B4: self-destruct / auto-delete timer picker below
            // the header.
            .when(self.notify.ttl_picker_open, |this| {
                this.child(self.ttl_picker_panel(cx))
            })
            // Parity slice: per-chat folder picker below the header.
            .when(self.folders.menu_open, |this| {
                this.child(self.folder_menu_panel(cx))
            })
            .children(
                open.map(|chat_id| self.chat_top_bars(chat_id, cx))
                    .unwrap_or_default(),
            )
            .children(open.and_then(|chat_id| self.pinned_message_banner(chat_id, cx)))
            .children(
                open.filter(|_| self.history.pinned_list_open)
                    .and_then(|chat_id| self.pinned_list_panel(chat_id, cx)),
            )
            .when(self.share.forward_picker_open, |this| {
                this.child(self.forward_picker_panel(cx))
            })
            .when_some(self.pending_bot_reply(cx), |this, reply| this.child(reply))
            .when(chat_search_open, |this| {
                this.child(self.chat_search_bar(cx))
            })
            .child(super::subsection_tabs::with_left_column(
                tabs_left,
                if let Some(reason) = gate {
                    pane_placeholder(tr(keys::Q_CHAT_UNSUPPORTED_CHAT), reason, cx)
                        .into_any_element()
                } else if open.is_none() {
                    pane_placeholder(
                        tr(keys::Q_CHAT_SELECT_A_CHAT),
                        tr(keys::Q_CHAT_CHOOSE_A_CONVERSATION_FROM_THE),
                        cx,
                    )
                    .into_any_element()
                } else if !supported {
                    pane_placeholder(
                        tr(keys::Q_CHAT_UNSUPPORTED_CHAT),
                        tr(keys::Q_CHAT_THIS_CONVERSATION_TYPE_IS_NOT),
                        cx,
                    )
                    .into_any_element()
                } else if saved_mode == Some(super::saved_sublists::SavedMode::Sublists) {
                    self.saved_sublists_pane(cx)
                } else if let Some((key, rows)) = saved_rows {
                    let list = self.history_message_list(
                        "saved-history",
                        rows,
                        chat.as_ref(),
                        &sender_name,
                        highlight_id,
                        media_roots,
                        cx,
                    );
                    self.history.rows_key = Some(key);
                    list
                } else if thread_pending {
                    self.thread_status_pane(cx)
                } else if let Some(thread) = thread_open {
                    let (key, rows) = self.thread_rows_input(thread, highlight_id);
                    let list = self.history_message_list(
                        "thread-history",
                        rows,
                        chat.as_ref(),
                        &sender_name,
                        highlight_id,
                        media_roots,
                        cx,
                    );
                    self.history.rows_key = Some(key);
                    self.thread_history_pane(list, cx)
                } else if is_forum
                    && open_topic.is_none()
                    && !tabs_used
                    && open.is_some_and(|id| session.is_some_and(|s| s.chat_views_as_topics(id)))
                {
                    // Phase 5.1: opening a forum supergroup shows its topics;
                    // with the topic column on screen the pane only asks for
                    // a choice.
                    if self.chat_list.forum_column_shown {
                        pane_placeholder(
                            tr(keys::Q_CHAT_CHOOSE_A_TOPIC),
                            tr(keys::Q_CHAT_PICK_A_TOPIC_FROM_THE),
                            cx,
                        )
                        .into_any_element()
                    } else {
                        self.forum_topics_pane(open, cx).into_any_element()
                    }
                } else if has_topics && open_topic.is_some() {
                    // Phase 5.1: per-topic history — same history component,
                    // fed from the topic history store (`searchChatMessages`
                    // with `topic_id`).
                    if topic_failed {
                        self.topic_load_failed_row(cx).into_any_element()
                    } else if topic_loading {
                        history_skeleton().into_any_element()
                    } else if topic_empty {
                        pane_placeholder(
                            tr(keys::Q_CHAT_NO_MESSAGES_IN_THIS_TOPIC),
                            tr(keys::Q_CHAT_NO_MESSAGES_IN_THIS_TOPIC_2),
                            cx,
                        )
                        .into_any_element()
                    } else {
                        let (key, rows) =
                            self.topic_rows_input(open, open_topic, topic_history, highlight_id);
                        let list = self.history_message_list(
                            "topic-history",
                            rows,
                            chat.as_ref(),
                            &sender_name,
                            highlight_id,
                            media_roots,
                            cx,
                        );
                        self.history.rows_key = Some(key);
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
                    // R5: the first page failed or timed out — a Retry row
                    // instead of an endless skeleton or a false "No messages".
                    let history_failed =
                        open.is_some_and(|id| session.is_some_and(|s| s.history_load_failed(id)));
                    // Nothing to show and no page on the way (an answer for
                    // a window that was replaced, or none asked yet): ask
                    // for the first page. The driver sends nothing for a
                    // chat known to be empty, so this cannot loop.
                    if open.is_some() && !history_loading && !history_failed {
                        let weak = cx.weak_entity();
                        let window_chat = self.windows.scope;
                        cx.defer(move |cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.in_window_scope(window_chat, cx, |this, cx| {
                                    this.maybe_auto_load_older(cx)
                                })
                            });
                        });
                    }
                    if history_failed {
                        history_load_failed(cx)
                    } else if history_loading {
                        history_skeleton().into_any_element()
                    } else if is_secret {
                        self.secret_empty_explainer(cx).into_any_element()
                    } else if let Some(intro) = self.bot_about_intro(chat.as_ref(), cx) {
                        intro
                    } else if let Some(intro) = self.greeting_intro(chat.as_ref(), cx) {
                        intro
                    } else {
                        // tdesktop paints nothing for an empty group or channel
                        // it did not create; no developer text in the pane.
                        div().id("empty-chat").flex_1().into_any_element()
                    }
                } else {
                    let (key, rows) =
                        self.main_rows_input(open, open_topic, main_history, highlight_id);
                    let list = self.history_message_list(
                        "session-history",
                        rows,
                        chat.as_ref(),
                        &sender_name,
                        highlight_id,
                        media_roots,
                        cx,
                    );
                    self.history.rows_key = Some(key);
                    // Channels end with their sponsored message (Telegram API
                    // terms: clients must show it), but only while the
                    // history is scrolled to the bottom.
                    let scrolled_up = self.history.scroller.read(cx).is_scrolled_up();
                    match self.sponsored_footer(scrolled_up, cx) {
                        Some(footer) => history_with_sponsored(list, footer),
                        None => list,
                    }
                },
            ))
    }

    /// A reply thread's history under its root bar and info card.
    #[inline(never)]
    fn thread_history_pane(&mut self, list: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .children(self.thread_root_bar(cx))
            .when(self.history.thread_info_open, |this| {
                this.children(self.thread_info_card(cx))
            })
            .child(list)
            .into_any_element()
    }
}

// `session_history` builds one of many panes; the larger ones are built
// here so its frame stays small in a debug build
// (docs/decisions/main-thread-stack.md).

/// The first page failed or timed out: the reason and Retry.
#[inline(never)]
fn history_load_failed(cx: &mut Context<QuillApp>) -> AnyElement {
    div()
        .id("history-load-failed")
        .flex_1()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap_2()
        .p_6()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_CHAT_COULDN_T_LOAD_MESSAGES)),
        )
        .child(
            Button::new("history-retry")
                .label(tr(keys::Q_CHAT_RETRY))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.retry_history_load(cx);
                })),
        )
        .into_any_element()
}

/// A channel's history with its sponsored message at the end.
#[inline(never)]
fn history_with_sponsored(list: AnyElement, footer: AnyElement) -> AnyElement {
    div()
        .id("history-with-sponsored")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(list)
        .child(footer)
        .into_any_element()
}
