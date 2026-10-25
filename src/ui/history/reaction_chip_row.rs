//! The row of reaction chips (or Saved Messages tags) under a message.

use super::*;
use quill::lang::{keys, tr_with};
use quill::telegram::envelope::MessageReaction;

#[allow(clippy::too_many_arguments)]
pub(super) fn reaction_chip_row(
    message: &HistoryMessage,
    chips: &[&MessageReaction],
    are_tags: bool,
    can_list_reactors: bool,
    in_channel: bool,
    session: Option<&Session>,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
    footer_meta: &super::super::message_text::FooterMeta,
    // Under bubble-less media (`quill::media_bubble`): the chips stand on
    // the chat background in the service pill colors and leave the time to
    // the picture (tdesktop reactions without `Flag::InBubble`).
    outside: bool,
    cx: &mut Context<QuillApp>,
) -> Stateful<Div> {
    let outside_bg = super::super::chat_palette::service_bg(cx.theme().secondary.opacity(0.85));
    let unchosen_bg = if outside {
        outside_bg
    } else {
        bg_subtle().into()
    };
    let message_id = message.id;
    let chat_id = message.chat_id;
    let mut row = div()
        .id(("reaction-chips", message_id.0 as u64))
        .flex()
        .flex_wrap()
        .items_center()
        .gap_1()
        .mt_1();
    // The Star reaction leads the row (tdesktop sorts it first).
    if let Some(paid) =
        super::super::paid_reaction_chip::row_paid_reaction(message, session, !chips.is_empty())
    {
        row = row.child(super::super::paid_reaction_chip::paid_reaction_chip(
            message, &paid, cx,
        ));
    }
    for (index, chip) in chips.iter().enumerate() {
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
            quill::state::ReactionChoice::Emoji(emoji) => {
                let emoji = super::super::reactions::emoji_presentation(emoji);
                super::super::emoji_sprites::glyph(&emoji, px(super::super::reactions::CHIP_GLYPH))
                    .unwrap_or_else(|| div().child(emoji).into_any_element())
            }
            quill::state::ReactionChoice::CustomEmoji(id) => {
                custom_emoji_chip_glyph(*id, session, files, media_roots)
            }
        };
        let label = match &choice {
            quill::state::ReactionChoice::Emoji(emoji) => format!("{emoji} {count}"),
            quill::state::ReactionChoice::CustomEmoji(_) => {
                tr_with(keys::Q_MEDIA_CUSTOM_EMOJI, &[("count", &count.to_string())])
            }
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
            .when(!chosen && !outside, |this| {
                this.bg(unchosen_bg)
                    .text_color(text_primary())
                    .border_1()
                    .border_color(text_muted())
            })
            .when(!chosen && outside, |this| {
                this.bg(unchosen_bg)
                    .text_color(cx.theme().secondary_foreground)
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
                                .border_color(unchosen_bg)
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
    if !outside && let Some(footer) = message_footer_meta(footer_meta) {
        row = row.child(div().ml_auto().pl_2().child(footer));
    }
    row
}
