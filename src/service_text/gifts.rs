//! Gift, upgraded-gift and giveaway wording: the cards' one-line text
//! (chat list, accessibility), following Telegram Desktop's
//! `lng_action_gift_*` and `lng_action_giveaway_*` keys.
use super::*;
use crate::telegram::envelope::GiftOfferState;

pub(super) fn giveaway_results(winners: i32, stars: bool, unclaimed: i32) -> ServiceText {
    if winners == 0 {
        return B::default()
            .t(tr(keys::ACTION_GIVEAWAY_RESULTS_NONE))
            .done();
    }
    if unclaimed > 0 {
        return B::default()
            .t(tr(if stars {
                keys::ACTION_GIVEAWAY_RESULTS_CREDITS_SOME
            } else {
                keys::ACTION_GIVEAWAY_RESULTS_SOME
            }))
            .done();
    }
    phrase_plural(
        if stars {
            keys::ACTION_GIVEAWAY_RESULTS_CREDITS
        } else {
            keys::ACTION_GIVEAWAY_RESULTS
        },
        i64::from(winners),
        &[],
    )
    .done()
}

pub(super) fn render_gift(
    ctx: &ServiceCtx<'_>,
    sender: Option<MessageSender>,
    receiver: Option<MessageSender>,
    stars: i64,
    prepaid_upgrade: bool,
    from_auction: bool,
) -> ServiceText {
    let cost = stars_cost(stars);
    let me = |s: &Option<MessageSender>| matches!(s, Some(MessageSender::User { user_id }) if ctx.is_me_user(*user_id));
    let sent_by_me = me(&sender) || (sender.is_none() && ctx.actor_is_me());
    if stars == 0 {
        return if sent_by_me {
            say(keys::ACTION_GIFT_UNIQUE_SENT)
        } else {
            phrase(
                keys::ACTION_GIFT_UNIQUE_RECEIVED,
                &[("user", ctx.peer_of_gift())],
            )
            .done()
        };
    }
    if prepaid_upgrade {
        return if sent_by_me {
            let cost = B::from(cost);
            match receiver {
                Some(r) if !me(&receiver) => phrase(
                    keys::Q_SERVICE_GIFT_UPGRADE_FOR,
                    &[("cost", cost), ("name", ctx.peer(r))],
                )
                .done(),
                _ => phrase(keys::ACTION_GIFT_SENT_UPGRADE_SELF, &[("cost", cost)]).done(),
            }
        } else {
            phrase(
                keys::ACTION_GIFT_SENT_UPGRADE,
                &[("from", ctx.peer_of_gift()), ("cost", B::from(cost))],
            )
            .done()
        };
    }
    if sender.is_none() && !ctx.actor_is_me() {
        return line(keys::ACTION_GIFT_RECEIVED_ANONYMOUS, &[("cost", &cost)]);
    }
    if from_auction {
        return line(keys::ACTION_GIFT_SELF_AUCTION, &[("cost", &cost)]);
    }
    if sent_by_me && me(&receiver) {
        return line(keys::ACTION_GIFT_SELF_BOUGHT, &[("cost", &cost)]);
    }
    ctx.gift_line(&cost)
}

pub(super) fn render_upgraded_gift(
    ctx: &ServiceCtx<'_>,
    name: &str,
    sender: Option<MessageSender>,
    receiver: Option<MessageSender>,
    origin: GiftOrigin,
    price: Option<&Money>,
) -> ServiceText {
    let me = |s: &Option<MessageSender>| matches!(s, Some(MessageSender::User { user_id }) if ctx.is_me_user(*user_id));
    let actor_me = ctx.actor_is_me();
    match origin {
        GiftOrigin::Upgrade | GiftOrigin::PrepaidUpgrade => {
            if actor_me && (me(&sender) || sender.is_none()) {
                say(keys::ACTION_GIFT_UPGRADED_SELF)
            } else if actor_me {
                phrase(
                    keys::ACTION_GIFT_UPGRADED_MINE,
                    &[(
                        "user",
                        match sender {
                            Some(s) => ctx.peer(s),
                            None => B::from(someone()),
                        },
                    )],
                )
                .done()
            } else {
                phrase(keys::ACTION_GIFT_UPGRADED, &[("user", ctx.from())]).done()
            }
        }
        GiftOrigin::Transfer | GiftOrigin::Blockchain => {
            if actor_me {
                match receiver {
                    Some(r) if !me(&receiver) => {
                        phrase(keys::ACTION_GIFT_TRANSFERRED_MINE, &[("user", ctx.peer(r))]).done()
                    }
                    _ => B::default()
                        .t(tr(keys::ACTION_GIFT_TRANSFERRED_SELF))
                        .done(),
                }
            } else if ctx.actor.is_some() {
                phrase(keys::ACTION_GIFT_TRANSFERRED, &[("user", ctx.from())]).done()
            } else {
                B::default()
                    .t(tr(keys::ACTION_GIFT_TRANSFERRED_UNKNOWN))
                    .done()
            }
        }
        GiftOrigin::Resale => {
            let cost = price.map(cost).unwrap_or_default();
            if actor_me || me(&sender) {
                line(keys::ACTION_GIFT_SENT_SOLD, &[("cost", &cost)])
            } else {
                phrase(
                    keys::ACTION_GIFT_RECEIVED_SOLD,
                    &[("user", ctx.from()), ("cost", B::from(cost))],
                )
                .done()
            }
        }
        GiftOrigin::Offer => {
            let cost = price.map(cost).unwrap_or_default();
            if actor_me {
                line(
                    keys::Q_SERVICE_GIFT_SOLD_NAMED,
                    &[("name", name), ("cost", &cost)],
                )
            } else {
                phrase(
                    keys::Q_SERVICE_GIFT_SOLD_NAMED_OTHER,
                    &[
                        ("user", ctx.from()),
                        ("name", B::from(name)),
                        ("cost", B::from(cost)),
                    ],
                )
                .done()
            }
        }
        GiftOrigin::Craft => say(keys::ACTION_GIFT_CRAFTED),
    }
}

/// `messageUpgradedGiftOnChainPurchaseOffer` (TDLib 1.8.71): the offer's
/// line by who made it, then its outcome. No action goes with it: an
/// on-chain offer is answered with TON, which Quill never moves.
pub(super) fn render_on_chain_offer(
    name: &str,
    price: &Money,
    state: GiftOfferState,
    is_buyer: bool,
) -> ServiceText {
    match state {
        GiftOfferState::Accepted => say(keys::ACTION_GIFT_OFFER_STATE_ACCEPTED),
        GiftOfferState::Rejected => say(keys::ACTION_GIFT_OFFER_STATE_REJECTED),
        GiftOfferState::Pending | GiftOfferState::Unknown if is_buyer => phrase(
            keys::ACTION_GIFT_OFFER_YOU,
            &[("cost", B::from(cost(price))), ("name", B::from(name))],
        )
        .done(),
        GiftOfferState::Pending | GiftOfferState::Unknown => phrase(
            keys::ACTION_GIFT_OFFER_INCOMING,
            &[("amount", B::from(cost(price)))],
        )
        .done(),
    }
}
