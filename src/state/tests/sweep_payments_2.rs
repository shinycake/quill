//! State reducer tests: failures of payments, Stars and gift requests land
//! only on the request that asked (sweep s3-payments-2).
use super::common::*;
use super::*;
use crate::gift_market::{CraftSession, GiftMarketPurpose, PageLoad, ResaleFilter, ResaleSearch};
use crate::ids::RequestId;
use crate::premium_hub::page_stage::{PageKind, PageStage};
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
    session.payments.hub.tx = PageStage::Loading {
        request: new,
        kind: PageKind::First,
    };
    fail(&mut session, old, &sink);
    assert!(session.payments.hub.tx.is_loading());
    assert!(session.payments.hub.tx.error().is_none());
    fail(&mut session, new, &sink);
    assert!(!session.payments.hub.tx.is_loading());
    assert!(session.payments.hub.tx.error().is_some());
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
    session.payments.hub.gifts_page = PageStage::Loading {
        request: new,
        kind: PageKind::First,
    };
    fail(&mut session, old, &sink);
    assert!(session.payments.hub.gifts_page.is_loading());
    assert!(session.payments.hub.gifts_page.error().is_none());
    fail(&mut session, new, &sink);
    assert!(!session.payments.hub.gifts_page.is_loading());
    assert!(session.payments.hub.gifts_page.error().is_some());
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
    session
        .payments
        .gifts_manage
        .collections_fetch
        .begin(second);
    let list = |extra: RequestId, id: i32| {
        format!(
            r#"{{"@type":"giftCollections","@extra":"{}","collections":[{{"@type":"giftCollection","id":{id},"name":"Mine","icon":null,"gift_count":1}}]}}"#,
            extra.0
        )
    };
    apply_json(&mut session, &seq, &sink, &list(first, 7));
    assert!(session.payments.gifts_manage.collections.is_empty());
    assert!(session.payments.gifts_manage.collections_fetch.is_loading());
    fail(&mut session, first, &sink);
    assert!(session.payments.gifts_manage.collections_fetch.is_loading());
    apply_json(&mut session, &seq, &sink, &list(second, 9));
    assert_eq!(session.payments.gifts_manage.collections.len(), 1);
    assert!(!session.payments.gifts_manage.collections_fetch.is_loading());
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
    resale.load.begin(new.0, false);
    market.resale = Some(resale);
    fail(&mut session, old, &sink);
    let resale = session.payments.gift_market.resale.as_ref().unwrap();
    assert!(
        resale.load.is_loading() && resale.load.error().is_none(),
        "the newer search keeps loading"
    );
    fail(&mut session, new, &sink);
    let resale = session.payments.gift_market.resale.as_ref().unwrap();
    assert!(!resale.load.is_loading() && resale.load.error().is_some());

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
    market.auction = crate::gift_market::AuctionFetch::Loading { request: b.0 };
    fail(&mut session, a, &sink);
    let market = &session.payments.gift_market;
    assert!(
        matches!(
            market.auction,
            crate::gift_market::AuctionFetch::Loading { .. }
        ) && market.notes.is_empty()
    );
    fail(&mut session, b, &sink);
    assert_eq!(
        session.payments.gift_market.auction,
        crate::gift_market::AuctionFetch::Idle
    );

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
    session.payments.gift_market.craft = Some(CraftSession {
        candidates_load: PageLoad::Loading {
            request: live.0,
            more: false,
        },
        ..CraftSession::default()
    });
    fail(&mut session, stale, &sink);
    let craft = session.payments.gift_market.craft.as_ref().unwrap();
    assert!(craft.candidates_load.is_loading() && !craft.candidates_load.answered());
    fail(&mut session, live, &sink);
    let craft = session.payments.gift_market.craft.as_ref().unwrap();
    assert!(!craft.candidates_load.is_loading() && craft.candidates_load.answered());
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
fn a_giveaway_launch_in_flight_is_never_forgotten_by_a_form() {
    use crate::state::{GiveawayLaunch, GiveawayLaunches};
    let mut launches = GiveawayLaunches::default();
    launches.set(
        5,
        GiveawayLaunch::Launching {
            request: RequestId(9),
        },
    );
    launches.for_form(5);
    assert!(launches.is_launching_for(5));
    assert!(!launches.is_launching_for(6));
    for seen in [
        GiveawayLaunch::Launched,
        GiveawayLaunch::Paid,
        GiveawayLaunch::Failed { reason: "x".into() },
    ] {
        launches.set(5, seen);
        launches.for_form(5);
        assert_eq!(launches.stage(5), &GiveawayLaunch::Idle);
    }
}

