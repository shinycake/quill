//! The card under a gift or giveaway service message.

use super::*;

/// The inner action's (sender, receiver) of a gift card message.
fn card_parties(action: &ServiceAction) -> (Option<MessageSender>, Option<MessageSender>) {
    match action {
        ServiceAction::Gift {
            sender, receiver, ..
        }
        | ServiceAction::UpgradedGift {
            sender, receiver, ..
        } => (*sender, *receiver),
        _ => (None, None),
    }
}

/// The card of a gift/giveaway service message, if it has one.
pub(in crate::ui) fn gift_card_of(
    content: &quill::telegram::envelope::MessageContent,
) -> Option<(&ServiceAction, &GiftCard)> {
    match content {
        quill::telegram::envelope::MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::WithCard { action, card } => Some((action.as_ref(), card.as_ref())),
            _ => None,
        },
        _ => None,
    }
}

/// The bubble card under a gift/giveaway pill (tdesktop service box).
pub(in crate::ui) fn gift_card_element(
    row_id: u64,
    inner: &ServiceAction,
    card: &GiftCard,
    session: Option<&Session>,
    files: &quill::state::FileMap,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let on_backdrop = card.backdrop.is_some();
    let fg = if on_backdrop {
        gpui_kit::white()
    } else {
        cx.theme().foreground
    };
    let sub_fg = if on_backdrop {
        gpui_kit::white().opacity(0.8)
    } else {
        muted
    };
    let fallback = match card.kind {
        GiftCardKind::Premium | GiftCardKind::GiftCode => "\u{2B50}",
        GiftCardKind::Stars => "\u{2B50}",
        GiftCardKind::Giveaway | GiftCardKind::GiveawayWinners => "\u{1F389}",
        _ => "\u{1F381}",
    };
    let me = session.and_then(|s| s.my_user_id);
    let (_, receiver) = card_parties(inner);
    let to_me = matches!(receiver, Some(MessageSender::User { user_id }) if Some(user_id) == me);
    let base = match card.backdrop {
        Some((center, edge)) => div().bg(linear_gradient(
            180.,
            linear_color_stop(rgb(center), 0.),
            linear_color_stop(rgb(edge), 1.),
        )),
        None => div().bg(cx.theme().secondary.opacity(0.85)),
    };
    base.id(("gift-card", row_id))
        .w(px(250.))
        .rounded_xl()
        .p_3()
        .flex()
        .flex_col()
        .items_center()
        .gap_1p5()
        .text_color(fg)
        .role(gpui_kit::Role::Group)
        .aria_label(format!("{}, {}", card.title, card.subtitle))
        .child(sticker_tile(
            ("gift-card-sticker", row_id),
            card.sticker.as_ref(),
            files,
            roots,
            110.,
            fallback,
        ))
        .child(
            div()
                .font_semibold()
                .text_center()
                .child(card.title.clone()),
        )
        .child(
            div()
                .text_sm()
                .text_center()
                .text_color(sub_fg)
                .child(card.subtitle.clone()),
        )
        .when(!card.text.is_empty(), |t| {
            t.child(div().text_sm().text_center().child(card.text.clone()))
        })
        .children(card.facts.iter().map(|(label, value)| {
            div()
                .w_full()
                .flex()
                .justify_between()
                .text_xs()
                .child(div().text_color(sub_fg).child(label.clone()))
                .child(div().font_medium().child(value.clone()))
        }))
        .when(to_me && !card.received_gift_id.is_empty(), |t| {
            t.child(
                Button::new(("gift-card-view", row_id))
                    .label(tr(keys::Q_MONEY_VIEW_MY_GIFTS))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.open_my_gifts(cx))),
            )
        })
        .into_any_element()
}

crate::ui::shell::register_dialogs! {
    /// Stars balance and transaction history.
    Stars => DialogSpec::new(
        3800,
        |app| app.session().is_some_and(|s| s.payments.hub.stars_open),
        QuillApp::build_stars_dialog,
    )
    .closes(|app, _, cx| app.close_stars(cx)),

    /// Received gifts of a user or channel.
    ReceivedGifts => DialogSpec::new(
        3900,
        |app| app.session().is_some_and(|s| s.payments.hub.gifts_open),
        QuillApp::build_gifts_dialog,
    ),

    /// Read-only Premium features explainer.
    PremiumFeatures => DialogSpec::new(
        4000,
        |app| app.session().is_some_and(|s| s.payments.hub.premium_open),
        QuillApp::build_premium_dialog,
    )
    .closes(|app, _, cx| app.close_premium(cx)),
}
