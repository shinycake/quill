//! Methods moved out of `history.rs` to keep files under 1000 lines.

use super::*;
use quill::lang::{keys, tr, tr_with};

pub(in crate::ui) fn session_history_row(
    message: &HistoryMessage,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    // kit Phase 4: per-row chrome (sender header / outbox receipt /
    // avatar) computed at row-build time so the virtualized row doesn't
    // carry the chat.
    sender: Option<SenderLabel>,
    // The Premium status or star after the sender's name.
    sender_status: Option<AnyElement>,
    receipt: OutboxReceipt,
    sender_avatar: Option<SenderAvatar>,
    reply_header: Option<quill::state::ReplyHeader>,
    forward_header: Option<quill::state::ForwardHeader>,
    via_bot: Option<String>,
    // Seek-bar view for audio/voice rows (`None` for other content).
    seek_bar: Option<SeekBarView>,
    animation_playing: bool,
    gif_play: Option<crate::ui::gif_clip::GifPlay>,
    sticker_frame: Option<super::super::sticker_player::StickerView>,
    // A dice's animated layers, bottom first (`DiceStickers::layers`;
    // empty for other rows).
    dice_layers: Vec<Option<super::super::sticker_player::StickerView>>,
    // Decoded animations of the message's custom emoji (by custom emoji id).
    animated_emoji: HashMap<i64, super::super::sticker_player::StickerView>,
    // The row's clip playing inline (muted autoplay), if any.
    inline: Option<super::super::inline_video::InlineFrame>,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Phase B4: whether the row's chat is a secret chat — selects the
    // "Self-destruct" vs "Auto-delete" service-row wording.
    _is_secret: bool,
    // The fact-check block under the message is expanded (`message_ui`).
    fact_check_expanded: bool,
    // Phase C2i: session for `messageCall` peer resolution ("Call
    // again" only for 1:1 chats).
    session: Option<&Session>,
    // Settings → Appearance: font size + bubble/plain style.
    look: BubbleLook,
    corner: CornerActions,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    // Stickers, dice, round video messages and emoji-only messages (the ones
    // drawn big) stand on their own, without a bubble, as in Telegram
    // Desktop. Their time then sits on the chat background, so it takes
    // the theme's text color instead of the bubble's.
    let emoji_media = super::super::animated_emoji::message_emoji_media(message, session);
    let emoji_only = emoji_media.is_some();
    let look = if emoji_only
        || matches!(
            message.content,
            MessageContent::Sticker(_) | MessageContent::VideoNote(_) | MessageContent::Dice(_)
        ) {
        BubbleLook {
            plain: true,
            text: cx.theme().foreground,
            ..look
        }
    } else {
        look
    };
    // Service actions (members, pins, gifts, topics…), timer changes and
    // screenshots: Telegram Desktop's wording as a centered pill — no
    // bubble, no reply/react/edit/delete controls.
    // The wide layout centres service messages in its column
    // (`Service::countGeometry`).
    let wide = look.wide;
    if super::super::service_row::is_service_row(&message.content) {
        return super::super::chat_wide_ui::wide_column(
            wide,
            super::super::service_row::service_message_row(
                message,
                session,
                files,
                media_roots,
                cx,
            ),
        );
    }
    // Community service rows (`messageChatAddedToCommunity` and kin).
    if let Some(text) =
        super::community_rows::community_service_text(message, session, sender.as_ref())
    {
        return super::super::chat_wide_ui::wide_column(
            wide,
            service_pill(("community-service-row", message.id.0 as u64), text, cx),
        );
    }
    // Phase C2f: `messageGroupCall` invitation service row (schema
    // 1.8.67, line 5288) — incoming and pending: Accept / Decline.
    if let MessageContent::GroupCallInvitation {
        is_active,
        was_missed,
        ..
    } = &message.content
    {
        return super::super::chat_wide_ui::wide_column(
            wide,
            QuillApp::group_call_invitation_row(message, *is_active, *was_missed, cx),
        );
    }
    let on_fill = message.is_outgoing && !look.plain;
    let quote = reply_header.map(|header| {
        let thumb = QuillApp::reply_thumb_path(&header, files, media_roots);
        let pattern = QuillApp::reply_pattern_path(&header, session, files, media_roots);
        reply_header_strip(message.id, header, thumb, pattern, on_fill, cx)
    });
    let has_forward = forward_header.is_some();
    let forward_strip = forward_header.map(|header| {
        let original = (header.original_date > 0 && message.forward_info.is_some()).then(|| {
            tr_with(
                keys::Q_CHAT_ORIGINAL,
                &[(
                    "into",
                    &(super::super::message_text::format_unix_date_time(
                        header.original_date.into(),
                    ))
                    .to_string(),
                )],
            )
        });
        forward_header_line(message.id, header, via_bot.clone(), original, on_fill, cx)
    });
    let via_strip = via_bot
        .filter(|_| !has_forward)
        .map(|bot| via_bot_line(message.id, bot, on_fill));
    let header_parts: Vec<AnyElement> = [via_strip, forward_strip, quote]
        .into_iter()
        .flatten()
        .collect();
    let header = match header_parts.len() {
        0 => None,
        1 => header_parts.into_iter().next(),
        _ => Some(
            div()
                .id(("row-headers", message.id.0 as u64))
                .flex()
                .flex_col()
                .children(header_parts)
                .into_any_element(),
        ),
    };
    // How wide the bubble may get in this history (`BubbleRoom`), and its
    // picture: inside the border when it leads the bubble, inside the
    // padding under a header.
    let room = bubble_width::BubbleRoom::new(look.history, sender_avatar.is_some(), look.plain);
    let media_max = room.media(header.is_none());
    let chat_id = message.chat_id;
    let message_id = message.id;
    let mut actions_span = Pixels::default();
    let mut more_btn = corner.mount(|| {
        let (buttons, span) = message_corner_actions(chat_id, message_id, message, session, cx);
        actions_span = span;
        buttons
    });
    // Broadcast posts (Phase 2.2): eye glyph + compact view count, like the
    // official clients' post footer. Renders whenever views exist; only
    // channel posts carry a view count in practice.
    // Channel post views and the author signature ride in the footer row
    // with the time ("Demo Admin · 👁 12.4K · 16:44").
    let views = message
        .interaction_info
        .as_ref()
        .map(|info| info.view_count)
        .filter(|&count| count > 0);
    // Phase D2: author signature (`message.author_signature`, schema 1.8.67
    // lines 3155/3165). The official clients show it under channel posts
    // and anonymous admin messages. Suppressed when the message is
    // forwarded — `forward_from_strip` already attributes the signature
    // there, and a second line would double-attribute.
    let signature = message
        .author_signature
        .clone()
        .filter(|_| message.forward_info.is_none());
    let footer_meta = FooterMeta::of(message, receipt, views, signature.clone())
        .with_effect(super::super::message_effect_badge::footer_effect(
            message,
            session,
            files,
            media_roots,
        ))
        .with_forward_drag(crate::ui::forward_drag_source::drag_for_message(
            session, message,
        ));
    // Telegram Desktop widens a picture to its caption's longest line (the
    // time's skip block included when the caption ends the bubble).
    let info_width = footer_meta.reserve(footer_reserve(message.is_outgoing));
    let chips = message.reaction_chips();
    // Telegram Desktop shows who reacted (small avatars) instead of a
    // count when there are at most three known reactors, outside channels.
    let in_channel = session
        .and_then(|s| s.chats.get(&message.chat_id.0))
        .is_some_and(|chat| {
            matches!(
                chat.kind,
                quill::telegram::envelope::ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        });
    // Saved Messages reactions are tags: Telegram Desktop shows just the
    // tag's emoji, without a count or who reacted.
    let are_tags = message
        .interaction_info
        .as_ref()
        .and_then(|info| info.reactions.as_ref())
        .is_some_and(|reactions| reactions.are_tags);
    let can_list_reactors = message
        .interaction_info
        .as_ref()
        .and_then(|info| info.reactions.as_ref())
        .is_some_and(|reactions| reactions.can_get_added_reactions);
    let has_chips = !chips.is_empty() || message.paid_reaction().is_some();
    // Channel posts open their comments, group messages their replies
    // (tdesktop `paintCommentsButton`). Inside the open thread itself the
    // bar would only point back at the view.
    let in_thread = session.is_some_and(|s| s.thread_for_chat(message.chat_id).is_some());
    let in_group = session
        .and_then(|s| s.chats.get(&message.chat_id.0))
        .is_some_and(|chat| {
            matches!(
                chat.kind,
                quill::telegram::envelope::ChatKind::BasicGroup { .. }
                    | quill::telegram::envelope::ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
            )
        });
    let mut reply_bar_el = message
        .interaction_info
        .as_ref()
        .and_then(|info| info.reply_info.as_ref())
        .filter(|_| !in_thread && message.id.0 > 0 && !message.pending)
        .and_then(|info| {
            let kind = if in_channel {
                super::super::threads::ReplyBarKind::Comments
            } else if in_group && info.reply_count > 0 {
                super::super::threads::ReplyBarKind::Replies
            } else {
                return None;
            };
            Some(super::super::threads::reply_bar(
                message.chat_id,
                message.id,
                kind,
                info,
                message.is_outgoing,
                super::super::threads::replier_avatars(info, session, media_roots),
                cx,
            ))
        });
    let fast_buttons = session.is_some_and(|session| {
        session
            .fast_button_target(message.chat_id)
            .is_some_and(|target| {
                target.message_id == message.id && quill::fast_buttons::is_enabled(target.bot_id)
            })
    });
    let keyboard = inline_keyboard(message, fast_buttons, cx);
    let has_caption = match &message.content {
        MessageContent::Photo(photo) => !photo.caption.is_empty(),
        MessageContent::Document(doc) => !doc.caption.is_empty(),
        MessageContent::Video(video) => !video.caption.is_empty(),
        MessageContent::Animation(animation) => !animation.caption.is_empty(),
        _ => false,
    };
    // A picture, video or GIF with nothing else in its bubble stands on its
    // own, as in Telegram Desktop (`quill::media_bubble`).
    let bubbleless = !look.plain
        && leads_with_media(effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        ))
        && !quill::media_bubble::MediaSurroundings {
            caption: has_caption,
            header: header.is_some(),
            sender_name: sender.is_some(),
            comments_bar: reply_bar_el.is_some(),
            keyboard: keyboard.is_some(),
            fact_check: message.extras.fact_check.is_some(),
            countdown: message.self_destruct_badge(unix_ms_now()).is_some()
                || message.auto_delete_chip(unix_ms_now()).is_some(),
        }
        .needs_bubble();
    let chip_row = (!chips.is_empty() || message.paid_reaction().is_some()).then(|| {
        reaction_chip_row(
            message,
            &chips,
            are_tags,
            can_list_reactors,
            in_channel,
            session,
            files,
            media_roots,
            &footer_meta,
            bubbleless,
            cx,
        )
    });
    let tail = bubble_width::caption_tail(message, reply_bar_el.is_some(), unix_ms_now());
    let caption_w = bubble_width::caption_width(
        cx,
        &message.content,
        look.font,
        bubble_width::caption_ends_bubble(tail).then_some(info_width),
    );
    // A spoiler stays covered until clicked; once revealed, it renders as
    // the plain media it is.
    let reveal_key = super::super::spoiler_fx::media_key(message.chat_id, message.id);
    let media_revealed = revealed.contains(&reveal_key);
    // Media corners follow the bubble (Telegram Desktop `BubbleRounding`): a
    // picture that leads the bubble takes its radius on the edges that are
    // free of the sender name, a caption above/below, badges or reactions.
    let corners = if bubbleless {
        MediaCorners::bare(
            cx,
            quill::chat_wide::has_right_layout(message.is_outgoing, look.wide),
            look.joined_above,
        )
    } else {
        let media_led = header.is_none()
            && leads_with_media(effective_content(
                &message.content,
                message.ephemeral.as_ref(),
            ));
        let caption_above = has_caption && caption_above_media(&message.content);
        let caption_below = has_caption && !caption_above;
        let badges = message.self_destruct_badge(unix_ms_now()).is_some()
            || message.auto_delete_chip(unix_ms_now()).is_some();
        MediaCorners::in_bubble(
            cx,
            message.is_outgoing,
            look.plain,
            media_led,
            sender.is_none() && !caption_above,
            !caption_below && !badges && !has_chips,
        )
    };
    let covered_content = effective_content(&message.content, message.ephemeral.as_ref());
    // A revealed spoiler is plain media from here on: it shows, opens and
    // plays like any other.
    let unveiled = media_revealed
        .then(|| quill::media_spoiler::unveiled(covered_content))
        .flatten();
    let content = unveiled.as_ref().unwrap_or(covered_content);
    let (frame_w, frame_h) = single_media_frame(content, caption_w, media_max).unwrap_or_default();
    let extra_media = match content {
        content if quill::media_spoiler::has_media_spoiler(content) => Some(spoiler_cover(
            message.id.0 as u64,
            message.chat_id,
            message.id,
            content,
            files,
            downloading,
            (frame_w, frame_h),
            corners,
            cx,
        )),
        MessageContent::Photo(photo) => {
            Some(photo_attachment(
                message.id.0 as u64,
                photo,
                files,
                downloading,
                media_roots,
                None,
                Some((message.chat_id, message.id)),
                corners,
                caption_w,
                media_max,
                cx,
            ))
        }
        MessageContent::Document(doc) if quill::media_viewer::shows_as_photo(doc) => {
            let gif = gif_play.filter(|_| animation_playing);
            Some(picture_file_attachment(message.id.0 as u64, doc, files, downloading, media_roots, (message.chat_id, message.id), (frame_w, frame_h), corners, gif, cx))
        }
        MessageContent::Document(doc) => Some(document_chip(
            message.id.0 as u64,
            doc,
            message.is_outgoing,
            files,
            downloading,
            media_roots,
            // The pause toggle is for user-initiated (listed) downloads.
            session
                .filter(|s| s.media.downloads.is_user(doc.file_id.0))
                .map(|s| s.media.downloads.is_paused(doc.file_id.0)),
            DocOrigin::Message(message.chat_id, message.id),
            room,
            cx,
        )),
        MessageContent::Sticker(sticker) => Some(crate::ui::send_fly::wrap(
            message,
            sticker,
            crate::ui::premium_sticker_effect::wrap(
                message.id.0 as u64,
                sticker,
                message.pending,
                message.chat_id.0,
                message.id.0,
                // The effect plays toward the side the sticker faces.
                quill::chat_wide::has_right_layout(message.is_outgoing, look.wide),
                sticker_attachment(
                    message.id.0 as u64,
                    sticker,
                    files,
                    downloading,
                    media_roots,
                    sticker_frame.clone(),
                    cx,
                ),
                cx,
            ),
        )),
        // Voice and audio rows always get a seek view above.
        MessageContent::VoiceNote(note) => seek_bar.as_ref().map(|seek_bar| {
            voice_note_row(
                message.chat_id,
                message.id,
                message.is_outgoing,
                note,
                files,
                downloading,
                seek_bar,
                room,
                f32::from(info_width).ceil() as i32,
                cx,
            )
        }),
        MessageContent::Audio(audio) => seek_bar.as_ref().map(|seek_bar| {
            audio_row(
                message.id,
                message.is_outgoing,
                audio,
                files,
                downloading,
                media_roots,
                seek_bar,
                room,
                cx,
            )
        }),
        MessageContent::Animation(animation) => Some(animation_attachment(
            message.id,
            animation,
            files,
            downloading,
            media_roots,
            animation_playing,
            gif_play,
            inline,
            None,
            Some((message.chat_id, message.id)),
            corners,
            caption_w,
            media_max,
            cx,
        )),
        MessageContent::Video(video) => Some(video_attachment(
            message.id,
            video,
            files,
            downloading,
            media_roots,
            inline,
            None,
            Some((message.chat_id, message.id)),
            corners,
            caption_w,
            media_max,
            cx,
        )),
        MessageContent::VideoNote(note) => Some(video_note_attachment(
            message.chat_id,
            message.id,
            message.is_outgoing,
            note,
            files,
            downloading,
            media_roots,
            inline,
            cx,
        )),
        MessageContent::Poll(poll) => Some(poll_row(message, poll, (files, media_roots, session), room, cx)),
        // B15: checklist card (tasks, done marks, completer names).
        MessageContent::Checklist(checklist) => Some(checklist_body(
            message.chat_id,
            message.id,
            &checklist.list,
            session,
            cx,
        )),
        // Slice P1: invoice card + payment receipt rows.
        MessageContent::Invoice(invoice) => {
            Some(invoice_body(message.chat_id, message.id, invoice, cx))
        }
        MessageContent::PaymentSuccessful(success) => Some(payment_success_row(success, cx)),
        MessageContent::PaymentReceived(received) => Some(payment_received_row(received, cx)),
        MessageContent::Location(location) => Some(location_row(
            message.id.0 as u64,
            &location.location,
            location.live.as_ref(),
            location
                .live
                .is_some_and(|live| {
                    live.can_stop_at(message.is_outgoing, quill::local_time::now_unix())
                })
                .then_some((message.chat_id, message.id)),
            map_tile_path(session, &location.location, files, media_roots),
            cx,
        )),
        MessageContent::Venue(venue) => Some(venue_row(
            message.id.0 as u64,
            venue,
            map_tile_path(session, &venue.location, files, media_roots),
            cx,
        )),
        MessageContent::Contact(contact) => Some(contact_row(
            message.id.0 as u64,
            contact,
            ContactCardState::of(contact.user_id, session),
            session
                .and_then(|s| s.user_photo_path(contact.user_id))
                .and_then(|path| sandboxed_display_path(path, media_roots)),
            room,
            cx,
        )),
        MessageContent::Dice(dice) => Some(dice_row(
            message.id.0 as u64,
            dice,
            files,
            media_roots,
            dice_layers,
        )),
        // Locked paid media and forwarded stories are bubble cards.
        MessageContent::Action(_) => action_card(message, session, files, downloading, media_roots, media_max, cx),
        MessageContent::Text(_)
        | MessageContent::RichMessage(_)
        | MessageContent::Game(_)
        | MessageContent::GroupCallInvitation { .. }
        | MessageContent::Call { .. }
        | MessageContent::ChatTtlChanged { .. }
        | MessageContent::ScreenshotTaken
        // Slice C2k: community service rows render no extra media.
        | MessageContent::ChatAddedToCommunity { .. }
        | MessageContent::ChatRemovedFromCommunity
        // Slice G9: the join-from-community service row renders no extra
        // media either (the name ships in the centered row text).
        | MessageContent::ChatJoinFromCommunity { .. } => None,
        MessageContent::Unsupported { .. } => Some(
            div().id(("unsupported-update-card", message.id.0 as u64))
                .flex().flex_col().gap_2().p_3()
                .child(div().id(("unsupported-message-label", message.id.0 as u64))
                    .role(Role::Label)
                    .aria_label(tr(keys::Q_CHAT_QUILL_CANNOT_DISPLAY_THIS_MESSAGE))
                    .child(tr(keys::Q_CHAT_QUILL_CANNOT_DISPLAY_THIS_MESSAGE)))
                .child(Button::new(("unsupported-update", message.id.0 as u64))
                    .label(tr(keys::Q_CHAT_GET_LATEST_QUILL))
                    .on_click(cx.listener(|this, _, _, cx| this.open_link(quill::updater::RELEASES_URL, cx))))
                .into_any_element(),
        ),
    };
    // Self-destruct badge: counts down locally against `unix_ms_now()` on
    // the 1-second tick (`ensure_self_destruct_tick`); TDLib deletes the
    // row when the timer fires.
    let outgoing = message.is_outgoing;
    let self_destruct_badge = message.self_destruct_badge(unix_ms_now()).map(|label| {
        div()
            .id(("self-destruct-badge", message.id.0 as u64))
            .mt_1()
            .text_xs()
            // Outgoing bubbles are accent-filled: inherit their text
            // color like the time footer instead of warning orange.
            .map(|this| {
                if outgoing {
                    this.opacity(0.8)
                } else {
                    this.text_color(warning_text())
                }
            })
            .child(label)
    });
    // Phase B4: auto-delete countdown chip (`message.auto_delete_in`,
    // schema 1.8.67 line 3148) — "🗑 59m left", decaying on the same
    // 1-second tick as the self-destruct badge.
    let auto_delete_chip = message.auto_delete_chip(unix_ms_now()).map(|label| {
        div()
            .id(("auto-delete-chip", message.id.0 as u64))
            .mt_1()
            .text_xs()
            // Outgoing bubbles are accent-filled: inherit their text
            // color like the time footer instead of warning orange.
            .map(|this| {
                if outgoing {
                    this.opacity(0.8)
                } else {
                    this.text_color(warning_text())
                }
            })
            .child(label)
    });
    // A time footer exists and nothing renders after the body/caption: the
    // footer can share its last line instead of taking a row of its own.
    let fact_check = super::fact_check_block::fact_check_block(
        message,
        fact_check_expanded,
        revealed,
        look.font,
        session,
        cx,
    );
    let tail_empty = keyboard.is_none()
        && fact_check.is_none()
        && self_destruct_badge.is_none()
        && auto_delete_chip.is_none()
        && views.is_none()
        && signature.is_none()
        && reply_bar_el.is_none()
        && chip_row.is_none();
    // A message ending in a right-to-left line takes its time on a line of
    // its own (Telegram Desktop): that line ends at the bubble's left.
    let ends_rtl = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::Text(text) => quill::text::last_line_is_rtl(&text.text),
        MessageContent::Photo(photo) => quill::text::last_line_is_rtl(&photo.caption),
        MessageContent::Document(doc) => quill::text::last_line_is_rtl(&doc.caption),
        MessageContent::Animation(animation) => quill::text::last_line_is_rtl(&animation.caption),
        MessageContent::Video(video) => quill::text::last_line_is_rtl(&video.caption),
        MessageContent::VoiceNote(note) => quill::text::last_line_is_rtl(&note.caption),
        MessageContent::Audio(audio) => quill::text::last_line_is_rtl(&audio.caption),
        _ => false,
    };
    let reserve_footer = tail_empty && message.date > 0 && !ends_rtl;
    // M2: ephemeral content replaces the regular content for rendering
    // (bot-built flows show the ephemeral variant to the current user).
    // A lone animated emoji or big custom emoji: stickers, not text.
    let emoji_block = super::super::animated_emoji::message_block(
        message,
        emoji_media.as_ref(),
        sticker_frame.as_ref(),
        &animated_emoji,
        files,
        media_roots,
        session,
        cx,
    );
    let text_body = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::Text(_) if emoji_block.is_some() => None,
        MessageContent::Text(text) => Some(message_text_block(
            (message.chat_id.0, message.id.0 as u64),
            text,
            files,
            downloading,
            media_roots,
            session
                .as_ref()
                .map(|s| &s.stickers.emoji.custom_emoji_stickers),
            &animated_emoji,
            revealed,
            // Settings → Appearance: message font size.
            look.font,
            session.is_none_or(|s| s.settings.media_prefs.big_emoji),
            reserve_footer.then(|| footer_meta.reserve(footer_reserve(message.is_outgoing))),
            room,
            cx,
        )),
        // Slice bots-games: the game card is the message's primary
        // content (thumbnail/title/text/description + Play/Scores).
        MessageContent::Game(game) => Some(game_card(
            message.chat_id,
            message.id,
            game,
            session,
            files,
            downloading,
            media_roots,
            revealed,
            look.font,
            // The game card's padding and border.
            room.media(false) - bubble_width::PADDED_CHROME,
            cx,
        )),
        // `HistoryView::Call`: a card in an ordinary bubble.
        MessageContent::Call {
            is_video,
            discard_reason,
            duration,
        } => Some(call_card(
            message.id,
            message.is_outgoing,
            *is_video,
            discard_reason,
            *duration,
            message.date,
            super::super::calls::call_message_peer(session, message.chat_id).map(|(id, _)| id),
            look.plain,
            cx,
        )),
        _ => None,
    };
    // MED4: caption element + position (`show_caption_above_media`,
    // schema 1.8.67 lines 6117/6128). Above → bubble body; below →
    // inside `extra` right after the media (the previous code always
    // rendered the caption above the media).
    let caption_above_el: Option<AnyElement>;
    let caption_below_el: Option<AnyElement>;
    let caption_reserved: bool;
    {
        let caption = Some(bubble_width::caption_parts(&message.content))
            .filter(|(caption, _)| !caption.is_empty());
        let below = !caption_above_media(&message.content);
        let el = caption.map(|(caption_text, caption_entities)| {
            rich_text_reserving(
                caption_text,
                caption_entities,
                (message.chat_id.0, message.id.0 as u64),
                true,
                revealed,
                // Settings → Appearance: caption follows the message font size.
                look.font,
                &super::super::message_text::custom_emoji_paths(
                    caption_entities,
                    session.map(|s| &s.stickers.emoji.custom_emoji_stickers),
                    files,
                    media_roots,
                ),
                &HashMap::new(),
                (below && reserve_footer)
                    .then(|| footer_meta.reserve(footer_reserve(message.is_outgoing))),
                cx,
            )
        });
        caption_reserved = below && reserve_footer && el.is_some();
        if caption_above_media(&message.content) {
            caption_above_el = el;
            caption_below_el = None;
        } else {
            caption_above_el = None;
            caption_below_el = el;
        }
    }
    // A just-revealed spoiler: its cover fades out over the media.
    let cover_fade = super::super::spoiler_fx::media_cover(covered_content, reveal_key, revealed);
    let extra_media = match (extra_media, cover_fade) {
        (Some(media), Some(super::super::spoiler_fx::MediaCover::Fading(opacity))) => {
            // Built outside the animation layer: the cover fades under an
            // opacity the layer can't apply, so the slice draws its specks.
            let cover = super::super::anim_layer::with_layer(None, || {
                spoiler_cover(
                    message.id.0 as u64,
                    message.chat_id,
                    message.id,
                    covered_content,
                    files,
                    downloading,
                    (frame_w, frame_h),
                    corners,
                    cx,
                )
            });
            Some(fading_spoiler(media, cover, opacity))
        }
        (media, _) => media,
    };
    let extra_media = extra_media.or(emoji_block);
    // Photo/GIF/video bubbles without a reply or forward header: the
    // picture sits on a thin inset and a caption gets its own padding.
    let media_led = header.is_none()
        && extra_media.is_some()
        && leads_with_media(effective_content(
            &message.content,
            message.ephemeral.as_ref(),
        ));
    // Media with nothing under it shows its time on the picture.
    // Bubble-less media keeps it there even with reactions under it.
    let footer_overlay = media_led
        && caption_below_el.is_none()
        && (reserve_footer || (bubbleless && message.date > 0));
    let caption_below_el = caption_below_el.map(|caption| {
        if media_led {
            div().px_2().py_1().child(caption).into_any_element()
        } else {
            caption
        }
    });
    let extra_media = extra_media.map(|media| {
        crate::ui::forward_drag_source::media_block(session, message, media, media_led)
    });
    // Bubble-less media carries its time itself, so reactions under it
    // don't push the pill off the picture.
    let extra_media = extra_media.map(|media| {
        if !bubbleless {
            return media;
        }
        let pill = footer_overlay
            .then(|| message_footer_meta(&footer_meta))
            .flatten()
            .map(|footer| {
                let meta = footer_meta.mirrored();
                crate::ui::synthetic::media_footer_pill(
                    footer,
                    Some(std::rc::Rc::new(move || message_footer_meta(&meta))),
                )
            });
        div()
            .relative()
            .child(media)
            .children(pill)
            .into_any_element()
    });
    let extra_media_present = extra_media.is_some();
    let extra_is_empty =
        extra_media.is_none() && caption_below_el.is_none() && tail_empty && keyboard.is_none();
    #[allow(clippy::some_filter)]
    let extra = Some(
        div()
            .id(("bubble-extra", message.id.0 as u64))
            .flex()
            .flex_col()
            .when_some(extra_media, |this, media| this.child(media))
            // MED4: caption below the media.
            .when_some(caption_below_el, |this, el| this.child(el))
            .when_some(fact_check, |this, block| this.child(block))
            .when_some(self_destruct_badge, |this, badge| {
                this.child(badge.when(media_led, |badge| badge.px_2()))
            })
            .when_some(auto_delete_chip, |this, chip| {
                this.child(chip.when(media_led, |chip| chip.px_2()))
            })
            .when_some(keyboard, |this, keyboard| this.child(keyboard))
            .when_some(chip_row, |this, chips| this.child(chips))
            .into_any_element(),
    )
    .filter(|_| !extra_is_empty);
    // kit Phase 4: the shared kit-shell chrome, built fresh per divergent
    // branch below (`MessageChrome` isn't `Clone` — it holds elements).
    // The text body reserved trailing footer space (mirrors the condition
    // in `message_text_block`: a link card below the text ends the bubble).
    let text_reserved = reserve_footer
        && matches!(
            effective_content(&message.content, message.ephemeral.as_ref()),
            MessageContent::Text(text)
                if !text.link_preview.as_ref().is_some_and(|preview| {
                    !preview.show_above_text && preview.has_card()
                })
        );
    // The leading picture / video / GIF decides the bubble's width; the
    // footer only widens it when the media is tiny.
    let media_width = (extra_media_present && !emoji_only)
        .then(|| {
            single_media_width(
                effective_content(&message.content, message.ephemeral.as_ref()),
                caption_w,
                media_max,
            )
        })
        .flatten()
        .map(|width| media_content_width(width, info_width, media_max));
    let max_width = bubble_width::message_width_limit(cx, content, look.font, look.plain);
    let mut avatar_link = avatar_link(&sender_avatar, message, cx);
    let mut sender_status = sender_status;
    let mut chrome = |footer_inline: bool| {
        let mut chrome = message_chrome(
            sender.clone(),
            receipt,
            sender_avatar.clone(),
            message.date,
            message.pending,
        );
        // A spacer (no link) keeps its slot: taking it would drop the gutter.
        if let Some(link) = avatar_link.take()
            && let Some(avatar) = chrome.avatar.take()
        {
            chrome.avatar = Some(link.wrap(avatar));
        }
        chrome.sender_status = sender_status.take();
        chrome.footer = message_footer_meta(&footer_meta);
        if has_chips || bubbleless {
            // The reaction row or the media carries the time.
            chrome.footer = None;
        }
        chrome.footer_inline = footer_inline;
        chrome.footer_overlay = footer_overlay;
        if footer_overlay && chrome.footer.is_some() {
            let meta = footer_meta.mirrored();
            chrome.footer_rebuild = Some(std::rc::Rc::new(move || message_footer_meta(&meta)));
        }
        chrome.media_led = media_led;
        chrome.bubbleless = bubbleless;
        chrome.media_width = media_width;
        chrome.max_width = max_width;
        chrome.actions = more_btn.take();
        chrome.actions_span = actions_span;
        chrome.bottom_bar = reply_bar_el.take();
        chrome
    };
    // M2: `messageRichMessage` (schema 1.8.67, line 5143) renders its
    // `pageBlock*` list as a block stack; ephemeral content wins here too.
    let rich_body = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::RichMessage(rich) => Some(message_rich_block(
            (message.chat_id.0, message.id.0 as u64),
            message.chat_id,
            message.id,
            rich,
            revealed,
            // Settings → Appearance: message font size.
            look.font,
            cx,
        )),
        _ => None,
    };
    if let Some(rich_body) = rich_body {
        return session_bubble_rich(
            message.id.0 as u64,
            chrome(false),
            message.is_outgoing,
            rich_body,
            extra,
            header,
            // Settings → Appearance: font size + bubble/plain style.
            look,
        );
    }
    if let Some(text_body) = text_body {
        return session_bubble_rich(
            message.id.0 as u64,
            chrome(
                (text_reserved || matches!(content, MessageContent::Call { .. })) && extra_is_empty,
            ),
            message.is_outgoing,
            text_body,
            extra,
            header,
            // Settings → Appearance: font size + bubble/plain style.
            look,
        );
    }
    // MED4: caption above the media → bubble body. Caption below the
    // media already rides inside `extra` (after the media), so it falls
    // through to the quoted fallback with an empty body.
    if let Some(caption_el) = caption_above_el {
        return session_bubble_rich(
            message.id.0 as u64,
            chrome(false),
            message.is_outgoing,
            caption_el,
            extra,
            header,
            // Settings → Appearance: font size + bubble/plain style.
            look,
        );
    }
    session_bubble_quoted(
        message.id.0 as u64,
        chrome(caption_reserved),
        String::new(),
        message.is_outgoing,
        extra,
        header,
        // Settings → Appearance: font size + bubble/plain style.
        look,
    )
}
