//! Methods moved out of `message_actions.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// Slice CL: dismiss the peek preview (click anywhere, the release
    /// that ends the long press, or Escape). Also cancels a pending
    /// long press.
    pub(in crate::ui) fn close_chat_preview(&mut self, cx: &mut Context<Self>) {
        if self.chat_list.preview.is_some() || self.chat_list.preview_press.is_some() {
            self.chat_list.preview = None;
            self.chat_list.preview_press = None;
            cx.notify();
        }
    }

    /// Slice CL: the floating peek preview — chat title + the most
    /// recent messages as read-only sender/body rows. Rendered absolute
    /// beside the pressed row (same pattern as `chat_menu_overlay`); any
    /// click on the catcher closes it.
    pub(in crate::ui) fn chat_preview_overlay(
        &self,
        preview: ChatPreviewState,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let chat_id = preview.chat_id;
        let session = self.session();
        let title = session
            .and_then(|session| session.chats.get(&chat_id.0))
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| "Chat".to_string());
        // Prefer already-loaded messages (open chats, recent traffic,
        // demo fixtures); the one-shot live fetch fills unopened chats.
        let mut lines: Vec<(String, String, u64)> = session
            .and_then(|session| session.histories.get(&chat_id.0))
            .map(|history| {
                history
                    .ordered()
                    .iter()
                    .rev()
                    .take(PREVIEW_HISTORY_LIMIT as usize)
                    .rev()
                    .map(|message| {
                        chat_preview_line(
                            &title,
                            message.is_outgoing,
                            message.author_signature.as_deref(),
                            &message.content,
                            message.ephemeral.as_ref(),
                            message.id.0 as u64,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let fetch = session.and_then(|session| {
            session
                .chat_list
                .chat_preview_fetch
                .as_ref()
                .filter(|fetch| fetch.chat_id == chat_id)
        });
        if lines.is_empty()
            && let Some(fetch) = fetch
        {
            lines = fetch
                .messages
                .iter()
                .map(|message| {
                    chat_preview_line(
                        &title,
                        message.is_outgoing,
                        message.author_signature.as_deref(),
                        &message.content,
                        message.ephemeral.as_ref(),
                        message.id.0 as u64,
                    )
                })
                .collect();
        }
        let failed = fetch.and_then(|fetch| fetch.failed.clone());
        let in_flight = session.is_some_and(|session| {
            session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetChatPreview, chat_id)
        });
        let mut body = div()
            .id(("chat-preview-scroll", chat_id.0 as u64))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .overflow_y_scroll()
            .max_h(px(360.));
        if let Some(failed) = failed {
            body = body.child(div().text_sm().text_color(danger()).child(failed));
        } else if !lines.is_empty() {
            for (name, text, id) in lines {
                body = body.child(
                    div()
                        .id(("chat-preview-row", id))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(cx.theme().muted_foreground)
                                .child(name),
                        )
                        .child(div().text_sm().child(text)),
                );
            }
        } else if in_flight {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Skeleton::new().w(px(180.)).h(px(12.)).rounded_sm())
                    .child(
                        Skeleton::new()
                            .w(px(140.))
                            .h(px(10.))
                            .rounded_sm()
                            .secondary(),
                    ),
            );
        } else {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No messages yet."),
            );
        }
        let panel = div()
            .id("chat-preview-panel")
            .flex()
            .flex_col()
            .w(px(320.))
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(bg_canvas())
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .text_sm()
                    .font_semibold()
                    .child(title),
            )
            .child(body);
        div()
            .id("chat-preview-overlay")
            .occlude()
            .absolute()
            .inset_0()
            // The release that ends the long press lands here (the
            // catcher is on top of the rows) — stop it before the rows
            // behind can act on it.
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            // Same for right-clicks: don't let the rows' context menu
            // open behind the preview.
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.close_chat_preview(cx);
                    // Consume the release that ends the long press: the
                    // row's `pending_mouse_down` is still armed, and
                    // without this the release would open the chat.
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(f32::from(preview.anchor.x) + 12.))
                    .top(px((f32::from(preview.anchor.y) - 40.).max(8.)))
                    .child(panel),
            )
            .into_any_element()
    }

    /// Slice CL1: right-click chat-row context menu — Pin/Unpin, Mark
    /// as read/unread, Mute/Unmute, Archive/Unarchive, Clear history,
    /// Delete. Rendered absolute at the click position; any click on
    /// the backdrop closes it. Same structure as
    /// `message_menu_overlay`.
    pub(in crate::ui) fn chat_menu_overlay(
        &self,
        menu: ChatMenuState,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let chat_id = menu.chat_id;
        let chat = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .cloned();
        let Some(chat) = chat else {
            return div().into_any_element();
        };
        let pinned = if chat.in_archive {
            chat.archive_is_pinned
        } else {
            chat.is_pinned
        };
        let unread = chat.is_marked_as_unread || chat.unread_count > 0;
        let muted = chat.is_muted();
        let archived = chat.in_archive;
        let can_clear = chat.can_be_deleted_only_for_self || chat.can_be_deleted_for_all_users;
        let can_revoke = chat.can_be_deleted_for_all_users;

        // Telegram Desktop's chat menu: left-aligned rows with an icon, in
        // its order (Archive, Pin, Mute, Mark as read/unread, Clear
        // history, Delete chat); Quill's extras slot in before Delete.
        let mut rows: Vec<(u8, AnyElement)> = Vec::new();
        let row_hover = cx.theme().accent;
        macro_rules! item {
            ($order:expr, $icon:expr, $id:expr, $label:expr, $this:ident, $cx:ident, $body:block) => {
                let danger = $id == "chat-menu-delete";
                rows.push((
                    $order,
                    div()
                        .id($id)
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .cursor_pointer()
                        .text_sm()
                        .text_color(if danger { danger_bright() } else { text_menu() })
                        .hover(|style| style.bg(row_hover))
                        .role(gpui_kit::Role::MenuItem)
                        .aria_label($label)
                        .child(Icon::new($icon).size(px(16.)))
                        .child($label)
                        .on_click($cx.listener(move |$this, _, _, $cx| $body))
                        .into_any_element(),
                ));
            };
        }
        // Same as `item!`, for bodies that need the window.
        macro_rules! item_window {
            ($order:expr, $icon:expr, $id:expr, $label:expr, $this:ident, $window:ident, $cx:ident, $body:block) => {
                rows.push((
                    $order,
                    div()
                        .id($id)
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .cursor_pointer()
                        .text_sm()
                        .text_color(text_menu())
                        .hover(|style| style.bg(row_hover))
                        .role(gpui_kit::Role::MenuItem)
                        .aria_label($label)
                        .child(Icon::new($icon).size(px(16.)))
                        .child($label)
                        .on_click($cx.listener(move |$this, _, $window, $cx| $body))
                        .into_any_element(),
                ));
            };
        }
        use gpui_kit::assets::IconName as Lucide;
        item!(
            10,
            if archived {
                Lucide::ArchiveRestore
            } else {
                Lucide::Archive
            },
            "chat-menu-archive",
            if archived { "Unarchive" } else { "Archive" },
            this,
            cx,
            {
                this.toggle_archive(chat_id, cx);
                this.chat_list.menu = None;
                cx.notify();
            }
        );
        item!(
            20,
            if pinned { Lucide::PinOff } else { Lucide::Pin },
            "chat-menu-pin",
            if pinned { "Unpin" } else { "Pin" },
            this,
            cx,
            {
                this.toggle_chat_pin(chat_id, cx);
                this.chat_list.menu = None;
                cx.notify();
            }
        );
        item!(
            30,
            if muted { Lucide::Bell } else { Lucide::BellOff },
            "chat-menu-mute",
            if muted {
                "Unmute notifications"
            } else {
                "Mute notifications"
            },
            this,
            cx,
            {
                this.apply_chat_mute(chat_id, if muted { 0 } else { MUTE_FOREVER }, cx);
                this.chat_list.menu = None;
                cx.notify();
            }
        );
        item!(
            40,
            if unread {
                Lucide::CircleCheck
            } else {
                Lucide::MessageSquareDot
            },
            "chat-menu-read",
            if unread {
                "Mark as read"
            } else {
                "Mark as unread"
            },
            this,
            cx,
            {
                this.toggle_chat_marked_as_unread(chat_id, cx);
                this.chat_list.menu = None;
                cx.notify();
            }
        );
        // tdesktop's `Filler`: View profile, and the "mark as read" entries
        // for unread mentions, reactions and poll votes.
        let extras = quill::chatlist_menu::row_menu_extras(quill::chatlist_menu::RowMenuFacts {
            kind: &chat.kind,
            is_saved_messages: self.session().is_some_and(|s| s.is_saved_messages(chat_id)),
            unread_mentions: chat.unread_mention_count,
            unread_reactions: chat.unread_reaction_count,
            unread_poll_votes: chat.unread_poll_vote_count,
            protected: self
                .session()
                .is_some_and(|s| s.chat_has_protected_content(chat_id)),
            live: self.live.is_some(),
        });
        if let Some(label) = extras.view_profile {
            item_window!(
                35,
                Lucide::CircleUser,
                "chat-menu-profile",
                label,
                this,
                window,
                cx,
                {
                    this.chat_list.menu = None;
                    if let Some(target) = this
                        .session()
                        .and_then(|s| s.info_panel_target_for_chat(chat_id))
                    {
                        this.open_info_panel_target(target, window, cx);
                    }
                    cx.notify();
                }
            );
        }
        if extras.read_mentions {
            item!(
                41,
                Lucide::AtSign,
                "chat-menu-read-mentions",
                "Mark all mentions as read",
                this,
                cx,
                {
                    this.read_chat_unread_markers(
                        chat_id,
                        quill::state::UnreadJumpKind::Mention,
                        cx,
                    );
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        if extras.read_reactions {
            item!(
                42,
                Lucide::Heart,
                "chat-menu-read-reactions",
                "Read all reactions",
                this,
                cx,
                {
                    this.read_chat_unread_markers(
                        chat_id,
                        quill::state::UnreadJumpKind::Reaction,
                        cx,
                    );
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        if extras.read_poll_votes {
            item!(
                43,
                Lucide::ChartBar,
                "chat-menu-read-poll-votes",
                "Read all poll votes",
                this,
                cx,
                {
                    this.read_chat_unread_markers(
                        chat_id,
                        quill::state::UnreadJumpKind::PollVote,
                        cx,
                    );
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        // Slice CL3: enter multi-select mode with this chat checked.
        item!(
            50,
            Lucide::ListChecks,
            "chat-menu-select",
            "Select",
            this,
            cx,
            {
                this.enter_select_mode(chat_id, cx);
                this.chat_list.menu = None;
                cx.notify();
            }
        );
        if can_clear {
            item!(
                60,
                Lucide::Eraser,
                "chat-menu-clear",
                "Clear history",
                this,
                cx,
                {
                    this.open_group_confirm(
                        chat_id,
                        GroupConfirmAction::ClearHistory { revoke: false },
                        cx,
                    );
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        if can_revoke {
            item!(
                61,
                Lucide::Eraser,
                "chat-menu-clear-all",
                "Clear history for everyone",
                this,
                cx,
                {
                    this.open_group_confirm(
                        chat_id,
                        GroupConfirmAction::ClearHistory { revoke: true },
                        cx,
                    );
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        // Slice CL3: Report — gated on `chat.can_be_reported` (schema
        // 1.8.67, line 3606); sends the simple spam report
        // (`reportChat` with empty option_id/message_ids, schema:3667).
        if chat.can_be_reported {
            item!(70, Lucide::Flag, "chat-menu-report", "Report", this, cx, {
                this.open_group_confirm(chat_id, GroupConfirmAction::ReportChat, cx);
                this.chat_list.menu = None;
                cx.notify();
            });
        }
        // Slice CL3: Block/Unblock the peer of a private or secret chat
        // (`setMessageSenderBlockList`, schema 1.8.67, line 14492;
        // schema:3674 covers secret chats). Never offered for the user's
        // own chat.
        let blockable = match &chat.kind {
            ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => {
                let is_self = self
                    .session()
                    .and_then(|s| s.my_user_id)
                    .is_some_and(|me| me == user_id.0);
                (!is_self).then_some(user_id.0)
            }
            _ => None,
        };
        if blockable.is_some() {
            let blocked = chat.blocked;
            item!(
                75,
                Lucide::Ban,
                "chat-menu-block",
                if blocked {
                    "Unblock user"
                } else {
                    "Block user"
                },
                this,
                cx,
                {
                    this.open_group_confirm(
                        chat_id,
                        GroupConfirmAction::BlockUser { block: !blocked },
                        cx,
                    );
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        if extras.export {
            item!(
                85,
                Lucide::Download,
                "chat-menu-export",
                "Export chat history",
                this,
                cx,
                {
                    this.start_chat_export(chat_id, cx);
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        if can_clear {
            item!(
                90,
                Lucide::Trash,
                "chat-menu-delete",
                "Delete chat",
                this,
                cx,
                {
                    this.open_group_confirm(chat_id, GroupConfirmAction::RemoveFromList, cx);
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        rows.sort_by_key(|(order, _)| *order);
        let panel = div()
            .id("chat-menu-panel")
            .occlude()
            .flex()
            .flex_col()
            .min_w(px(200.))
            .px_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .children(rows.into_iter().map(|(_, row)| row));
        div()
            .id("chat-menu-overlay")
            .track_focus(&self.frame.context_menu_focus)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .child(
                div()
                    .id("chat-menu-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.chat_list.menu = None;
                        cx.notify();
                    })),
            )
            .child(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(panel),
            )
            .focus_trap("chat-menu-focus", &self.frame.context_menu_focus)
            .into_any_element()
    }

    /// MED4: Instant View reader overlay (TGX behavior — attempt IV
    /// when the card offers it, fall back to the browser on 404 /
    /// unsupported). Drains `Session::instant_view_fallback_url` into
    /// the browser and renders `Session::instant_view` page blocks via
    /// `message_rich_block`. A refusal is never rendered as a reader.
    pub(in crate::ui) fn instant_view_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(live) = self.live.as_mut()
            && let Some(url) = live
                .driver
                .session
                .messages
                .instant_view_fallback_url
                .take()
        {
            self.open_message_url(&url, cx);
        }
        let page = self
            .live
            .as_ref()?
            .driver
            .session
            .messages
            .instant_view
            .clone()?;
        let url = page.url.clone();
        Some(
            div()
                .id("instant-view-overlay")
                .occlude()
                .absolute()
                .inset_0()
                .bg(bg_deep())
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .py_2()
                        .border_b_1()
                        .border_color(border())
                        .child(div().text_sm().font_medium().child("Instant View"))
                        .child(
                            Button::new("instant-view-close")
                                .icon(gpui_kit::assets::IconName::X)
                                .tooltip("Close")
                                .accessibility_label("Close")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(live) = this.live.as_mut() {
                                        live.driver.session.messages.instant_view = None;
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_xs().text_color(text_muted()).child(url))
                        .child(message_rich_block(
                            (0, 0),
                            ChatId(0),
                            MessageId(0),
                            &page.rich,
                            &std::collections::HashSet::new(),
                            // Settings → Appearance: message font size.
                            self.msg_font(),
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }

    /// M1: text that "Copy" can copy — the message text, or a non-empty
    /// media caption.
    pub(in crate::ui) fn message_copyable_text(content: &MessageContent) -> Option<String> {
        match content {
            MessageContent::Text(text) if !text.text.is_empty() => Some(text.text.clone()),
            MessageContent::Photo(photo) if !photo.caption.is_empty() => {
                Some(photo.caption.clone())
            }
            MessageContent::Document(document) if !document.caption.is_empty() => {
                Some(document.caption.clone())
            }
            MessageContent::Animation(animation) if !animation.caption.is_empty() => {
                Some(animation.caption.clone())
            }
            MessageContent::Video(video) if !video.caption.is_empty() => {
                Some(video.caption.clone())
            }
            MessageContent::Audio(audio) if !audio.caption.is_empty() => {
                Some(audio.caption.clone())
            }
            // M2: rich messages copy their plain-text block form.
            MessageContent::RichMessage(rich) => {
                let text = rich.copy_text();
                (!text.is_empty()).then_some(text)
            }
            _ => None,
        }
    }

    pub(in crate::ui) fn open_message_url(&mut self, url: &str, cx: &mut Context<Self>) {
        self.connection.status_note = if quill::platform::open_external_url(url) {
            "opened link".into()
        } else {
            "could not open link".into()
        };
        cx.notify();
    }

    /// MED4: open a link-preview card tap (TGX `TdlibUi` behavior).
    /// Embedded players open their embed URL (IV never applies); cards
    /// with `instant_view_version > 0` (schema:4570) open the IV reader
    /// in Telegram mode, and `All` mode attempts IV for any card link.
    /// Otherwise — and on TDLib's 404 — the browser opens. A refusal is
    /// never rendered as a reader.
    pub(in crate::ui) fn open_preview_url(
        &mut self,
        preview: &quill::telegram::envelope::LinkPreview,
        cx: &mut Context<Self>,
    ) {
        // Embedded players open their embed URL, not the page URL.
        if let quill::telegram::envelope::LinkPreviewKind::EmbeddedPlayer { url, .. } =
            &preview.kind
            && !url.is_empty()
        {
            self.open_message_url(url, cx);
            return;
        }
        let url = preview.url.clone();
        let mode = self
            .live
            .as_ref()
            .map(|live| live.driver.session.settings.media_prefs.instant_view_mode);
        let try_iv = matches!(mode, Some(quill::settings::InstantViewMode::All))
            || (preview.instant_view_version > 0
                && matches!(mode, Some(quill::settings::InstantViewMode::Telegram)));
        let mut requested = false;
        if try_iv && let Some(live) = self.live.as_mut() {
            match live.driver.open_instant_view(&url) {
                quill::connect::InstantViewOutcome::Requested => requested = true,
                quill::connect::InstantViewOutcome::Browser => {}
            }
        }
        if requested {
            self.connection.status_note = "loading Instant View…".into();
            cx.notify();
        } else {
            self.open_message_url(&url, cx);
        }
    }

    /// Phase 3.2: press an inline keyboard callback button. Live sessions
    /// send `getCallbackQueryAnswer`; the bot's `callbackQueryAnswer`
    /// response is picked up by `poll_live` and shown in the status line.
    /// Demo sessions have no live TDLib, so the press is an honest no-op.
    pub(in crate::ui) fn press_inline_callback(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        data: Vec<u8>,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.send_callback_query(chat_id, message_id, &data);
            self.connection.status_note = match result {
                Ok(_) => "sending…".into(),
                Err(_) => "could not send callback".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.connection.status_note = "demo — callback sent (no live Telegram)".into();
            cx.notify();
        }
    }

    /// Phase 3.2: insert a `switchInline` query into the current chat's
    /// composer. `targetChatChosen` / `targetChatInternalLink` (no chat
    /// picker in this slice) use the current chat, same as `targetChatCurrent`.
    ///
    /// Phase S2: in a secret chat the insertion is gated on the one-time
    /// inline-bot warning (TGX `SecretChatContextBotAlert`,
    /// `helper/InlineSearchContext.java:792-800`): a `SwitchInline` press is
    /// Quill's only inline-bot invocation point, so the alert fires here
    /// before the query text lands in the composer.
    pub(in crate::ui) fn insert_switch_inline_query(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.composer_ui.inline_bot_alert_shown && self.open_chat_is_secret() {
            self.composer_ui.pending_inline_bot_alert = Some(query.to_string());
            cx.notify();
            return;
        }
        // A stash left over from another chat (the open chat changed
        // since the gated press) never survives an ungated insert.
        self.composer_ui.pending_inline_bot_alert = None;
        let next = quill::composer::insert_switch_inline_text(&self.composer_markup(cx), query);
        self.set_composer_markup(&next, window, cx);
        self.sync_command_menu(cx);
    }

    /// Phase S2: confirm the secret-chat inline-bot warning (TGX's single
    /// `Confirm`, `ALERT_NO_CANCEL`) — insert the stashed `SwitchInline`
    /// query and don't ask again this session.
    pub(in crate::ui) fn confirm_inline_bot_alert(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The banner only renders in a secret chat; a stale stash must
        // never confirm into another chat's composer.
        if !self.open_chat_is_secret() {
            return;
        }
        let Some(query) = self.composer_ui.pending_inline_bot_alert.take() else {
            return;
        };
        self.composer_ui.inline_bot_alert_shown = true;
        if !query.is_empty() {
            let next =
                quill::composer::insert_switch_inline_text(&self.composer_markup(cx), &query);
            self.set_composer_markup(&next, window, cx);
        }
        self.sync_command_menu(cx);
        self.sync_inline_mode(cx);
        cx.notify();
    }

    pub(in crate::ui) fn jump_to_pinned_message(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.jump_to_chat_search_message(message_id)
            {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to pinned message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.connection.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }
}
