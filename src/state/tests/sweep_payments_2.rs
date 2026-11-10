//! State reducer tests: failures of payments, Stars and gift requests land
//! only on the request that asked (sweep s3-payments-2).
use super::common::*;
use super::*;
use crate::gift_market::{CraftSession, GiftMarketPurpose, ResaleFilter, ResaleSearch};
use crate::ids::RequestId;
use crate::state::fetch::Fetch;

fn fail(session: &mut Session, id: RequestId, sink: &Arc<MemorySink>) {
    thread_local! {
        static SEQ: AtomicU64 = const { AtomicU64::new(0) };
    }
    SEQ.with(|seq| {
        apply_json(
            session,
            seq,
            sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"BAD"}}"#,
                id.0
            ),
        )
    });
}

#[test]
fn an_older_stars_history_failure_leaves_the_live_request_loading() {
    let (mut session, sink) = session();
    let old = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append: false }),
        None,
    );
    let new = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append: false }),
        None,
    );
    session.payments.hub.tx_request = new.0;
    session.payments.hub.tx_loading = true;
    fail(&mut session, old, &sink);
    assert!(session.payments.hub.tx_loading);
    assert!(session.payments.hub.tx_error.is_none());
    fail(&mut session, new, &sink);
    assert!(!session.payments.hub.tx_loading);
    assert!(session.payments.hub.tx_error.is_some());
}

#[test]
fn an_older_received_gifts_failure_leaves_the_live_request_loading() {
    let (mut session, sink) = session();
    let old = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append: false }),
        None,
    );
    let new = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append: false }),
        None,
    );
    session.payments.hub.gifts_request = new.0;
    session.payments.hub.gifts_loading = true;
    fail(&mut session, old, &sink);
    assert!(session.payments.hub.gifts_loading);
    assert!(session.payments.hub.gifts_error.is_none());
    fail(&mut session, new, &sink);
    assert!(!session.payments.hub.gifts_loading);
    assert!(session.payments.hub.gifts_error.is_some());
}

#[test]
fn collections_of_another_owner_are_dropped() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let mine = MessageSender::User { user_id: 1 };
    let channel = MessageSender::Chat { chat_id: -100 };
    let first = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetGiftCollections { owner: mine }),
        None,
    );
    let second = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GetGiftCollections { owner: channel }),
        None,
    );
    // The channel's list is open now.
    session.payments.hub.gifts_owner = Some(channel);
    session.payments.gifts_manage.collections_loading = true;
    let list = |extra: RequestId, id: i32| {
        format!(
            r#"{{"@type":"giftCollections","@extra":"{}","collections":[{{"@type":"giftCollection","id":{id},"name":"Mine","icon":null,"gift_count":1}}]}}"#,
            extra.0
        )
    };
    apply_json(&mut session, &seq, &sink, &list(first, 7));
    assert!(session.payments.gifts_manage.collections.is_empty());
    assert!(session.payments.gifts_manage.collections_loading);
    fail(&mut session, first, &sink);
    assert!(session.payments.gifts_manage.collections_loading);
    apply_json(&mut session, &seq, &sink, &list(second, 9));
    assert_eq!(session.payments.gifts_manage.collections.len(), 1);
    assert!(!session.payments.gifts_manage.collections_loading);
}

#[test]
fn market_failures_hit_only_the_request_that_failed() {
    let (mut session, sink) = session();
    let search = |page| GiftMarketPurpose::SearchResale { gift_id: 5, page };
    let old = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GiftMarket(search(0))),
        None,
    );
    let new = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GiftMarket(search(0))),
        None,
    );
    let market = &mut session.payments.gift_market;
    let mut resale = ResaleSearch::new(5, ResaleFilter::default(), false);
    resale.loading = true;
    resale.request_id = new.0;
    market.resale = Some(resale);
    fail(&mut session, old, &sink);
    let resale = session.payments.gift_market.resale.as_ref().unwrap();
    assert!(
        resale.loading && resale.error.is_none(),
        "the newer search keeps loading"
    );
    fail(&mut session, new, &sink);
    let resale = session.payments.gift_market.resale.as_ref().unwrap();
    assert!(!resale.loading && resale.error.is_some());

    // Auction A fails after B was asked: B's box still opens.
    let a = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GiftMarket(
            GiftMarketPurpose::AuctionState { gift_id: 1 },
        )),
        None,
    );
    let b = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GiftMarket(
            GiftMarketPurpose::AuctionState { gift_id: 2 },
        )),
        None,
    );
    let market = &mut session.payments.gift_market;
    market.auction_request = b.0;
    market.auction_loading = true;
    fail(&mut session, a, &sink);
    let market = &session.payments.gift_market;
    assert!(market.auction_loading && market.notes.is_empty());
    fail(&mut session, b, &sink);
    assert!(!session.payments.gift_market.auction_loading);

    // Craft candidates of a closed craft box do not fail the next one.
    let candidates = |page| GiftMarketPurpose::GiftsForCrafting { gift_id: 5, page };
    let stale = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GiftMarket(candidates(0))),
        None,
    );
    let live = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GiftMarket(candidates(0))),
        None,
    );
    let mut craft = CraftSession::default();
    craft.candidates_loading = true;
    craft.candidates_request = live.0;
    session.payments.gift_market.craft = Some(craft);
    fail(&mut session, stale, &sink);
    let craft = session.payments.gift_market.craft.as_ref().unwrap();
    assert!(craft.candidates_loading && !craft.candidates_loaded);
    fail(&mut session, live, &sink);
    let craft = session.payments.gift_market.craft.as_ref().unwrap();
    assert!(!craft.candidates_loading && craft.candidates_loaded);
}

#[test]
fn acquired_gifts_and_preview_failures_say_so_and_can_be_asked_again() {
    let (mut session, sink) = session();
    let acquired = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GiftMarket(
            GiftMarketPurpose::AcquiredGifts { gift_id: 8 },
        )),
        None,
    );
    let preview = session.request(
        RequestPurpose::Payments(PaymentsPurpose::GiftMarket(
            GiftMarketPurpose::UpgradePreview { gift_id: 8 },
        )),
        None,
    );
    fail(&mut session, acquired, &sink);
    fail(&mut session, preview, &sink);
    let market = &session.payments.gift_market;
    assert!(matches!(
        market.acquired.get(&8),
        Some(Fetch::Failed { .. })
    ));
    assert!(matches!(
        market.previews.get(&8),
        Some(Fetch::Failed { .. })
    ));
}

#[test]
fn a_giveaway_launch_in_flight_survives_reopening_the_form() {
    use crate::state::GiveawayLaunch;
    let launching = GiveawayLaunch::Launching {
        chat_id: 5,
        request: 9,
    };
    assert_eq!(launching.clone().for_next_form(), launching);
    for seen in [
        GiveawayLaunch::Launched,
        GiveawayLaunch::Failed("x".into()),
        GiveawayLaunch::Idle,
    ] {
        assert_eq!(seen.for_next_form(), GiveawayLaunch::Idle);
    }
}
