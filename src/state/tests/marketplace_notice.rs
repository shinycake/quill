//! A resale purchase whose box closed tells its outcome in a notice, and one
//! whose box is open pushes none (sweep r4-04).
use super::common::*;
use super::*;
use crate::ids::RequestId;
use crate::marketplace::{GiftPrice, GiftPurchase, GiftPurchaseStage, GiftQuote};
use crate::state::NoticeKind;

fn quote() -> GiftQuote {
    GiftQuote {
        name: "Plush".into(),
        title: "Plush".into(),
        stars: Some(GiftPrice::Stars(25)),
        ton: None,
    }
}

/// A session with a purchase sending under a real request; `open` says
/// whether its box is still shown.
fn sending(open: bool) -> (Session, Arc<MemorySink>, RequestId) {
    let (mut session, sink) = session();
    let id = session.request(RequestPurpose::SendMarketplaceGift, None);
    let mut gift = GiftPurchase::loading(
        ChatId(1),
        MessageSender::User { user_id: 1 },
        "Ada".into(),
        "Plush".into(),
        RequestId(1),
    );
    gift.stage = GiftPurchaseStage::Sending {
        quote: quote(),
        price: GiftPrice::Stars(25),
        request: id,
    };
    if !open {
        gift.close();
    }
    session.payments.marketplace_gift = Some(gift);
    (session, sink, id)
}

fn apply(session: &mut Session, sink: &Arc<MemorySink>, json: &str) {
    thread_local! {
        static SEQ: AtomicU64 = const { AtomicU64::new(0) };
    }
    SEQ.with(|seq| apply_json(session, seq, sink, json));
}

fn sent(id: RequestId) -> String {
    format!(
        r#"{{"@type":"giftResaleResultOk","@extra":"{}","received_gift_id":"g"}}"#,
        id.0
    )
}

fn refused(id: RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"BAD"}}"#,
        id.0
    )
}

#[test]
fn a_closed_boxs_answer_pushes_an_info_notice() {
    let (mut session, sink, id) = sending(false);
    apply(&mut session, &sink, &sent(id));
    let kinds: Vec<_> = session.payments.notices.iter().map(|n| n.kind).collect();
    assert_eq!(kinds, vec![NoticeKind::Info]);
}

#[test]
fn a_closed_boxs_error_pushes_an_error_notice() {
    let (mut session, sink, id) = sending(false);
    apply(&mut session, &sink, &refused(id));
    let kinds: Vec<_> = session.payments.notices.iter().map(|n| n.kind).collect();
    assert_eq!(kinds, vec![NoticeKind::Error]);
}

#[test]
fn an_open_boxs_answer_or_error_pushes_no_notice() {
    let (mut session, sink, id) = sending(true);
    apply(&mut session, &sink, &sent(id));
    assert!(session.payments.notices.is_empty());
    let (mut session, sink, id) = sending(true);
    apply(&mut session, &sink, &refused(id));
    assert!(session.payments.notices.is_empty());
}