/// 10-023: closing the form during the provider's verification, and opening
/// it again, keeps Start refused until the user ends the verification.
#[test]
fn a_giveaway_waiting_on_verification_survives_the_form_closing() {
    use crate::state::{GiveawayLaunch, GiveawayLaunches};
    let mut launches = GiveawayLaunches::default();
    launches.set(5, GiveawayLaunch::Verifying);
    launches.for_form(5);
    launches.for_form(5);
    assert!(launches.is_done_for(5) && launches.is_verifying_for(5));
    launches.end_verification(6);
    assert!(
        launches.is_verifying_for(5),
        "another chat's does not end it"
    );
    launches.end_verification(5);
    assert_eq!(launches.stage(5), &GiveawayLaunch::Idle);
}

/// 10-024: editing the form dismisses a failed launch's line, and only that.
#[test]
fn editing_the_form_dismisses_a_failed_launch_but_not_one_in_flight() {
    use crate::state::{GiveawayLaunch, GiveawayLaunches};
    let mut launches = GiveawayLaunches::default();
    launches.set(5, GiveawayLaunch::Failed { reason: "x".into() });
    launches.dismiss_failure(5);
    assert!(launches.failure_for(5).is_none());
    launches.set(
        5,
        GiveawayLaunch::Launching {
            request: RequestId(3),
        },
    );
    launches.dismiss_failure(5);
    assert!(launches.is_launching_for(5));
}

/// Each chat has its own launch stage: a second launch does not overwrite
/// the first, and each answer lands on the chat that asked.
#[test]
fn two_chats_launch_at_once_and_each_answer_lands_on_its_own_chat() {
    use crate::state::{GiveawayLaunch, GiveawayLaunches};
    let mut launches = GiveawayLaunches::default();
    launches.set(
        1,
        GiveawayLaunch::Launching {
            request: RequestId(10),
        },
    );
    launches.set(
        2,
        GiveawayLaunch::Launching {
            request: RequestId(11),
        },
    );
    assert!(launches.is_launching_for(1) && launches.is_launching_for(2));
    assert_eq!(launches.launching(RequestId(10)), Some(1));
    assert_eq!(launches.launching(RequestId(11)), Some(2));
    assert_eq!(launches.launching(RequestId(0)), None);
    launches.set(
        1,
        GiveawayLaunch::Failed {
            reason: "no".into(),
        },
    );
    assert_eq!(launches.failure_for(1), Some("no"));
    assert!(launches.is_launching_for(2), "B is still out");
}

#[test]
fn a_join_box_parked_for_a_bot_profile_goes_when_the_profile_closes() {
    use crate::earn::{EarnPeer, JoinBox};
    use crate::state::InfoPanelTarget;
    let (mut session, _sink) = session();
    let park = |session: &mut Session| {
        session.payments.earn.recipients = crate::earn::Recipients::Loading {
            waiting: Vec::new(),
            found: Vec::new(),
            join: Some(JoinBox {
                bot_user_id: 5,
                info: Default::default(),
                recipient: EarnPeer::User(1),
                choose_recipient: true,
                stage: crate::earn::JoinStage::Open,
            }),
        };
    };
    park(&mut session);
    // The same bot's profile staying open keeps it.
    session.set_open_info_panel(Some(InfoPanelTarget::User(5)));
    assert!(session.payments.earn.recipients.parked().is_some());
    // Another panel, or none, drops it.
    session.set_open_info_panel(Some(InfoPanelTarget::User(6)));
    assert!(session.payments.earn.recipients.parked().is_none());
    park(&mut session);
    session.set_open_info_panel(None);
    assert!(session.payments.earn.recipients.parked().is_none());
}
