//! Methods moved out of `message_actions.rs` to keep files under 1000 lines.

use super::*;

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
        let selection = self.message_ui.menu_selection.clone();
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
        ) && let Some(position) = self.playback.clock.as_ref().map(|c| c.elapsed_secs())
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
                    .role(gpui_kit::Role::MenuItem)
                    .aria_label(format!("Reply with timecode {timecode}"))
                    .child(Icon::new(gpui_kit::assets::IconName::Reply).size(px(16.)))
                    .child(div().flex_1().child("Reply with timecode"))
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
                "Reply in Another Chat",
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
                "Translate Selected Text",
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
                "Translate",
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
                    "Open Link",
                    this,
                    _window,
                    cx,
                    {
                        this.message_ui.menu = None;
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
                "Forward",
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
                "Select",
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
                    "Select up to this message",
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
                "Copy game link",
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
                "Edit",
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
            let existing = message.extras.fact_check.clone();
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
                "Retract vote",
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
                "Poll Stats",
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
                "View Comments".into()
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
                "Resend",
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
                "Delete",
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
                "Cancel Upload",
                true,
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
        let reportable = !message.pending
            && message.id.0 > 0
            && !message.is_outgoing
            && !saved
            && actions.is_none_or(|a| a.can_report_chat);
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
        {
            let first = packs[0];
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
            rows.push(info_row(
                order::SELECT,
                "menu-noforwards",
                None,
                quill::message_menu::noforwards_text(
                    is_channel_post,
                    is_group && !is_channel_post,
                    is_bot,
                    message.is_outgoing,
                    peer_name.as_deref(),
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
                        this.message_ui.menu_ui.page = MessageMenuPage::Main;
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
                        this.message_ui.menu = None;
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

    /// Slice CL: start a long press on a chat-list row — after
    /// `CHAT_PREVIEW_LONG_PRESS` without a release the read-only peek
    /// preview opens; a quicker release stays a plain click.
    pub(in crate::ui) fn begin_chat_preview_press(
        &mut self,
        chat_id: ChatId,
        anchor: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let started = Instant::now();
        self.chat_list.preview_press = Some((chat_id, started));
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Self::CHAT_PREVIEW_LONG_PRESS)
                .await;
            let _ = this.update(cx, |this, cx| {
                // Still the same press (no release, no newer press)?
                if this.chat_list.preview_press == Some((chat_id, started)) {
                    this.open_chat_preview(chat_id, anchor, cx);
                }
            });
        })
        .detach();
    }

    /// Slice CL: open the read-only peek preview for a chat — recent
    /// messages in a floating panel beside the row. Never opens the
    /// chat and never marks anything read.
    pub(in crate::ui) fn open_chat_preview(
        &mut self,
        chat_id: ChatId,
        anchor: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.chat_list.menu = None;
        self.chat_list.preview = Some(ChatPreviewState { chat_id, anchor });
        // Live fetch for unopened chats; already-loaded history renders
        // immediately. Demo/no-driver sessions skip the network.
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_chat_preview_history(chat_id);
        }
        cx.notify();
    }
}
