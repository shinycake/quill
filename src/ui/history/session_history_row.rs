//! Methods moved out of `history.rs` to keep files under 1000 lines.

use super::*;

pub(in crate::ui) fn session_history_row(
    message: &HistoryMessage,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    failed: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    // kit Phase 4: per-row chrome (sender header / outbox receipt /
    // avatar) computed at row-build time so the virtualized row doesn't
    // carry the chat.
    sender: Option<SenderLabel>,
    receipt: OutboxReceipt,
    sender_avatar: Option<(String, Option<PathBuf>)>,
    reply_header: Option<quill::state::ReplyHeader>,
    forward_header: Option<quill::state::ForwardHeader>,
    via_bot: Option<String>,
    // Seek-bar view for audio/voice rows (`None` for other content).
    seek_bar: Option<SeekBarView>,
    animation_playing: bool,
    animation_frame: Option<Arc<RenderImage>>,
    sticker_frame: Option<super::super::sticker_playback::AnimatedVisual>,
    // A slot machine's five layers, bottom to top (empty for other rows).
    dice_layers: Vec<Option<super::super::sticker_playback::AnimatedVisual>>,
    // Decoded animations of the message's custom emoji (by custom emoji id).
    animated_emoji: HashMap<i64, super::super::sticker_playback::AnimatedVisual>,
    video_playing: bool,
    video_frame: Option<PathBuf>,
    // The row's clip playing inline (muted autoplay), if any.
    inline: Option<super::super::inline_video::InlineFrame>,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Phase B4: whether the row's chat is a secret chat — selects the
    // "Self-destruct" vs "Auto-delete" service-row wording.
    _is_secret: bool,
    // Phase C2i: session for `messageCall` peer resolution ("Call
    // again" only for 1:1 chats).
    session: Option<&Session>,
    // Settings → Appearance: font size + bubble/plain style.
    look: BubbleLook,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    // Stickers, round video messages and emoji-only messages (the ones
    // drawn big) stand on their own, without a bubble, as in Telegram
    // Desktop. Their time then sits on the chat background, so it takes
    // the theme's text color instead of the bubble's.
    let emoji_only = match &message.content {
        MessageContent::Text(text) => {
            text.link_preview.is_none()
                && session.is_none_or(|s| s.settings.media_prefs.big_emoji)
                && quill::emoji_catalog::big_emoji_count_with_entities(&text.text, &text.entities)
                    .is_some()
        }
        _ => false,
    };
    let look = if emoji_only
        || matches!(
            message.content,
            MessageContent::Sticker(_) | MessageContent::VideoNote(_)
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
    if super::super::service_row::is_service_row(&message.content) {
        return super::super::service_row::service_message_row(
            message,
            session,
            files,
            media_roots,
            cx,
        );
    }
    // Slice G9: community service rows (`messageChatAddedToCommunity`,
    // `messageChatRemovedFromCommunity`, `messageChatJoinFromCommunity`,
    // schema 1.8.67 lines 5353–5363) — centered neutral notices, no
    // bubble, no reply/react/edit/delete controls. Wording is TGX
    // verbatim (strings.xml: ActionChatAddedToCommunity{,Unknown} /
    // ActionChatRemovedFromCommunity / group_user_join_from_community*).
    // The community name resolves from the session's `updateCommunity`
    // cache; unknown communities fall back to the nameless TGX forms.
    let community_name = |community_id: i64| {
        session.and_then(|s| {
            s.groups
                .communities
                .get(&community_id)
                .map(|c| c.name.clone())
        })
    };
    let community_service_row = |text: String| {
        service_pill(("community-service-row", message.id.0 as u64), text, cx).into_any_element()
    };
    if let MessageContent::ChatAddedToCommunity { community_id } = &message.content {
        let text = match community_name(*community_id) {
            Some(name) => format!("This chat was added to community \"{name}\""),
            None => "This chat was added to community".to_string(),
        };
        return community_service_row(text);
    }
    if matches!(message.content, MessageContent::ChatRemovedFromCommunity) {
        return community_service_row("This chat was removed from community".to_string());
    }
    if let MessageContent::ChatJoinFromCommunity { community_id } = &message.content {
        let community = community_name(*community_id);
        let text = match (message.is_outgoing, community) {
            (true, Some(name)) => {
                format!("You joined the group from the community \"{name}\"")
            }
            (true, None) => "You joined the group from the community".to_string(),
            (false, Some(name)) => format!(
                "{} joined the group from the community \"{name}\"",
                sender
                    .as_ref()
                    .map_or("Someone", |label| label.name.as_str())
            ),
            (false, None) => format!(
                "{} joined the group from the community",
                sender
                    .as_ref()
                    .map_or("Someone", |label| label.name.as_str())
            ),
        };
        return community_service_row(text);
    }
    // Phase C2f: `messageGroupCall` invitation service row (schema
    // 1.8.67, line 5288) — incoming and pending: Accept / Decline.
    if let MessageContent::GroupCallInvitation {
        is_active,
        was_missed,
        is_video,
        ..
    } = &message.content
    {
        return QuillApp::group_call_invitation_row(
            message,
            *is_active,
            *was_missed,
            *is_video,
            cx,
        )
        .into_any_element();
    }
    // Phase C2i: `messageCall` service row (schema 1.8.67, line 5277)
    // — reason-aware label + "Call again" for 1:1 chats.
    if let MessageContent::Call {
        is_video,
        discard_reason,
        duration,
    } = &message.content
    {
        return QuillApp::call_message_row(
            message,
            *is_video,
            discard_reason,
            *duration,
            session,
            cx,
        )
        .into_any_element();
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
            format!(
                "Original: {}",
                super::super::message_text::format_unix_date_time(header.original_date.into())
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
    let chat_id = message.chat_id;
    let message_id = message.id;
    let (more_btn, actions_span) =
        message_corner_actions(chat_id, message_id, message, session, cx);
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
    let footer_meta = FooterMeta::of(message, receipt, views, signature.clone());
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
    let has_chips = !chips.is_empty();
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
    let chip_row = (!chips.is_empty()).then(|| {
        let mut row = div()
            .id(("reaction-chips", message_id.0 as u64))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .mt_1();
        for (index, chip) in chips.into_iter().enumerate() {
            let Some(choice) = quill::state::ReactionChoice::from_type(&chip.reaction_type) else {
                continue;
            };
            let count = chip.total_count;
            let chosen = chip.is_chosen;
            let reactors: Vec<(String, Option<PathBuf>)> =
                if !in_channel && count <= 3 && chip.recent_senders.len() == count as usize {
                    {
                        chip.recent_senders
                            .iter()
                            .map(|sender| reactor_avatar(sender, session, media_roots))
                            .collect()
                    }
                } else {
                    Default::default()
                };
            let glyph: AnyElement = match &choice {
                quill::state::ReactionChoice::Emoji(emoji) => div()
                    .child(super::super::reactions::emoji_presentation(emoji))
                    .into_any_element(),
                quill::state::ReactionChoice::CustomEmoji(id) => {
                    custom_emoji_chip_glyph(*id, session, files, media_roots)
                }
            };
            let label = match &choice {
                quill::state::ReactionChoice::Emoji(emoji) => format!("{emoji} {count}"),
                quill::state::ReactionChoice::CustomEmoji(_) => format!("Custom emoji {count}"),
            };
            let tag_type = chip.reaction_type.clone();
            // Hover names who reacted; right-click opens the full list.
            let hover_text = (!are_tags).then(|| {
                let names: Vec<String> = chip
                    .recent_senders
                    .iter()
                    .map(|sender| reactor_avatar(sender, session, media_roots).0)
                    .collect();
                quill::reaction_who::tooltip_text(&names, count)
            });
            let list_type = quill::reaction_who::chip_opens_list(can_list_reactors, are_tags)
                .then(|| chip.reaction_type.clone());
            let chip_el = div()
                .id(("reaction-chip", message_id.0 as u64 * 64 + index as u64))
                .when_some(hover_text.filter(|text| !text.is_empty()), |this, text| {
                    this.tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
                    })
                })
                .when_some(list_type, |this, reaction| {
                    this.on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_reactors_menu(
                                MessageMenuState {
                                    chat_id,
                                    message_id,
                                    position: event.position,
                                },
                                reaction.clone(),
                                window,
                                cx,
                            );
                        }),
                    )
                })
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .rounded_full()
                .text_xs()
                .role(gpui_kit::Role::Button)
                .aria_label(label)
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .when(chosen, |this| {
                    this.bg(accent_strong())
                        .text_color(text_on_fill())
                        .border_1()
                        .border_color(accent())
                })
                .when(!chosen, |this| {
                    this.bg(bg_subtle())
                        .text_color(text_primary())
                        .border_1()
                        .border_color(text_muted())
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_reaction(chat_id, message_id, choice.clone(), cx);
                }))
                .child(glyph)
                .map(|this| {
                    if are_tags {
                        this
                    } else if reactors.is_empty() {
                        this.child(count.to_string())
                    } else {
                        // Overlapping 18 px avatars of who reacted.
                        this.child(div().flex().items_center().children(
                            reactors.iter().enumerate().map(|(ix, (name, photo))| {
                                div()
                                    .when(ix > 0, |this| this.ml(px(-5.)))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(bg_subtle())
                                    .child(super::super::message_text::kit_avatar_element(
                                        name,
                                        photo.as_deref(),
                                        px(18.),
                                    ))
                            }),
                        ))
                    }
                });
            // A Saved Messages tag has its own menu (Filter by Tag, Add or
            // Edit Name, Remove Tag).
            row = row.child(if are_tags {
                let named = session.is_some_and(|s| {
                    s.threads
                        .saved
                        .tags
                        .iter()
                        .any(|entry| entry.tag == tag_type && !entry.label.is_empty())
                });
                let premium = session
                    .and_then(|s| s.my_user_id.and_then(|id| s.user(id)))
                    .is_some_and(|user| user.is_premium);
                super::super::saved_sublists::tag_chip_menu(
                    chip_el,
                    super::super::saved_sublists::TagChipMenu {
                        owner: cx.entity().downgrade(),
                        tag: tag_type,
                        choice: quill::state::ReactionChoice::from_type(&chip.reaction_type),
                        chat_id,
                        message_id,
                        named,
                        premium,
                    },
                )
            } else {
                chip_el.into_any_element()
            });
        }
        // Telegram Desktop keeps the time on the reactions' line.
        if let Some(footer) = message_footer_meta(&footer_meta) {
            row = row.child(div().ml_auto().pl_2().child(footer));
        }
        row
    });
    // A spoiler stays covered until clicked; once revealed, it renders as
    // the plain media it is.
    let media_revealed =
        revealed.contains(&(message.chat_id.0, message.id.0 as u64, u64::MAX, false));
    // Media corners follow the bubble (Telegram Desktop `BubbleRounding`): a
    // picture that leads the bubble takes its radius on the edges that are
    // free of the sender name, a caption above/below, badges or reactions.
    let corners = {
        let media_led = header.is_none()
            && matches!(
                effective_content(&message.content, message.ephemeral.as_ref()),
                MessageContent::Photo(_) | MessageContent::Video(_) | MessageContent::Animation(_)
            );
        let has_caption = match &message.content {
            MessageContent::Photo(photo) => !photo.caption.is_empty(),
            MessageContent::Video(video) => !video.caption.is_empty(),
            MessageContent::Animation(animation) => !animation.caption.is_empty(),
            _ => false,
        };
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
    let extra_media = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::Photo(photo) if photo.has_spoiler && !media_revealed => {
            let (frame_w, frame_h) = photo
                .largest_size()
                .or_else(|| photo.thumb_size())
                .map(|size| media_frame(MediaFrameKind::Photo, size.width, size.height))
                .unwrap_or_else(|| media_frame(MediaFrameKind::Photo, 0, 0));
            Some(spoiler_cover(
                message.id.0 as u64,
                message.chat_id,
                message.id,
                photo.minithumbnail.as_ref(),
                photo.open_file_id(),
                frame_w,
                frame_h,
                corners,
                cx,
            ))
        }
        MessageContent::Video(video) if video.has_spoiler && !media_revealed => {
            let (frame_w, frame_h) = media_frame(MediaFrameKind::Video, video.width, video.height);
            Some(spoiler_cover(message.id.0 as u64, message.chat_id, message.id, None, None, frame_w, frame_h, corners, cx))
        }
        MessageContent::Animation(animation) if animation.has_spoiler && !media_revealed => {
            let (frame_w, frame_h) = media_frame(MediaFrameKind::Gif, animation.width, animation.height);
            Some(spoiler_cover(message.id.0 as u64, message.chat_id, message.id, None, None, frame_w, frame_h, corners, cx))
        }
        MessageContent::Photo(photo) => {
            let unveiled;
            let photo = if photo.has_spoiler {
                unveiled = quill::telegram::envelope::PhotoContent {
                    has_spoiler: false,
                    ..photo.clone()
                };
                &unveiled
            } else {
                photo
            };
            Some(photo_attachment(
                message.id.0 as u64,
                photo,
                files,
                downloading,
                media_roots,
                None,
                Some((message.chat_id, message.id)),
                corners,
                cx,
            ))
        }
        MessageContent::Document(doc) => Some(document_chip(
            message.id.0 as u64,
            doc,
            message.is_outgoing,
            files,
            downloading,
            failed,
            // Slice media-downloads-pause: the pause toggle only appears
            // for user-initiated (listed) downloads.
            session
                .filter(|s| s.media.user_downloads.contains(&doc.file_id.0))
                .map(|s| s.media.paused_downloads.contains(&doc.file_id.0)),
            None,
            cx,
        )),
        MessageContent::Sticker(sticker) => {
            let attachment = sticker_attachment(
                message.id.0 as u64,
                sticker,
                files,
                downloading,
                media_roots,
                sticker_frame,
                cx,
            );
            // A tap opens the sticker's set (tdesktop `StickerSetBox`).
            let set_id = sticker.set_id;
            Some(if set_id != 0 && !message.pending {
                div()
                    .id(("sticker-open-set", message.id.0 as u64))
                    .cursor_pointer()
                    .role(gpui_kit::Role::Button)
                    .aria_label("Open sticker set")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.view_message_sticker_set(set_id, cx);
                    }))
                    .child(attachment)
                    .into_any_element()
            } else {
                attachment
            })
        }
        MessageContent::VoiceNote(note) => Some(voice_note_row(
            message.chat_id,
            message.id,
            message.is_outgoing,
            note,
            files,
            downloading,
            seek_bar.as_ref().expect("voice row always has a seek view"),
            cx,
        )),
        MessageContent::Audio(audio) => Some(audio_row(
            message.id,
            message.is_outgoing,
            audio,
            files,
            downloading,
            media_roots,
            seek_bar.as_ref().expect("audio row always has a seek view"),
            cx,
        )),
        MessageContent::Animation(animation) => Some(animation_attachment(
            message.id,
            animation,
            files,
            downloading,
            media_roots,
            animation_playing,
            animation_frame,
            inline,
            None,
            Some((message.chat_id, message.id)),
            corners,
            cx,
        )),
        MessageContent::Video(video) => Some(video_attachment(
            message.id,
            video,
            files,
            downloading,
            media_roots,
            video_playing,
            video_frame.as_deref(),
            inline,
            None,
            Some((message.chat_id, message.id)),
            corners,
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
            video_playing,
            video_frame.as_deref(),
            inline,
            cx,
        )),
        MessageContent::Poll(poll) => Some(poll_body(message.chat_id, message.id, poll, cx)),
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
            cx,
        )),
        MessageContent::Dice(dice) => Some(dice_row(
            message.id.0 as u64,
            dice,
            files,
            downloading,
            media_roots,
            sticker_frame,
            dice_layers,
            cx,
        )),
        MessageContent::Action(_) if super::super::service_row::locked_paid_media(&message.content).is_some() => {
            super::super::service_row::locked_paid_media(&message.content)
                .map(|(stars, locked, caption)| paid_media_card(message.id.0 as u64, stars, locked, caption))
        }
        MessageContent::Action(_)
        | MessageContent::Text(_)
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
                    .aria_label("Quill cannot display this message. A newer release may support it.")
                    .child("Quill cannot display this message. A newer release may support it."))
                .child(Button::new(("unsupported-update", message.id.0 as u64))
                    .label("Get latest Quill")
                    .on_click(cx.listener(|_, _, _, cx| cx.open_url(quill::updater::RELEASES_URL))))
                .into_any_element(),
        ),
    };
    let fast_buttons = session.is_some_and(|session| {
        session
            .fast_button_target(message.chat_id)
            .is_some_and(|target| {
                target.message_id == message.id && quill::fast_buttons::is_enabled(target.bot_id)
            })
    });
    let keyboard = inline_keyboard(message, fast_buttons, cx);
    // Phase B3: self-destruct timer badge (`message.self_destruct_type` /
    // `message.self_destruct_in`, schema 1.8.67 lines 3146–3147). The
    // countdown decays locally against `unix_ms_now()` (same pattern as
    // the Phase A1 slow-mode countdown); TDLib removes the row via
    // `updateDeleteMessages` when the timer fires. The 1-second render
    // tick (see `ensure_self_destruct_tick`) keeps this fresh.
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
    let tail_empty = keyboard.is_none()
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
    let text_body = match effective_content(&message.content, message.ephemeral.as_ref()) {
        MessageContent::Text(text) => Some(message_text_block(
            (message.chat_id.0, message.id.0 as u64),
            text,
            files,
            downloading,
            media_roots,
            session
                .as_ref()
                .map(|s| s.stickers.emoji.custom_emoji_stickers.as_slice())
                .unwrap_or(&[]),
            &animated_emoji,
            revealed,
            // Settings → Appearance: message font size.
            look.font,
            session.is_none_or(|s| s.settings.media_prefs.big_emoji),
            reserve_footer.then(|| footer_meta.reserve(footer_reserve(message.is_outgoing))),
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
        let caption: Option<(&str, &[TextEntity])> = match &message.content {
            MessageContent::Photo(photo) => (!photo.caption.is_empty())
                .then_some((photo.caption.as_str(), photo.caption_entities.as_slice())),
            MessageContent::Document(doc) => (!doc.caption.is_empty())
                .then_some((doc.caption.as_str(), doc.caption_entities.as_slice())),
            MessageContent::Animation(animation) => (!animation.caption.is_empty()).then_some((
                animation.caption.as_str(),
                animation.caption_entities.as_slice(),
            )),
            MessageContent::Video(video) => (!video.caption.is_empty())
                .then_some((video.caption.as_str(), video.caption_entities.as_slice())),
            MessageContent::VoiceNote(note) => (!note.caption.is_empty())
                .then_some((note.caption.as_str(), note.caption_entities.as_slice())),
            MessageContent::Audio(audio) => (!audio.caption.is_empty())
                .then_some((audio.caption.as_str(), audio.caption_entities.as_slice())),
            _ => None,
        };
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
                // Captions don't resolve custom emoji in this slice (text fallback).
                &HashMap::new(),
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
    let reveal_key = (message.chat_id.0, message.id.0 as u64, u64::MAX, false);
    let extra_media = match (
        extra_media,
        media_revealed
            .then(|| super::super::spoiler_fx::reveal_fade(reveal_key))
            .flatten(),
    ) {
        (Some(media), Some(cover_opacity)) => {
            // Built outside the animation layer: the cover fades under an
            // opacity the layer can't apply, so the slice draws its specks.
            let cover = super::super::anim_layer::with_layer(None, || {
                match effective_content(&message.content, message.ephemeral.as_ref()) {
                    MessageContent::Photo(photo) if photo.has_spoiler => {
                        let (frame_w, frame_h) = photo
                            .largest_size()
                            .or_else(|| photo.thumb_size())
                            .map(|size| media_frame(MediaFrameKind::Photo, size.width, size.height))
                            .unwrap_or_else(|| media_frame(MediaFrameKind::Photo, 0, 0));
                        Some(spoiler_cover(
                            message.id.0 as u64,
                            message.chat_id,
                            message.id,
                            photo.minithumbnail.as_ref(),
                            None,
                            frame_w,
                            frame_h,
                            corners,
                            cx,
                        ))
                    }
                    MessageContent::Video(video) if video.has_spoiler => {
                        let (frame_w, frame_h) =
                            media_frame(MediaFrameKind::Video, video.width, video.height);
                        Some(spoiler_cover(
                            message.id.0 as u64,
                            message.chat_id,
                            message.id,
                            None,
                            None,
                            frame_w,
                            frame_h,
                            corners,
                            cx,
                        ))
                    }
                    MessageContent::Animation(animation) if animation.has_spoiler => {
                        let (frame_w, frame_h) =
                            media_frame(MediaFrameKind::Gif, animation.width, animation.height);
                        Some(spoiler_cover(
                            message.id.0 as u64,
                            message.chat_id,
                            message.id,
                            None,
                            None,
                            frame_w,
                            frame_h,
                            corners,
                            cx,
                        ))
                    }
                    _ => None,
                }
            });
            Some(match cover {
                Some(cover) => div()
                    .relative()
                    .child(media)
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .opacity(cover_opacity)
                            .child(cover),
                    )
                    .into_any_element(),
                None => media,
            })
        }
        (media, _) => media,
    };
    // Photo/GIF/video bubbles without a reply or forward header: the
    // picture sits on a thin inset and a caption gets its own padding.
    let media_led = header.is_none()
        && extra_media.is_some()
        && matches!(
            effective_content(&message.content, message.ephemeral.as_ref()),
            MessageContent::Photo(_) | MessageContent::Video(_) | MessageContent::Animation(_)
        );
    // Media with nothing under it shows its time on the picture.
    let footer_overlay = media_led && caption_below_el.is_none() && reserve_footer;
    let caption_below_el = caption_below_el.map(|caption| {
        if media_led {
            div().px_2().py_1().child(caption).into_any_element()
        } else {
            caption
        }
    });
    let extra_media = extra_media.map(|media| {
        if media_led {
            media
        } else {
            div().mt_2().child(media).into_any_element()
        }
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
            single_media_width(effective_content(
                &message.content,
                message.ephemeral.as_ref(),
            ))
        })
        .flatten()
        .map(|width| {
            media_content_width(
                width,
                footer_meta.reserve(footer_reserve(message.is_outgoing)),
            )
        });
    let mut more_btn = Some(more_btn);
    let mut avatar_link = avatar_link(&sender_avatar, message, cx);
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
        chrome.footer = message_footer_meta(&footer_meta);
        if has_chips {
            // The reaction row carries the time.
            chrome.footer = None;
        }
        chrome.footer_inline = footer_inline;
        chrome.footer_overlay = footer_overlay;
        if footer_overlay && chrome.footer.is_some() {
            let meta = footer_meta.clone();
            chrome.footer_rebuild = Some(std::rc::Rc::new(move || message_footer_meta(&meta)));
        }
        chrome.media_led = media_led;
        chrome.media_width = media_width;
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
            chrome(text_reserved && extra_is_empty),
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
