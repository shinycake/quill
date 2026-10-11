//! Contact cards, paid media and dice.

use super::*;

/// What the viewer knows about a shared contact's Telegram account, which
/// picks the card's buttons (tdesktop `HistoryView::Contact`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum ContactCardState {
    /// The card's user is the current user.
    SelfCard,
    /// A Telegram user that is already in the address book.
    Known,
    /// A Telegram user that is not in the address book.
    Unknown,
    /// Only a phone number: no Telegram account to message.
    PhoneOnly,
}

impl ContactCardState {
    pub(in crate::ui) fn of(user_id: i64, session: Option<&Session>) -> Self {
        if user_id == 0 {
            return Self::PhoneOnly;
        }
        match session {
            Some(s) if s.my_user_id == Some(user_id) => Self::SelfCard,
            Some(s) if s.user(user_id).is_some_and(|user| user.is_contact) => Self::Known,
            _ => Self::Unknown,
        }
    }

    /// Whether the card offers "Message".
    pub(in crate::ui) fn can_message(self) -> bool {
        matches!(self, Self::Known | Self::Unknown)
    }

    /// The second button: "View contact" for address-book entries,
    /// "Add contact" for the rest, none for the user's own card.
    pub(in crate::ui) fn second_action(self) -> Option<&'static str> {
        match self {
            Self::SelfCard => None,
            Self::Known => Some("View contact"),
            Self::Unknown | Self::PhoneOnly => Some("Add contact"),
        }
    }
}

/// Phase 4.3: `messageContact` card — avatar, display name, phone number
/// and the tdesktop action buttons: Message (chat with the account), View
/// contact (profile of an address-book entry) or Add contact (opens the
/// add-contact dialog prefilled from the card). The phone number is
/// display-only: `tel:` URLs are refused by `open_external_url`'s scheme
/// gate anyway. The vCard is kept in the model but not rendered.
pub(in crate::ui) fn contact_row(
    row_id: u64,
    contact: &quill::telegram::envelope::ContactContent,
    state: ContactCardState,
    photo: Option<PathBuf>,
    // The plain look: the msgMaxWidth cap with no bubble padding to leave out.
    plain: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let name = contact.display_name();
    let shown = if name.is_empty() { "Contact" } else { &name };
    let user_id = contact.user_id;
    let (phone, first, last) = (
        contact.phone_number.clone(),
        contact.first_name.clone(),
        contact.last_name.clone(),
    );
    let header = div()
        .flex()
        .items_center()
        .gap_2()
        .child(crate::ui::message_text::kit_avatar_element(
            shown,
            photo.as_deref(),
            px(40.),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .truncate()
                        .child(shown.to_string()),
                )
                .when(!contact.phone_number.is_empty(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .whitespace_nowrap()
                            .child(contact.phone_number.clone()),
                    )
                }),
        );
    let message_button = state.can_message().then(|| {
        Button::new(format!("contact-message-{row_id}"))
            .label("Message")
            .small()
            .primary()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_user_chat(user_id, window, cx);
            }))
    });
    let second_button = state.second_action().map(|label| {
        Button::new(format!("contact-second-{row_id}"))
            .label(label)
            .small()
            .ghost()
            .on_click(cx.listener(move |this, _, window, cx| {
                if state == ContactCardState::Known {
                    this.open_avatar_profile(
                        quill::telegram::envelope::MessageSender::User { user_id },
                        window,
                        cx,
                    );
                } else {
                    this.open_add_contact_dialog_for(user_id, &phone, &first, &last, window, cx);
                }
            }))
    });
    // Telegram Desktop `Contact::countOptimalSize`: as wide as the name,
    // phone and buttons need, no floor; the bubble caps it at msgMaxWidth.
    div()
        .id(("contact-row", row_id))
        .flex()
        .flex_col()
        .max_w(crate::ui::history::bubble_width::file_row_bounds(plain).1)
        .gap_2()
        .mt_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(text_muted())
        .bg(bg_subtle())
        .child(header)
        .when(
            message_button.is_some() || second_button.is_some(),
            |this| {
                this.child(
                    div()
                        .flex()
                        .gap_2()
                        .children(message_button)
                        .children(second_button),
                )
            },
        )
        .into_any_element()
}

/// Widest a locked paid-media card grows; the preview keeps the first
/// item's aspect ratio inside `PAID_MEDIA_MIN_ASPECT..=PAID_MEDIA_MAX_ASPECT`
/// (width / height) like a photo bubble.
const PAID_MEDIA_WIDTH: f32 = 260.0;
const PAID_MEDIA_MIN_ASPECT: f32 = 0.75;
const PAID_MEDIA_MAX_ASPECT: f32 = 1.6;

/// Card height for a paid-media preview of `width` x `height` pixels.
pub(in crate::ui) fn paid_media_height(width: i32, height: i32) -> f32 {
    let aspect = if width > 0 && height > 0 {
        (width as f32 / height as f32).clamp(PAID_MEDIA_MIN_ASPECT, PAID_MEDIA_MAX_ASPECT)
    } else {
        1.0
    };
    (PAID_MEDIA_WIDTH / aspect).round()
}

