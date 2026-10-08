//! message menu overlay, copy/open URLs, pin banner, delete confirm.

use super::app::QuillApp;
use super::chat_row::{ChatPreviewState, chat_preview_line};
use super::message_menu_ui::{
    MenuRow, MessageMenuPage, info_row, menu_row, menu_separator, stack_rows,
};
use super::message_text::message_rich_block;
use super::search_ui::chat_search_jump_note;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::DeleteConfirm;
use quill::connect::PREVIEW_HISTORY_LIMIT;
use quill::diagnostics::DiagnosticSink;
use quill::ids::{ChatId, MessageId};
use quill::message_menu::order;
use quill::poll::can_stop_poll;
use quill::state::{RequestPurpose, effective_preview};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    ChatKind, MUTE_FOREVER, MessageContent, PollType, effective_content,
};
use std::sync::Arc;
use std::time::Instant;
impl QuillApp {
    /// Telegram Desktop's refusal for copying out of a protected chat;
    /// true when the chat is protected (and the note was shown).
    pub(super) fn refuse_protected_copy(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        if !session.chat_has_protected_content(chat_id) {
            return false;
        }
        let kind = session.chats.get(&chat_id.0).map(|chat| chat.kind.clone());
        self.status_note = match kind {
            Some(ChatKind::Supergroup {
                is_channel: true, ..
            }) => "Sorry, copying from this channel is disabled by admins.",
            Some(ChatKind::Private { .. } | ChatKind::Secret { .. }) | None => {
                "Sorry, copying from this chat is restricted."
            }
            _ => "Sorry, copying from this group is disabled by admins.",
        }
        .into();
        cx.notify();
        true
    }

