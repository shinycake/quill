//! Methods moved out of `message_actions.rs` to keep files under 1000 lines.

use super::*;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// M1: right-click message context menu — Reply, Copy, Forward, Pin,
    /// Share link, Retry (failed sends), Delete. Rendered absolute at the
    /// click position; any click on the backdrop closes it.
    pub(in crate::ui) fn message_menu_overlay(
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
            .and_then(|s| s.messages.message_menu_actions)
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
            .role(gpui_kit::Role::Group)
            .a11y_tag(quill::agent::ui_driver::redact::REGION_MENU)
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
        let page = self.message_ui.menu_ui.page;
        let strip = (message.can_react() && page == MessageMenuPage::Main)
            .then(|| self.reaction_strip(chat_id, message_id, &message, cx))
            .flatten();
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
                        .test_support()
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
                        .menu_key_target(row_key($order), &self.frame.context_menu_items, $cx)
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
        let selection = self.message_ui.menu_selection.clone();
        let content = effective_content(&message.content, message.ephemeral.as_ref());
        // Over a checklist task, Reply becomes "Reply to Task".
        if can_reply && !self.menu_replies_to_task(content) {
            let quote = selection.clone();
            item!(
                order::REPLY,
                gpui_kit::assets::IconName::Reply,
                "menu-reply",
                if quote.is_some() {
                    tr(keys::Q_CHAT_QUOTE_REPLY)
                } else {
                    tr(keys::Q_CHAT_REPLY)
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
                    this.message_ui.menu = None;
                    cx.notify();
                }
            );
        }
        // A playing voice message also offers a reply stamped with where the
        // player is (Telegram Desktop `AddTimecodeAction`).
        let playing_here = self.playback.playing_voice == Some(message_id);
        if quill::message_menu::timecode_offered(
            effective_content(&message.content, message.ephemeral.as_ref()),
            playing_here,
            can_reply,
        ) && let Some(position) = self.playback.active_position_secs()
        {
            let timecode = quill::message_menu::timecode_text(position);
            let label_timecode = timecode.clone();
            let row_hover = cx.theme().accent;
            rows.push((
                order::REPLY_TIMECODE,
                div()
                    .id("menu-reply-timecode")
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
                    .menu_key_target(
                        row_key(order::REPLY_TIMECODE),
                        &self.frame.context_menu_items,
                        cx,
                    )
                    .role(gpui_kit::Role::MenuItem)
                    .aria_label(tr_with(
                        keys::Q_CHAT_REPLY_WITH_TIMECODE,
                        &[("timecode", &timecode.to_string())],
                    ))
                    .child(Icon::new(gpui_kit::assets::IconName::Reply).size(px(16.)))
                    .child(div().flex_1().child(tr(keys::Q_CHAT_REPLY_WITH_TIMECODE_2)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child(label_timecode),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reply_with_timecode(chat_id, message_id, window, cx);
                        this.message_ui.menu = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        }
        // Reply in Another Chat: TDLib decides per message, secret chats
        // never take part.
        let is_secret_chat = matches!(chat_kind, Some(ChatKind::Secret { .. }));
        if !is_secret_chat
            && message_id.0 > 0
            && allows(false, |a| a.can_be_replied_in_another_chat)
        {
            item!(
                order::REPLY_ELSEWHERE,
                gpui_kit::assets::IconName::Replace,
                "menu-reply-another-chat",
                tr(keys::REPLY_IN_ANOTHER_CHAT),
                this,
                window,
                cx,
                {
                    this.message_ui.menu = None;
                    this.begin_reply_elsewhere(chat_id, message_id, window, cx);
                }
            );
        }
        let copy = match selection {
            Some(selected) => Some((tr(keys::Q_CHAT_COPY_SELECTED_TEXT), selected)),
            None => copyable.map(|text| (tr(keys::Q_CHAT_COPY_TEXT), text)),
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
                if label == tr(keys::Q_CHAT_COPY_TEXT) {
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
                    this.message_ui.menu = None;
                    cx.notify();
                }
            );
        }
        // Translate Selected Text over a selection, Translate for the
        // message's own text (Telegram Desktop: both can show; the second
        // only while the chat is not already shown translated).
        if let Some(selected) = self
            .message_ui
            .menu_selection
            .clone()
            .filter(|selected| self.translate_menu_offered(chat_id, selected))
        {
            item!(
                order::TRANSLATE_SELECTED,
                gpui_kit::assets::IconName::Languages,
                "menu-translate-selected",
                tr(keys::Q_CHAT_TRANSLATE_SELECTED_TEXT),
                this,
                window,
                cx,
                {
                    this.message_ui.menu = None;
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
                tr(keys::Q_CHAT_TRANSLATE),
                this,
                window,
                cx,
                {
                    this.message_ui.menu = None;
                    this.open_translate_message(chat_id, message_id, window, cx);
                }
            );
        }
        // Over a link, the menu leads with what Telegram Desktop adds for it:
        // a copy entry named for the kind of link (`copyToClipboardContextItemText`),
        // and, for web links, Open.
        if let Some(link) = self.message_ui.menu_link.clone() {
            let msg_key = (chat_id.0, message_id.0 as u64);
            if matches!(link, quill::text::LinkTarget::Url { .. }) {
                let open = link.clone();
                item!(
                    4,
                    gpui_kit::assets::IconName::ExternalLink,
                    "menu-open-link",
                    tr(keys::Q_CHAT_OPEN_LINK),
                    this,
                    _window,
                    cx,
                    {
                        this.message_ui.menu = None;
                        this.queue_link(open.clone(), msg_key, cx);
                    }
                );
            }
            // A TON address: "Copy Address" below and, as the iOS address
            // menu, "View in Explorer".
            if let quill::text::LinkTarget::TonAddress(address) = &link {
                let address = address.clone();
                rows.push((
                    order::TON_EXPLORER,
                    super::super::ton_wallet_ui::explorer_menu_row(
                        "menu-ton-explorer",
                        address,
                        |this| this.message_ui.menu = None,
                        Some((&self.frame.context_menu_items, row_key(order::TON_EXPLORER))),
                        cx,
                    ),
                ));
            }
            if matches!(link, quill::text::LinkTarget::TonAddress(_))
                && self.session().is_some_and(|s| s.ton_wallet.can_use)
            {
                rows.push((
                    order::TON_SEND_MONEY,
                    super::super::ton_wallet_ui::send_money_menu_row(
                        "menu-ton-send-money",
                        |this| this.message_ui.menu = None,
                        Some((
                            &self.frame.context_menu_items,
                            row_key(order::TON_SEND_MONEY),
                        )),
                        cx,
                    ),
                ));
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
                        this.message_ui.menu = None;
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
                tr(keys::Q_CHAT_FORWARD),
                this,
                window,
                cx,
                {
                    this.begin_forward_one(chat_id, message_id, message.pending, window, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                }
            );
            item!(
                70,
                gpui_kit::assets::IconName::CircleCheck,
                "menu-select",
                tr(keys::Q_CHAT_SELECT),
                this,
                _window,
                cx,
                {
                    this.toggle_forward_select(chat_id, message_id, message.pending, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                }
            );
            if self.can_select_up_to(chat_id, message_id) {
                item!(
                    71,
                    gpui_kit::assets::IconName::CircleCheck,
                    "menu-select-up-to",
                    tr(keys::CONTEXT_SELECT_MSG_BULK),
                    this,
                    _window,
                    cx,
                    {
                        this.select_up_to(chat_id, message_id, cx);
                        this.message_ui.menu = None;
                        cx.notify();
                    }
                );
            }
        }
        // Telegram Desktop's share box copies a game's `t.me/<bot>?game=` link.
        if matches!(message.content, MessageContent::Game(_)) && !message.pending {
            item!(
                51,
                gpui_kit::assets::IconName::Link,
                "menu-copy-game-link",
                tr(keys::Q_CHAT_COPY_GAME_LINK),
                this,
                _window,
                cx,
                {
                    this.copy_game_link(chat_id, message_id, cx);
                    this.message_ui.menu = None;
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
        .map(|edit| edit.in_album(message.media_album_id != 0))
        .filter(|_| allows(true, |a| a.can_be_edited))
        {
            item!(
                20,
                gpui_kit::assets::IconName::Pencil,
                "menu-edit",
                tr(keys::MAC_MENU_EDIT),
                this,
                window,
                cx,
                {
                    this.begin_edit(edit.clone(), window, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                }
            );
        }
        // Channel admins (and anyone TDLib allows) add or edit a fact check.
        if allows(false, |a| a.can_set_fact_check) && message.id.0 > 0 && !message.pending {
            let existing = message
                .extras
                .fact_check
                .as_ref()
                .map(|check| check.text.clone())
                .unwrap_or_default();
            let label = quill::message_menu::fact_check_label(!existing.is_empty());
            item!(
                order::FACT_CHECK,
                gpui_kit::assets::IconName::ShieldCheck,
                "menu-fact-check",
                label,
                this,
                window,
                cx,
                {
                    this.message_ui.menu = None;
                    this.open_fact_check(chat_id, message_id, existing.clone(), window, cx);
                }
            );
        }
        if allows(self.can_add_offer(chat_id, message_id), |a| a.can_add_offer) {
            item!(
                order::ADD_OFFER,
                gpui_kit::assets::IconName::CircleDollarSign,
                "menu-add-offer",
                tr(keys::Q_CHAT_ADD_OFFER),
                this,
                window,
                cx,
                {
                    this.message_ui.menu = None;
                    this.begin_add_offer(chat_id, message_id, window, cx);
                }
            );
        }
        if allows(message.can_pin() && !is_channel_post, |a| a.can_be_pinned) {
            let label = if pinned {
                tr(keys::Q_CHAT_UNPIN)
            } else {
                tr(keys::CONTEXT_PIN_MSG)
            };
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
                    this.message_ui.menu = None;
                    this.request_toggle_pin(chat_id, message_id, _window, cx);
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
                    this.message_ui.menu = None;
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
            let label = if is_quiz {
                tr(keys::Q_CHAT_STOP_QUIZ)
            } else {
                tr(keys::Q_CHAT_STOP_POLL)
            };
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
                    this.message_ui.menu = None;
                    cx.notify();
                }
            );
        }
        // Telegram Desktop's "Retract vote" (`AddPollActions`), above Stop.
        if let MessageContent::Poll(poll_content) = &message.content
            && quill::poll::can_retract_vote(&poll_content.poll)
            && message.id.0 > 0
            && !message.pending
        {
            item!(
                order::RETRACT_VOTE,
                gpui_kit::assets::IconName::Undo2,
                "menu-retract-vote",
                tr(keys::Q_CHAT_RETRACT_VOTE),
                this,
                _window,
                cx,
                {
                    this.retract_poll_vote(chat_id, message_id, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                }
            );
        }
        // B15: "Poll Stats" (`getPollVoteStatistics`, schema 1.8.67 line
        // 12947) when `messageProperties.can_get_poll_vote_statistics`.
        if matches!(message.content, MessageContent::Poll(_))
            && allows(false, |a| a.can_get_poll_vote_statistics)
        {
            item!(
                quill::message_menu::order::POLL_STATS,
                gpui_kit::assets::IconName::ChartPie,
                "menu-poll-stats",
                tr(keys::Q_CHAT_POLL_STATS),
                this,
                _window,
                cx,
                {
                    this.open_poll_stats_dialog(chat_id, message_id, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                }
            );
        }
        // "Statistics" of a channel post (tdesktop `AddViewStatisticsAction`;
        // TDLib allows it when `messageProperties.can_get_statistics`).
        if message.id.0 > 0 && allows(false, |a| a.can_get_statistics) {
            item!(
                quill::message_menu::order::STATISTICS,
                gpui_kit::assets::IconName::ChartPie,
                "menu-statistics",
                tr(keys::Q_CHAT_STATISTICS),
                this,
                window,
                cx,
                {
                    this.message_ui.menu = None;
                    this.open_message_statistics_panel(chat_id, message_id, window, cx);
                }
            );
        }
        // Channel-post comments and group reply threads: `getMessageThread`
        // (schema 1.8.67, line 11566) gated by
        // `messageProperties.can_get_message_thread`.
        let reply_count = message
            .interaction_info
            .as_ref()
            .and_then(|info| info.reply_info.as_ref())
            .map_or(0, |reply| reply.reply_count);
        if (is_channel_post || reply_count > 0) && allows(false, |a| a.can_get_message_thread) {
            // Comments for channel posts; "View N Replies" / "View Thread"
            // for group messages (tdesktop `lng_replies_view`).
            let label: SharedString = if is_channel_post {
                tr(keys::Q_CHAT_VIEW_COMMENTS).into()
            } else {
                quill::state::replies_menu_label(reply_count).into()
            };
            item!(
                order::VIEW_COMMENTS,
                gpui_kit::assets::IconName::MessageSquare,
                "menu-comments",
                label.clone(),
                this,
                window,
                cx,
                {
                    this.open_thread_view(chat_id, message_id, window, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                }
            );
        }
        if failed && message.can_retry {
            item!(
                58,
                gpui_kit::assets::IconName::RotateCw,
                "menu-retry",
                tr(keys::Q_CHAT_RESEND),
                this,
                _window,
                cx,
                {
                    this.retry_failed_message(chat_id, message_id, cx);
                    this.message_ui.menu = None;
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
                tr(keys::MAC_MENU_DELETE),
                this,
                window,
                cx,
                {
                    this.open_delete_dialog_with(confirm.clone(), moderation.clone(), window, cx);
                    this.message_ui.menu = None;
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
                tr(keys::Q_CHAT_CANCEL_UPLOAD),
                true,
                &self.frame.context_menu_items,
                cx,
                move |this, _, cx| {
                    this.cancel_message_upload(chat_id, message_id, cx);
                    this.message_ui.menu = None;
                    cx.notify();
                },
            );
            rows.push(cancel);
        }
        // Telegram Desktop's media block, Report, and the audience rows.
        rows.extend(self.menu_media_rows(chat_id, &message, actions, cx));
        // The task or poll option under the pointer, a shared contact's phone.
        rows.extend(self.menu_item_rows(chat_id, message_id, content, can_reply, cx));
        rows.extend(self.menu_gift_row(chat_id, &message, cx));
        rows.extend(self.menu_block_sender_row(chat_id, &message, cx));
        let reportable = !message.pending
            && message.id.0 > 0
            && !message.is_outgoing
            && !saved
            && !self.session().is_some_and(|s| s.is_replies_chat(chat_id))
            && actions.is_none_or(|a| a.can_report_chat);
        if reportable {
            let report = menu_row(
                order::REPORT,
                gpui_kit::assets::IconName::Flag,
                "menu-report",
                tr(keys::Q_CHAT_REPORT),
                false,
                &self.frame.context_menu_items,
                cx,
                move |this, _, cx| {
                    // A grouped album reports every photo in it.
                    this.open_message_report(chat_id, vec![message_id], cx);
                },
            );
            rows.push(report);
        }
        // "This message contains emoji from X pack": opens the pack.
        let packs = self
            .session()
            .map(|s| {
                s.message_emoji_pack_ids(effective_content(
                    &message.content,
                    message.ephemeral.as_ref(),
                ))
            })
            .unwrap_or_default();
        let pack_name = match packs[..] {
            [only] => self
                .session()
                .and_then(|s| s.stickers.emoji_pack_titles.get(&only).cloned()),
            _ => None,
        };
        if let Some((before, bold, after)) =
            quill::message_menu::emoji_pack_footer(packs.len(), pack_name.as_deref())
            && let Some(&first) = packs.first()
        {
            let row_hover = cx.theme().accent;
            rows.push(menu_separator(order::EMOJI_PACKS - 1));
            rows.push((
                order::EMOJI_PACKS,
                div()
                    .id("menu-emoji-packs")
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .max_w(px(260.))
                    .text_color(text_muted())
                    .hover(|style| style.bg(row_hover))
                    .menu_key_target(
                        row_key(order::EMOJI_PACKS),
                        &self.frame.context_menu_items,
                        cx,
                    )
                    .role(gpui_kit::Role::MenuItem)
                    .aria_label(format!("{before}{bold}{after}"))
                    .child(before)
                    .child(div().font_semibold().child(bold))
                    .child(after)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.message_ui.menu = None;
                        this.view_message_sticker_set(first, cx);
                    }))
                    .into_any_element(),
            ));
        }
        let mut audience_rows = self.menu_audience_rows(chat_id, &message, cx);
        self.add_origin_rows(&mut audience_rows, chat_id, &message);
        audience_rows.extend(self.menu_author_row(chat_id, message_id, cx));
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
            // A private chat names the peer, or says it is a bot.
            let peer = match chat_kind {
                Some(ChatKind::Private { user_id }) => self
                    .session()
                    .map(|s| (s.is_bot_user(user_id.0), s.user(user_id.0))),
                _ => None,
            };
            let is_bot = peer.is_some_and(|(bot, _)| bot);
            let peer_name = peer
                .and_then(|(_, user)| user)
                .map(|user| user.first_name.trim().to_string())
                .filter(|name| !name.is_empty());
            let protected_by_me = actions.map(|a| a.has_protected_content_by_current_user);
            if peer.is_none() || is_bot || protected_by_me.is_some() {
                rows.push(info_row(
                    order::SELECT,
                    "menu-noforwards",
                    None,
                    quill::message_menu::noforwards_text(
                        is_channel_post,
                        is_group && !is_channel_post,
                        is_bot,
                        protected_by_me.unwrap_or(false),
                        peer_name.as_deref(),
                    ),
                ));
            }
        }
        let page_rows: Vec<MenuRow> = match page {
            MessageMenuPage::Main => rows,
            sub => {
                let mut sub_rows = vec![menu_row(
                    0,
                    gpui_kit::assets::IconName::ChevronLeft,
                    "menu-back",
                    match sub {
                        MessageMenuPage::SaveTo => tr(keys::Q_CHAT_SAVE_TO),
                        MessageMenuPage::AddToSet { emoji } => {
                            quill::sticker_set_box::SetKind::of(emoji).add_to_set_label()
                        }
                        _ => tr(keys::Q_CHAT_BACK),
                    },
                    false,
                    &self.frame.context_menu_items,
                    cx,
                    |this, _, cx| {
                        this.message_ui.menu_ui.page = MessageMenuPage::Main;
                        cx.notify();
                    },
                )];
                sub_rows.extend(match sub {
                    MessageMenuPage::SaveTo => self.menu_save_to_rows(chat_id, message_id, cx),
                    MessageMenuPage::AddToSet { emoji } => {
                        self.menu_add_to_set_rows(chat_id, message_id, emoji, cx)
                    }
                    MessageMenuPage::PollOption => {
                        self.menu_option_page_rows(chat_id, message_id, &message, can_reply, cx)
                    }
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
            .track_focus(&self.frame.context_menu_focus)
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
                        this.close_message_menu();
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
            .focus_trap("message-menu-focus", &self.frame.context_menu_focus)
            .into_any_element()
    }
}