/// `messagePaidMedia` while still locked (tdesktop `HistoryView::Invoice`
/// with a media preview): the blurred inline thumbnail with a lock and the
/// Stars price, an item count for albums and the video length. Unlocking
/// (`payForPaidMedia`) is not offered yet, so the card is informative.
pub(in crate::ui) fn paid_media_card(
    row_id: u64,
    stars: i64,
    locked: &[quill::telegram::envelope::PaidMediaPreview],
    caption: &str,
) -> AnyElement {
    let first = locked.first();
    let height = px(paid_media_height(
        first.map_or(0, |p| p.width),
        first.map_or(0, |p| p.height),
    ));
    let width = px(PAID_MEDIA_WIDTH);
    let mut tile = div()
        .id(("paid-media", row_id))
        .relative()
        .w(width)
        .h(height)
        .overflow_hidden()
        .rounded_md()
        .bg(fill_muted())
        .role(gpui_kit::Role::Image)
        .aria_label(format!("Paid media, {stars} Stars, locked"));
    if let Some(mini) = first
        .and_then(|p| p.minithumbnail.as_ref())
        .filter(|mini| !mini.data.is_empty())
    {
        tile = tile.child(
            img(ImageSource::Image(Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Jpeg,
                mini.data.clone(),
            ))))
            .size_full()
            .object_fit(ObjectFit::Cover),
        );
    }
    // Over the picture the colors are fixed (they sit on media, not on
    // the theme), like the video play badge and time pill.
    let pill = |child: AnyElement| {
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_0p5()
            .rounded_full()
            .bg(gpui_kit::black().opacity(0.55))
            .text_color(gpui_kit::white())
            .text_xs()
            .child(child)
    };
    let price = if stars == 1 {
        "1 Star".to_string()
    } else {
        format!("{stars} Stars")
    };
    tile = tile
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .size(px(44.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(gpui_kit::black().opacity(0.55))
                                .text_color(gpui_kit::white())
                                .child(Icon::new(gpui_kit::assets::IconName::Lock)),
                        )
                        .child(pill(
                            div()
                                .font_medium()
                                .child(format!("Unlock for {price}"))
                                .into_any_element(),
                        )),
                ),
        )
        .when(locked.len() > 1, |this| {
            this.child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .child(pill(format!("{} items", locked.len()).into_any_element())),
            )
        });
    if let Some(duration) = first.map(|p| p.duration).filter(|d| *d > 0) {
        tile = tile.child(
            div()
                .absolute()
                .top_2()
                .left_2()
                .child(pill(format_voice_duration(duration).into_any_element())),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(tile)
        .when(!caption.is_empty(), |this| {
            this.child(div().text_sm().child(caption.to_string()))
        })
        .into_any_element()
}

/// Phase 4.4: `messageDice` row. A regular dice plays its landing
/// animation once (`diceStickersRegular` final state, drawn like an
/// animated sticker) and rests on the last frame, which shows the rolled
/// value. A slot machine stacks its five stickers the way Telegram
/// Desktop does (`history_view_slot_machine.cpp`): background, three
/// reels, lever, each resting on its last frame, which shows the symbol.
/// Until the animation decodes the emoji face stands in. The line under
/// the picture states the result in words.
pub(in crate::ui) fn dice_row(
    row_id: u64,
    dice: &quill::telegram::envelope::DiceContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    animated: Option<crate::ui::sticker_playback::AnimatedVisual>,
    layers: Vec<Option<crate::ui::sticker_playback::AnimatedVisual>>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let picture = if dice.slot_layers.is_empty() {
        match &dice.final_sticker {
            Some(sticker) => sticker_attachment(
                row_id,
                sticker,
                files,
                downloading,
                media_roots,
                animated,
                cx,
            ),
            None => div()
                .text_size(px(64.0))
                .child(dice.face().to_string())
                .into_any_element(),
        }
    } else {
        slot_machine(row_id, dice, files, downloading, media_roots, layers, cx)
    };
    div()
        .id(("dice-row", row_id))
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .px_3()
        .py_1()
        .child(picture)
        .child(
            div()
                .id(("dice-result", row_id))
                .role(Role::Label)
                .text_sm()
                .font_medium()
                .child(dice.result_line()),
        )
        .into_any_element()
}

/// The slot machine's layers stacked in one 128 px square, bottom first.
/// A layer without its animation yet shows its still, so the machine is
/// whole from the first frame.
fn slot_machine(
    row_id: u64,
    dice: &quill::telegram::envelope::DiceContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    layers: Vec<Option<crate::ui::sticker_playback::AnimatedVisual>>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut layers = layers.into_iter();
    let stack = div()
        .id(("slot-machine", row_id))
        .role(Role::Image)
        .aria_label(format!("Slot machine, {}", dice.result_line()))
        .relative()
        .mt_2()
        .flex_none()
        .size(px(128.));
    dice.slot_layers
        .iter()
        .enumerate()
        .fold(stack, |stack, (index, sticker)| {
            let animated = layers.next().flatten();
            let layer_id = row_id.wrapping_mul(8).wrapping_add(index as u64);
            // The still path carries the sticker's own top margin; the
            // stack has none to spare, so pull it back.
            let layer = sticker_attachment(
                layer_id,
                sticker,
                files,
                downloading,
                media_roots,
                animated,
                cx,
            );
            stack.child(
                div()
                    .absolute()
                    .top(px(-8.))
                    .left_0()
                    .size(px(128.))
                    .child(layer),
            )
        })
        .into_any_element()
}