    /// M1: right-click message context menu — Reply, Copy, Forward, Pin,
    /// Share link, Retry (failed sends), Delete. Rendered absolute at the
    /// click position; any click on the backdrop closes it.
    pub(super) fn message_menu_overlay(
        &self,
        menu: MessageMenuState,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (chat_id, message_id) = (menu.chat_id, menu.message_id);
        let message = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .and_then(|history| history.messages.get(&message_id.0))
            .cloned();
        let Some(message) = message else {
            return div().into_any_element();
        };
        let failed = message.failed;
        let copyable = Self::message_copyable_text(effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        ));
        let delete_confirm =
            DeleteConfirm::for_message(chat_id, message_id, message.is_outgoing, message.pending);
        let pinned = message.is_pinned;
        // What TDLib says this message allows (`messageProperties`), as
        // Telegram Desktop gates its menu. Until it arrives (a local call,
        // milliseconds), channel posts offer only what any reader can do.
        let actions = self
            .session()
            .and_then(|s| s.message_menu_actions)
            .filter(|(c, m, _)| *c == chat_id && *m == message_id)
            .map(|(_, _, actions)| actions);
        let chat_kind = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|chat| chat.kind.clone());
        let is_channel_post = matches!(
            chat_kind,
            Some(ChatKind::Supergroup {
                is_channel: true,
                ..
            })
        );
        let is_shared_chat = matches!(chat_kind, Some(ChatKind::Supergroup { .. }));
        let allows = |local: bool, flag: fn(&quill::telegram::envelope::MessageActions) -> bool| {
            actions.map_or(local, |a| flag(&a))
        };
        let can_reply = allows(!is_channel_post, |a| a.can_be_replied);

        let mut panel = div()
            .id("message-menu-panel")
            // Clicks inside the panel (the reaction strip's plain cells)
            // must not reach the backdrop, which closes the menu.
            .occlude()
            .flex()
            .flex_col()
            .min_w(px(180.))
            .px_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas());
        // Reactions lead the menu (Telegram Desktop): the chat's quick
        // strip in its own pill above the menu, expandable to every
        // reaction it allows.
        let page = self.message_menu_ui.page;
        let strip = (message.can_react() && page == MessageMenuPage::Main)
            .then(|| self.reaction_strip(chat_id, message_id, &message, cx));
        // Telegram Desktop's message menu: left-aligned rows with an icon,
        // in its order (Reply, Edit, Pin, Copy Text, Copy Link, Forward,
        // Delete, Select). Items are collected with their position, then
        // sorted into the panel.
        let mut rows: Vec<(u8, AnyElement)> = Vec::new();
        let row_hover = cx.theme().accent;
        macro_rules! item {
            ($order:expr, $icon:expr, $id:expr, $label:expr, $this:ident, $window:ident, $cx:ident, $body:block) => {
                let danger = $id == "menu-delete";
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
                        .on_click($cx.listener(move |$this, _, $window, $cx| $body))
                        .into_any_element(),
                ));
            };
        }
        // Over a text selection, Reply quotes it and Copy copies it
        // (Telegram Desktop: "Quote & Reply", "Copy Selected Text").
        let selection = self.message_menu_selection.clone();
        if can_reply {
            let quote = selection.clone();
            item!(
                order::REPLY,
                gpui_kit::assets::IconName::Reply,
                "menu-reply",
                if quote.is_some() {
                    "Quote & Reply"
                } else {
                    "Reply"
                },
                this,
                window,
                cx,
                {
                    match quote.as_deref() {
                        Some(quote) => {
                            this.begin_quote_reply(chat_id, message_id, quote, window, cx)
                        }
                        None => this.begin_reply_from_message(chat_id, message_id, window, cx),
                    }
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        let copy = match selection {
            Some(selected) => Some(("Copy Selected Text", selected)),
            None => copyable.map(|text| ("Copy Text", text)),
        };
        // Protected chats allow neither copying nor forwarding, before
        // TDLib's per-message answer arrives too.
        let protected = self
            .session()
            .is_some_and(|session| session.chat_has_protected_content(chat_id));
        if let Some((label, text)) =
            copy.filter(|_| !protected && allows(true, |a| a.can_be_copied))
        {
            item!(
                // Copy Selected Text leads the menu; Copy Text follows the
                // media actions (`FillContextMenuItems`).
                if label == "Copy Text" {
                    order::COPY_TEXT
                } else {
                    order::COPY_SELECTED
                },
                gpui_kit::assets::IconName::Copy,
                "menu-copy",
                label,
                this,
                _window,
                cx,
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        // Translate Selected Text over a selection, Translate for the
        // message's own text (Telegram Desktop: both can show; the second
        // only while the chat is not already shown translated).
        if let Some(selected) = self
            .message_menu_selection
            .clone()
            .filter(|selected| self.translate_menu_offered(chat_id, selected))
        {
            item!(
                order::TRANSLATE_SELECTED,
                gpui_kit::assets::IconName::Languages,
                "menu-translate-selected",
                "Translate Selected Text",
                this,
                window,
                cx,
                {
                    this.message_menu = None;
                    this.open_translate_selection(chat_id, selected.clone(), window, cx);
                }
            );
        }
        let already_translated = self.session().is_some_and(|s| {
            s.chat_translated_to(chat_id).is_some_and(|to| {
                matches!(
                    s.message_translation(chat_id, message_id, to),
                    Some(quill::state::Translation::Done { .. })
                )
            })
        });
        if !already_translated
            && message_id.0 > 0
            && quill::translate::translatable_content(effective_content(
                &message.content,
                message.ephemeral.as_ref(),
            ))
            .is_some_and(|(text, _)| self.translate_menu_offered(chat_id, text))
        {
            item!(
                order::TRANSLATE,
                gpui_kit::assets::IconName::Languages,
                "menu-translate",
                "Translate",
                this,
                window,
                cx,
                {
                    this.message_menu = None;
                    this.open_translate_message(chat_id, message_id, window, cx);
                }
            );
        }
        // Over a link, the menu leads with what Telegram Desktop adds for it:
        // a copy entry named for the kind of link (`copyToClipboardContextItemText`),
        // and, for web links, Open.
        if let Some(link) = self.message_menu_link.clone() {
            let msg_key = (chat_id.0, message_id.0 as u64);
            if matches!(link, quill::text::LinkTarget::Url { .. }) {
                let open = link.clone();
                item!(
                    4,
                    gpui_kit::assets::IconName::ExternalLink,
                    "menu-open-link",
                    "Open Link",
                    this,
                    _window,
                    cx,
                    {
                        this.message_menu = None;
                        this.queue_link(open.clone(), msg_key, cx);
                    }
                );
            }
            if let (Some(label), Some(text)) = (link.copy_label(), link.copy_text())
                && !protected
            {
                let text = text.to_string();
                item!(
                    41,
                    gpui_kit::assets::IconName::Copy,
                    "menu-copy-link",
                    label,
                    this,
                    _window,
                    cx,
                    {
                        this.message_menu = None;
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        cx.notify();
                    }
                );
            }
        }
        let is_secret = matches!(chat_kind, Some(ChatKind::Secret { .. }));
        if !is_secret
            && !protected
            && allows(true, |a| a.can_be_forwarded)
            && quill::composer::ForwardDraft::from_message(chat_id, message_id, message.pending)
                .is_some()
        {
            item!(
                50,
                gpui_kit::assets::IconName::Forward,
                "menu-forward",
                "Forward",
                this,
                window,
                cx,
                {
                    this.begin_forward_one(chat_id, message_id, message.pending, window, cx);
                    this.message_menu = None;
                    cx.notify();
                }
            );
            item!(
                70,
                gpui_kit::assets::IconName::CircleCheck,
                "menu-select",
                "Select",
                this,
                _window,
                cx,
                {
                    this.toggle_forward_select(chat_id, message_id, message.pending, cx);
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        if let Some(edit) = quill::composer::ComposerEdit::from_own_content(
            chat_id,
            message_id,
            message.is_outgoing,
            message.pending,
            &message.content,
        )
        .filter(|_| allows(true, |a| a.can_be_edited))
        {
            item!(
                20,
                gpui_kit::assets::IconName::Pencil,
                "menu-edit",
                "Edit",
                this,
                window,
                cx,
                {
                    this.begin_edit(edit.clone(), window, cx);
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        if allows(message.can_pin() && !is_channel_post, |a| a.can_be_pinned) {
            let label = if pinned { "Unpin" } else { "Pin" };
            item!(
                30,
                if pinned {
                    gpui_kit::assets::IconName::PinOff
                } else {
                    gpui_kit::assets::IconName::Pin
                },
                "menu-toggle-pin",
                label,
                this,
                _window,
                cx,
                {
                    this.toggle_pin_message(chat_id, message_id, cx);
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        if allows(is_shared_chat, |a| a.can_get_link) {
            item!(
                order::COPY_POST_LINK,
                gpui_kit::assets::IconName::Link,
                "menu-share",
                quill::message_menu::copy_link_label(is_channel_post),
                this,
                _window,
                cx,
                {
                    this.share_message_link(chat_id, message_id, cx);
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        // B4: stopPoll (schema 1.8.67 line 12953) — TGX `StopPollWarn`
        // shows Stop Poll / Stop Quiz only for open polls with
        // `messageProperties.can_be_edited` (schema line 12951); client
        // side we offer it on own polls (`quill::poll::can_stop_poll`).
        if let MessageContent::Poll(poll_content) = &message.content
            && can_stop_poll(message.is_outgoing, &poll_content.poll)
        {
            let is_quiz = matches!(poll_content.poll.poll_type, PollType::Quiz { .. });
            let label = if is_quiz { "Stop Quiz" } else { "Stop Poll" };
            item!(
                55,
                gpui_kit::assets::IconName::CircleStop,
                "menu-stop-poll",
                label,
                this,
                _window,
                cx,
                {
                    this.begin_stop_poll(chat_id, message_id, is_quiz, cx);
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        // Slice G2: channel-post comment threads (`getMessageThreadHistory`,
        // schema 1.8.67, line 11839). The dialog shows an honest error
        // when the post has no discussion thread.
        if is_channel_post && allows(false, |a| a.can_get_message_thread) {
            item!(
                15,
                gpui_kit::assets::IconName::MessageSquare,
                "menu-comments",
                "View Comments",
                this,
                window,
                cx,
                {
                    this.open_comment_thread_dialog(chat_id, message_id, window, cx);
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        if failed && message.can_retry {
            item!(
                58,
                gpui_kit::assets::IconName::RotateCw,
                "menu-retry",
                "Resend",
                this,
                _window,
                cx,
                {
                    this.retry_failed_message(chat_id, message_id, cx);
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        // Telegram Desktop offers "delete for everyone" exactly when TDLib
        // allows it (`can_be_deleted_for_all_users`), never in Saved
        // Messages; until the properties arrive, for your own messages.
        let saved = self.session().is_some_and(|s| s.is_saved_messages(chat_id));
        let can_revoke = !saved && allows(message.is_outgoing, |a| a.can_be_deleted_for_all_users);
        let delete_confirm = delete_confirm.map(|mut confirm| {
            confirm.can_revoke = can_revoke;
            confirm.revoke = can_revoke;
            confirm
        });
        let moderation = self.moderation_offer(chat_id, &message, actions);
        if let Some(confirm) =
            delete_confirm.filter(|_| allows(!is_channel_post, |a| a.can_be_deleted()))
        {
            item!(
                60,
                gpui_kit::assets::IconName::Trash,
                "menu-delete",
                "Delete",
                this,
                window,
                cx,
                {
                    this.open_delete_dialog_with(confirm.clone(), moderation.clone(), window, cx);
                    this.message_menu = None;
                    cx.notify();
                }
            );
        }
        // Cancel Upload takes Delete's place on a message still being sent.
        if message.pending
            && quill::message_menu::media_target(effective_content(
                &message.content,
                message.ephemeral.as_ref(),
            ))
            .is_some()
        {
            let cancel = menu_row(
                order::DELETE,
                gpui_kit::assets::IconName::X,
                "menu-cancel-upload",
                "Cancel Upload",
                true,
                cx,
                move |this, _, cx| {
                    this.cancel_message_upload(chat_id, message_id, cx);
                    this.message_menu = None;
                    cx.notify();
                },
            );
            rows.push(cancel);
        }
        // Telegram Desktop's media block, Report, and the audience rows.
        rows.extend(self.menu_media_rows(chat_id, &message, actions, cx));
        let reportable = !message.pending
            && message.id.0 > 0
            && !message.is_outgoing
            && !saved
            && actions.map_or(true, |a| a.can_report_chat);
        if reportable {
            let report = menu_row(
                order::REPORT,
                gpui_kit::assets::IconName::Flag,
                "menu-report",
                "Report",
                false,
                cx,
                move |this, _, cx| {
                    // A grouped album reports every photo in it.
                    this.open_message_report(chat_id, vec![message_id], cx);
                },
            );
            rows.push(report);
        }
        let audience_rows = self.menu_audience_rows(chat_id, &message, cx);
        if !audience_rows.is_empty() {
            rows.push(menu_separator(order::AUDIENCE - 1));
            rows.extend(audience_rows);
        }
        // Nothing to copy or forward: say why (`AddSelectRestrictionAction`).
        if protected && !message.pending {
            let is_group = matches!(
                chat_kind,
                Some(ChatKind::Supergroup { .. } | ChatKind::BasicGroup { .. })
            );
            rows.push(info_row(
                order::SELECT,
                "menu-noforwards",
                None,
                quill::message_menu::noforwards_info(
                    is_channel_post,
                    is_group && !is_channel_post,
                    false,
                    message.is_outgoing,
                ),
            ));
        }
        let page_rows: Vec<MenuRow> = match page {
            MessageMenuPage::Main => rows,
            sub => {
                let mut sub_rows = vec![menu_row(
                    0,
                    gpui_kit::assets::IconName::ChevronLeft,
                    "menu-back",
                    match sub {
                        MessageMenuPage::SaveTo => "Save to...",
                        _ => "Back",
                    },
                    false,
                    cx,
                    |this, _, cx| {
                        this.message_menu_ui.page = MessageMenuPage::Main;
                        cx.notify();
                    },
                )];
                sub_rows.extend(match sub {
                    MessageMenuPage::SaveTo => self.menu_save_to_rows(chat_id, message_id, cx),
                    _ => self.menu_audience_page(chat_id, &message, cx),
                });
                sub_rows
            }
        };
        let stacked = stack_rows(page_rows);
        if page == MessageMenuPage::Audience {
            panel = panel.w(px(280.));
            panel = panel.child(
                div()
                    .id("menu-audience-scroll")
                    .flex()
                    .flex_col()
                    .max_h(px(360.))
                    .overflow_y_scroll()
                    .children(stacked),
            );
        } else {
            panel = panel.children(stacked);
        }
        div()
            .id("message-menu-overlay")
            .track_focus(&self.context_menu_focus)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .child(
                div()
                    .id("message-menu-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.message_menu = None;
                        cx.notify();
                    })),
            )
            .child(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap_1()
                            .when_some(strip, |this, strip| {
                                this.child(
                                    div()
                                        .id("message-menu-reactions")
                                        .occlude()
                                        .rounded_xl()
                                        .border_1()
                                        .border_color(accent())
                                        .bg(bg_canvas())
                                        .shadow_md()
                                        .child(strip),
                                )
                            })
                            .child(panel),
                    ),
            )
            .focus_trap("message-menu-focus", &self.context_menu_focus)
            .into_any_element()
    }

    /// Slice CL: start a long press on a chat-list row — after
    /// `CHAT_PREVIEW_LONG_PRESS` without a release the read-only peek
    /// preview opens; a quicker release stays a plain click.
    pub(super) fn begin_chat_preview_press(
        &mut self,
        chat_id: ChatId,
        anchor: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let started = Instant::now();
        self.preview_press = Some((chat_id, started));
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Self::CHAT_PREVIEW_LONG_PRESS)
                .await;
            let _ = this.update(cx, |this, cx| {
                // Still the same press (no release, no newer press)?
                if this.preview_press == Some((chat_id, started)) {
                    this.open_chat_preview(chat_id, anchor, cx);
                }
            });
        })
        .detach();
    }

    /// Slice CL: open the read-only peek preview for a chat — recent
    /// messages in a floating panel beside the row. Never opens the
    /// chat and never marks anything read.
    pub(super) fn open_chat_preview(
        &mut self,
        chat_id: ChatId,
        anchor: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.chat_menu = None;
        self.chat_preview = Some(ChatPreviewState { chat_id, anchor });
        // Live fetch for unopened chats; already-loaded history renders
        // immediately. Demo/no-driver sessions skip the network.
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_chat_preview_history(chat_id);
        }
        cx.notify();
    }

    /// Slice CL: dismiss the peek preview (click anywhere, the release
    /// that ends the long press, or Escape). Also cancels a pending
    /// long press.
    pub(super) fn close_chat_preview(&mut self, cx: &mut Context<Self>) {
        if self.chat_preview.is_some() || self.preview_press.is_some() {
            self.chat_preview = None;
            self.preview_press = None;
            cx.notify();
        }
    }

    /// Slice CL: the floating peek preview — chat title + the most
    /// recent messages as read-only sender/body rows. Rendered absolute
    /// beside the pressed row (same pattern as `chat_menu_overlay`); any
    /// click on the catcher closes it.
    pub(super) fn chat_preview_overlay(
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
    pub(super) fn chat_menu_overlay(
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
                this.chat_menu = None;
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
                this.chat_menu = None;
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
                this.chat_menu = None;
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
                this.chat_menu = None;
                cx.notify();
            }
        );
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
                this.chat_menu = None;
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
                    this.chat_menu = None;
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
                    this.chat_menu = None;
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
                this.chat_menu = None;
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
                    this.chat_menu = None;
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
                    this.chat_menu = None;
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
            .track_focus(&self.context_menu_focus)
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
                        this.chat_menu = None;
                        cx.notify();
                    })),
            )
            .child(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(panel),
            )
            .focus_trap("chat-menu-focus", &self.context_menu_focus)
            .into_any_element()
    }

    /// MED4: Instant View reader overlay (TGX behavior — attempt IV
    /// when the card offers it, fall back to the browser on 404 /
    /// unsupported). Drains `Session::instant_view_fallback_url` into
    /// the browser and renders `Session::instant_view` page blocks via
    /// `message_rich_block`. A refusal is never rendered as a reader.
    pub(super) fn instant_view_overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(live) = self.live.as_mut()
            && let Some(url) = live.driver.session.instant_view_fallback_url.take()
        {
            self.open_message_url(&url, cx);
        }
        let page = self.live.as_ref()?.driver.session.instant_view.clone()?;
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
                                        live.driver.session.instant_view = None;
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
    pub(super) fn message_copyable_text(content: &MessageContent) -> Option<String> {
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

    pub(super) fn open_message_url(&mut self, url: &str, cx: &mut Context<Self>) {
        self.status_note = if quill::platform::open_external_url(url) {
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
    pub(super) fn open_preview_url(
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
            .map(|live| live.driver.session.media_prefs.instant_view_mode);
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
            self.status_note = "loading Instant View…".into();
            cx.notify();
        } else {
            self.open_message_url(&url, cx);
        }
    }

    /// Phase 3.2: press an inline keyboard callback button. Live sessions
    /// send `getCallbackQueryAnswer`; the bot's `callbackQueryAnswer`
    /// response is picked up by `poll_live` and shown in the status line.
    /// Demo sessions have no live TDLib, so the press is an honest no-op.
    pub(super) fn press_inline_callback(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        data: Vec<u8>,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.send_callback_query(chat_id, message_id, &data);
            self.status_note = match result {
                Ok(_) => "sending…".into(),
                Err(_) => "could not send callback".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.status_note = "demo — callback sent (no live Telegram)".into();
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
    pub(super) fn insert_switch_inline_query(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.inline_bot_alert_shown && self.open_chat_is_secret() {
            self.pending_inline_bot_alert = Some(query.to_string());
            cx.notify();
            return;
        }
        // A stash left over from another chat (the open chat changed
        // since the gated press) never survives an ungated insert.
        self.pending_inline_bot_alert = None;
        self.composer.update(cx, |input, cx| {
            let next = quill::composer::insert_switch_inline_text(&input.value(), query);
            input.set_value(next, window, cx);
        });
        self.sync_command_menu(cx);
    }

    /// Phase S2: confirm the secret-chat inline-bot warning (TGX's single
    /// `Confirm`, `ALERT_NO_CANCEL`) — insert the stashed `SwitchInline`
    /// query and don't ask again this session.
    pub(super) fn confirm_inline_bot_alert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The banner only renders in a secret chat; a stale stash must
        // never confirm into another chat's composer.
        if !self.open_chat_is_secret() {
            return;
        }
        let Some(query) = self.pending_inline_bot_alert.take() else {
            return;
        };
        self.inline_bot_alert_shown = true;
        if !query.is_empty() {
            self.composer.update(cx, |input, cx| {
                let next = quill::composer::insert_switch_inline_text(&input.value(), &query);
                input.set_value(next, window, cx);
            });
        }
        self.sync_command_menu(cx);
        self.sync_inline_mode(cx);
        cx.notify();
    }

    pub(super) fn toggle_pin_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .toggle_pin_chat_message(chat_id, message_id);
            self.status_note = match result {
                Ok(_) => "updating pin…".into(),
                Err(_) => "could not update pin".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_pin_toggle(chat_id, message_id);
            self.status_note = "pin updated".into();
            cx.notify();
        }
    }

    pub(super) fn jump_to_pinned_message(&mut self, message_id: MessageId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.jump_to_chat_search_message(message_id) {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to pinned message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    pub(super) fn unpin_from_banner(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .unpin_chat_message(chat_id, message_id);
            self.status_note = match result {
                Ok(_) => "unpinning…".into(),
                Err(_) => "could not unpin".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            let json = format!(
                r#"{{"@type":"updateMessageIsPinned","chat_id":{},"message_id":{},"is_pinned":false}}"#,
                chat_id.0, message_id.0
            );
            if let Some(session) = self.demo_session.as_mut() {
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_sink.clone();
                if let Some(owned) = copy_and_parse(&json, &self.demo_seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            self.status_note = "unpinned".into();
            cx.notify();
        }
    }

    /// Telegram Desktop's pinned bar: the pinned message at the bar's
    /// position, titled "Pinned message", "Previous message" or
    /// "Pinned message #N"; a segment per pinned message on the left. A
    /// click jumps to it and steps to the next older one. The right
    /// button unpins (or hides) a lone pin, or lists several.
    pub(super) fn pinned_message_banner(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let list = session.pinned_list(chat_id);
        let newest = list.first()?.id;
        if self.hidden_pinned.get(&chat_id.0) == Some(&newest) {
            return None;
        }
        let count = list.len();
        let index = self
            .pinned_cursor
            .get(&chat_id.0)
            .copied()
            .unwrap_or(0)
            .min(count - 1);
        let message_id = list[index].id;
        let preview = effective_preview(list[index]);
        let title = if index == 0 {
            "Pinned message".to_string()
        } else if count == 2 {
            "Previous message".to_string()
        } else {
            format!("Pinned message #{}", count - index)
        };
        let can_pin = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_pin_messages());
        // Up to four segments, oldest on top; the window follows the
        // shown message.
        let shown = count.min(4);
        let position = count - 1 - index;
        let start = position.saturating_sub(shown - 1).min(count - shown);
        let segments = (0..shown).map(|segment| {
            div()
                .flex_1()
                .w(px(2.))
                .rounded_full()
                .bg(if start + segment == position {
                    accent()
                } else {
                    accent().opacity(0.35)
                })
        });
        let right = if count == 1 {
            Button::new("pinned-bar-close")
                .icon(gpui_kit::assets::IconName::X)
                .ghost()
                .small()
                .tooltip(if can_pin { "Unpin" } else { "Hide" })
                .accessibility_label(if can_pin {
                    "Unpin message"
                } else {
                    "Hide pinned message"
                })
                .on_click(cx.listener(move |_, _, window, cx| {
                    let app = cx.entity().downgrade();
                    if can_pin {
                        confirm(
                            window,
                            cx,
                            "Would you like to unpin this message?",
                            "Unpin",
                            move |cx| {
                                let _ = app.update(cx, |this, cx| {
                                    this.unpin_from_banner(chat_id, message_id, cx);
                                });
                            },
                        );
                    } else {
                        confirm_hide_pinned(window, cx, app, chat_id, newest);
                    }
                }))
        } else {
            Button::new("pinned-bar-list")
                .icon(gpui_kit::assets::IconName::List)
                .ghost()
                .small()
                .selected(self.pinned_list_open)
                .tooltip("Pinned messages")
                .accessibility_label("Pinned messages")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pinned_list_open = !this.pinned_list_open;
                    cx.notify();
                }))
        };
        Some(
            div()
                .id("pinned-message-bar")
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1p5()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(bg_canvas())
                .child(
                    div()
                        .id("pinned-message-jump")
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .flex_1()
                        .role(gpui_kit::Role::Button)
                        .aria_label("Go to pinned message")
                        .tab_index(0)
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.jump_to_pinned_message(message_id, cx);
                            this.pinned_cursor.insert(chat_id.0, (index + 1) % count);
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .h(px(34.))
                                .children(segments),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(accent())
                                        .child(title),
                                )
                                .child(
                                    div().text_sm().truncate().text_color(text_primary()).child(
                                        super::bidi_line::one_line_plain(
                                            super::search_ui::one_line_preview(&preview),
                                        ),
                                    ),
                                ),
                        ),
                )
                .child(right)
                .into_any_element(),
        )
    }

    /// The pinned bar's list (Telegram Desktop's pinned-messages section,
    /// as a panel): every pinned message, newest first; a click jumps
    /// there. The footer unpins all, or for readers hides the bar.
    pub(super) fn pinned_list_panel(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let list = session.pinned_list(chat_id);
        let newest = list.first()?.id;
        let count = list.len();
        let can_pin = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_pin_messages());
        let hover = cx.theme().accent;
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        let rows = list.iter().enumerate().map(|(index, message)| {
            let message_id = message.id;
            div()
                .id(("pinned-list-row", message_id.0 as u64))
                .flex()
                .flex_col()
                .px_3()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .hover(|style| style.bg(hover))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(quill::local_time::day_label(
                            &quill::local_time::civil_local(i64::from(message.date)),
                            &now,
                        )),
                )
                .child(div().text_sm().truncate().text_color(text_primary()).child(
                    super::bidi_line::one_line_plain(super::search_ui::one_line_preview(
                        &effective_preview(message),
                    )),
                ))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.jump_to_pinned_message(message_id, cx);
                    this.pinned_cursor.insert(chat_id.0, index);
                    this.pinned_list_open = false;
                    cx.notify();
                }))
        });
        let noun = if count == 1 { "message" } else { "messages" };
        let footer = if can_pin {
            Button::new("pinned-list-unpin-all")
                .label(if count == 1 {
                    "Unpin 1 message".to_string()
                } else {
                    format!("Unpin all {count} messages")
                })
                .ghost()
                .on_click(cx.listener(move |_, _, window, cx| {
                    let app = cx.entity().downgrade();
                    confirm(
                        window,
                        cx,
                        "Do you want to unpin all messages?",
                        "Unpin",
                        move |cx| {
                            let _ = app.update(cx, |this, cx| {
                                this.pinned_list_open = false;
                                this.unpin_all_messages(chat_id, cx);
                            });
                        },
                    );
                }))
        } else {
            Button::new("pinned-list-hide")
                .label("Don't show pinned messages")
                .ghost()
                .on_click(cx.listener(move |_, _, window, cx| {
                    let app = cx.entity().downgrade();
                    confirm_hide_pinned(window, cx, app, chat_id, newest);
                }))
        };
        Some(
            div()
                .id("pinned-list-panel")
                .flex()
                .flex_col()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(bg_canvas())
                .child(
                    div()
                        .px_3()
                        .pt_2()
                        .text_xs()
                        .font_medium()
                        .text_color(accent())
                        .child(format!("{count} pinned {noun}")),
                )
                .child(
                    div()
                        .id("pinned-list-rows")
                        .flex()
                        .flex_col()
                        .px_1()
                        .py_1()
                        .max_h(px(280.))
                        .overflow_y_scroll()
                        .children(rows),
                )
                .child(div().flex().justify_center().pb_1().child(footer))
                .into_any_element(),
        )
    }

    pub(super) fn jump_to_replied_message(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.jump_to_replied_message(message_id) {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not jump to message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let _ = session.begin_chat_search_jump(message_id);
            self.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }
}

/// A confirmation in Telegram Desktop's style: the question, Cancel and
/// a primary action that runs `on_confirm`.
fn confirm(
    window: &mut Window,
    cx: &mut App,
    question: &'static str,
    action: &'static str,
    on_confirm: impl Fn(&mut App) + 'static,
) {
    let on_confirm = std::rc::Rc::new(on_confirm);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let on_confirm = on_confirm.clone();
        alert
            .description(question)
            .ok_text(action)
            .cancel_text("Cancel")
            .show_cancel(true)
            .on_ok(move |_, _, cx| {
                on_confirm(cx);
                true
            })
    });
}

/// Hide the pinned bar until a newer message is pinned (Telegram
/// Desktop's "Don't show pinned messages" for readers).
fn confirm_hide_pinned(
    window: &mut Window,
    cx: &mut App,
    app: WeakEntity<QuillApp>,
    chat_id: ChatId,
    newest: MessageId,
) {
    confirm(
        window,
        cx,
        "Do you want to hide the pinned message bar? It will stay hidden until a new message is pinned.",
        "Hide",
        move |cx| {
            let _ = app.update(cx, |this, cx| {
                this.hidden_pinned.insert(chat_id.0, newest);
                this.pinned_list_open = false;
                cx.notify();
            });
        },
    );
}
