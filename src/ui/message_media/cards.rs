//! Contact cards and dice.

use super::*;
use quill::lang::{keys, tr};

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
            Self::Known => Some(tr(keys::Q_MISC_VIEW_CONTACT)),
            Self::Unknown | Self::PhoneOnly => Some(tr(keys::Q_MISC_ADD_CONTACT_2)),
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
    // The bubble's room: the msgMaxWidth cap (or a narrow history's), with
    // no bubble padding to leave out in the plain look.
    room: crate::ui::history::bubble_width::BubbleRoom,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let name = contact.display_name();
    let shown = if name.is_empty() {
        tr(keys::Q_MISC_CONTACT)
    } else {
        &name
    };
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
            .label(tr(keys::PROFILE_ACTION_SHORT_MESSAGE))
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
        .max_w(crate::ui::history::bubble_width::file_row_bounds(room).1)
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
