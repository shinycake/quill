//! Methods moved out of `message_actions.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::go_to_message::{GoToTarget, MessageListContext, go_to_message_offered, go_to_target};
use quill::lang::{keys, tr};

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
            .unwrap_or_else(|| tr(keys::Q_CHAT_CHAT).to_string());
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
                let message_id = MessageId(id as i64);
                let row = div()
                    .id(("chat-preview-row", id))
                    .debug_selector(|| "chat-preview-row".into())
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child(name),
                    )
                    .child(div().text_sm().child(text));
                // Telegram Desktop's "Go To Message" on a peeked message
                // (`Context::ChatPreview`).
                let row =
                    if go_to_message_offered(MessageListContext::ChatPreview, message_id, false) {
                        let owner = cx.entity().downgrade();
                        row.context_menu(move |menu, _, _| {
                            let owner = owner.clone();
                            menu.item(
                                gpui_kit::component::menu::PopupMenuItem::new(tr(
                                    keys::Q_CHAT_GO_TO_MESSAGE,
                                ))
                                .icon(gpui_kit::assets::IconName::MessageSquare)
                                .on_click(move |_, window, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        this.go_to_message_in(chat_id, message_id, window, cx);
                                    });
                                }),
                            )
                        })
                        .into_any_element()
                    } else {
                        row.into_any_element()
                    };
                body = body.child(row);
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
                    .child(tr(keys::Q_CHAT_NO_MESSAGES_YET_2)),
            );
        }
        // The peek shows the chat's messages: the UI driver cuts it
        // wherever it cuts that chat's row (a secret chat with All chats
        // on included).
        let panel = div()
            .id("chat-preview-panel")
            .debug_selector(|| "chat-preview-panel".into())
            .role(gpui_kit::Role::Group)
            .a11y_tag(quill::agent::ui_driver::redact::chat_tag(chat_id.0))
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
        if menu.top_person {
            return self.top_person_menu_overlay(menu, cx);
        }
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
        let archived = chat.in_archive;
        let can_clear = chat.can_be_deleted_only_for_self || chat.can_be_deleted_for_all_users;

        // Telegram Desktop's chat menu: left-aligned rows with an icon, in
        // its order (Archive, Pin, Mute, Mark as read/unread, Clear
        // history, Delete chat); Quill's extras slot in before Delete.
        let mut rows: Vec<(u8, AnyElement)> = Vec::new();
        let row_hover = cx.theme().accent;
        macro_rules! item {
            ($order:expr, $icon:expr, $id:expr, $label:expr, $this:ident, $cx:ident, $body:block) => {
                rows.push((
                    $order,
                    div()
                        .id($id)
                        .test_support()
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
                        .menu_key_target(row_key($order), &self.frame.context_menu_items, $cx)
                        .role(gpui_kit::Role::MenuItem)
                        .aria_label($label)
                        .child(Icon::new($icon).size(px(16.)))
                        .child($label)
                        .on_hover($cx.listener(|this, hovered, _, cx| {
                            this.chat_menu_row_hovered(*hovered, cx);
                        }))
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
                        .test_support()
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
                        .menu_key_target(row_key($order), &self.frame.context_menu_items, $cx)
                        .role(gpui_kit::Role::MenuItem)
                        .aria_label($label)
                        .child(Icon::new($icon).size(px(16.)))
                        .child($label)
                        .on_hover($cx.listener(|this, hovered, _, cx| {
                            this.chat_menu_row_hovered(*hovered, cx);
                        }))
                        .on_click($cx.listener(move |$this, _, $window, $cx| $body))
                        .into_any_element(),
                ));
            };
        }
        use gpui_kit::assets::IconName as Lucide;
        // `History::useTopPromotion`: a promoted chat has no Archive or Pin
        // row, and no new window for a typed announcement.
        let promotion = self
            .session()
            .and_then(|session| quill::promotion::top_promotion(session, &chat))
            .cloned();
        let hideable = promotion.as_ref().is_some_and(|p| p.can_hide());
        // tdesktop `Filler::addNewWindow` heads the chat-list menu.
        if self.can_open_chat_window(&chat) && !hideable {
            item_window!(
                5,
                Lucide::ExternalLink,
                "chat-menu-new-window",
                tr(keys::CONTEXT_NEW_WINDOW),
                this,
                window,
                cx,
                {
                    this.chat_list.menu = None;
                    this.open_chat_in_new_window(chat_id, window, cx);
                    cx.notify();
                }
            );
        }
        // tdesktop `Filler::addHidePromotion`: announcements only.
        if hideable {
            item!(
                6,
                Lucide::Trash,
                "chat-menu-hide-promotion",
                tr(keys::CONTEXT_HIDE_PSA),
                this,
                cx,
                {
                    this.chat_list.menu = None;
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.hide_promotion(chat_id);
                    }
                    cx.notify();
                }
            );
        }
        if promotion.is_none() {
            item!(
                10,
                if archived {
                    Lucide::ArchiveRestore
                } else {
                    Lucide::Archive
                },
                "chat-menu-archive",
                if archived {
                    tr(keys::Q_CHAT_UNARCHIVE)
                } else {
                    tr(keys::Q_CHAT_ARCHIVE)
                },
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
                if pinned {
                    tr(keys::Q_CHAT_UNPIN)
                } else {
                    tr(keys::CONTEXT_PIN_MSG)
                },
                this,
                cx,
                {
                    this.toggle_chat_pin(chat_id, cx);
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        if let Some(row) = self.chat_menu_mute_row(menu, &chat, row_hover, cx) {
            rows.push((30, row));
        }
        item!(
            40,
            if unread {
                Lucide::CircleCheck
            } else {
                Lucide::MessageSquareDot
            },
            "chat-menu-read",
            if unread {
                tr(keys::CONTEXT_MARK_READ)
            } else {
                tr(keys::Q_CHAT_MARK_AS_UNREAD)
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
                    if let Some(target) = this.session().and_then(|s| s.chat_info_target(chat_id)) {
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
                tr(keys::CONTEXT_MARK_READ_MENTIONS_ALL),
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
                tr(keys::Q_CHAT_READ_ALL_REACTIONS),
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
                tr(keys::CONTEXT_MARK_READ_POLL_VOTES_ALL),
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
            tr(keys::Q_CHAT_SELECT),
            this,
            cx,
            {
                this.enter_select_mode(chat_id, cx);
                this.chat_list.menu = None;
                cx.notify();
            }
        );
        // One item; the box asks whether to clear for the other side too.
        if can_clear {
            item_window!(
                60,
                Lucide::Eraser,
                "chat-menu-clear",
                tr(keys::Q_CHAT_CLEAR_HISTORY),
                this,
                window,
                cx,
                {
                    this.chat_list.menu = None;
                    this.open_clear_history_box(chat_id, window, cx);
                    cx.notify();
                }
            );
        }
        // Slice CL3: Report — gated on `chat.can_be_reported` (schema
        // 1.8.67, line 3606); opens the report flow with the reasons the
        // server offers (`reportChat` with no message ids, answered by a
        // `reportChatResult`).
        if chat.can_be_reported && self.live.is_some() {
            item!(
                70,
                Lucide::Flag,
                "chat-menu-report",
                tr(keys::Q_CHAT_REPORT),
                this,
                cx,
                {
                    this.chat_list.menu = None;
                    this.open_message_report(chat_id, Vec::new(), cx);
                    cx.notify();
                }
            );
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
                    tr(keys::Q_CHAT_UNBLOCK_USER)
                } else {
                    tr(keys::Q_CHAT_BLOCK_USER)
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
                tr(keys::PROFILE_EXPORT_CHAT),
                this,
                cx,
                {
                    this.start_chat_export(chat_id, cx);
                    this.chat_list.menu = None;
                    cx.notify();
                }
            );
        }
        if let Some(row) = self.chat_menu_exit_row(&chat, can_clear, row_hover, cx) {
            rows.push((90, row));
        }
        if let Some(row) = self.chat_menu_folder_row(menu, &chat, row_hover, cx) {
            rows.push((45, row));
        }
        rows.sort_by_key(|(order, _)| *order);
        self.chat_menu_shell(
            chat_id,
            menu.position,
            rows.into_iter().map(|(_, row)| row).collect(),
            cx,
        )
    }

    /// MED4: Instant View reader overlay (TGX behavior — attempt IV
    /// when the card offers it, fall back to the browser on 404 /
    /// unsupported). Drains `Session::instant_view_fallback_url` into
    /// the browser and renders `Session::instant_view` page blocks via
    /// `message_rich_block`. A refusal is never rendered as a reader. A
    /// fallback for a link an agent's step clicked opens as part of that
    /// step (`guard::with_injecting`): the browser stays closed.
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
            let messages = &mut live.driver.session.messages;
            let from_agent = std::mem::take(&mut messages.instant_view_fallback_from_agent);
            self.agent_step(from_agent, |this| this.open_message_url(&url, cx));
        }
        self.session()?.messages.instant_view.as_ref()?;
        self.instant_view_overlay_body(cx)
    }

    /// `instant_view_overlay` past its cheap check. A debug build reserves
    /// this builder's whole frame on entry, so frames that show nothing
    /// skip it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn instant_view_overlay_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let page = self.session()?.messages.instant_view.clone()?;
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
                        .child(
                            div()
                                .text_sm()
                                .font_medium()
                                .child(tr(keys::Q_CHAT_INSTANT_VIEW)),
                        )
                        .child(
                            Button::new("instant-view-close")
                                .icon(gpui_kit::assets::IconName::X)
                                .tooltip(tr(keys::CLOSE))
                                .accessibility_label(tr(keys::CLOSE))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(live) = this.live.as_mut() {
                                        live.driver.session.messages.instant_view = None;
                                    } else if let Some(demo) = this.demo_session.as_mut() {
                                        demo.messages.instant_view = None;
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
        let open = quill::platform::open_external_url(url);
        self.when_os_answers(open, cx, |this, opened, cx| {
            this.connection.set_note(if opened {
                StatusNote::info(tr(keys::Q_CHAT_OPENED_LINK))
            } else {
                StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_OPEN_LINK))
            });
            cx.notify();
        });
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
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHAT_LOADING_INSTANT_VIEW)));
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
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_CHAT_SENDING)),
                Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_SEND_CALLBACK)),
            });
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHAT_DEMO_CALLBACK_SENT_NO_LIVE,
            )));
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

    /// Open `chat_id` if it is not showing, then jump to and highlight the
    /// message (the peek preview's "Go To Message").
    pub(in crate::ui) fn go_to_message_in(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_chat_preview(cx);
        let open = self.session().and_then(|session| session.open_chat);
        match go_to_target(open, chat_id, message_id) {
            GoToTarget::OtherChat {
                chat_id,
                message_id,
            } => {
                // The chat opens at the message: its first history load is
                // the window around it.
                self.chat_list.open_at = Some(message_id);
                self.select_listed_chat(chat_id, window, cx);
                self.chat_list.open_at = None;
                if let Some(live) = self.live.as_ref() {
                    self.connection
                        .set_note(chat_search_jump_note(&live.driver.session));
                } else if let Some(session) = self.demo_session.as_ref() {
                    self.connection.set_note(chat_search_jump_note(session));
                }
            }
            GoToTarget::SameChat { message_id } => self.jump_to_pinned_message(message_id, cx),
        }
    }

    pub(in crate::ui) fn jump_to_pinned_message(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.jump_to_chat_search_message(message_id) {
                    Ok(_) => chat_search_jump_note(&live.driver.session),
                    Err(_) => StatusNote::error(tr(keys::Q_CHAT_COULD_NOT_JUMP_TO_PINNED)),
                });
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.connection.set_note(chat_search_jump_note(session));
        }
        cx.notify();
    }
}
