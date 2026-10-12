use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, UserId};
use crate::state::{Session, placeholder_chat};
use crate::telegram::envelope::ChatKind;
use std::sync::Arc;

/// Pull with the finger down, in `steps` events of `each` px, starting at
/// `t`; returns the steps and the time after.
fn pull(machine: &mut PullMachine, steps: usize, each: f32, t: u64) -> (Vec<Step>, u64) {
    let mut out = vec![machine.wheel(Phase::Started, 0., true, true, t)];
    let mut now = t;
    for _ in 0..steps {
        now += 10;
        out.push(machine.wheel(Phase::Moved, each, true, true, now));
    }
    (out, now)
}

#[test]
fn soft_pull_curve_matches_lib_ui() {
    // Within the pull distance the stretch is the travel / 2.5.
    assert_eq!(overscroll_from_accumulated(100., PULL_THRESHOLD), 40.);
    assert_eq!(accumulated_from_overscroll(40., PULL_THRESHOLD), 100.);
    // Past it, the stiff band (1/20) takes over.
    let soft = PULL_THRESHOLD * 2.5;
    assert_eq!(
        overscroll_from_accumulated(soft + 200., PULL_THRESHOLD),
        PULL_THRESHOLD + 10.
    );
    for value in [0., 10., 95., 96., 120., 300.] {
        let back = overscroll_from_accumulated(
            accumulated_from_overscroll(value, PULL_THRESHOLD),
            PULL_THRESHOLD,
        );
        assert!((back - value).abs() < 1e-3, "{value} → {back}");
    }
    assert_eq!(PULL_THRESHOLD, 96.);
}

#[test]
fn bounce_settles_at_zero() {
    assert_eq!(bounce_progress(0.), 0.);
    assert!((bounce_progress(0.45) - 1.).abs() < 1e-5);
    assert!((bounce_progress(0.75) + 0.5).abs() < 1e-5);
    assert!(bounce_progress(1.).abs() < 1e-5);
}

#[test]
fn pull_starts_only_at_the_bottom_of_an_active_chat() {
    let mut machine = PullMachine::new();
    machine.wheel(Phase::Started, 0., false, true, 0);
    let step = machine.wheel(Phase::Moved, 30., false, true, 10);
    assert!(!step.consumed && !step.started);
    let mut machine = PullMachine::new();
    machine.wheel(Phase::Started, 0., true, false, 0);
    let step = machine.wheel(Phase::Moved, 30., true, false, 10);
    assert!(!step.consumed && !step.started);
    // Scrolling up at the bottom is the list's.
    let mut machine = PullMachine::new();
    machine.wheel(Phase::Started, 0., true, true, 0);
    let step = machine.wheel(Phase::Moved, -30., true, true, 10);
    assert!(!step.consumed);
    // A wheel without a finger down (mouse, momentum) never pulls.
    let mut machine = PullMachine::new();
    let step = machine.wheel(Phase::Moved, 30., true, true, 10);
    assert!(!step.consumed && !step.started);
}

#[test]
fn holding_past_the_threshold_gets_ready_after_the_dwell() {
    let mut machine = PullMachine::new();
    let (steps, now) = pull(&mut machine, 10, 25., 0);
    assert!(steps.iter().filter(|s| s.started).count() == 1);
    assert!(steps.iter().skip(1).all(|s| s.consumed));
    // 250px of travel: 96 soft + 10 more past it.
    assert!(machine.overscroll() >= PULL_THRESHOLD);
    assert!(!machine.reached());
    // The dwell was armed when the threshold was crossed (t = 100).
    let early = machine.frame(true, now + READY_DWELL_MS - 100);
    assert!(!early.haptic);
    let ready = machine.frame(true, now + READY_DWELL_MS);
    assert!(ready.haptic);
    assert!(machine.reached());
    // Letting go asks the UI to jump.
    let release = machine.wheel(Phase::Ended, 0., true, true, now + 200);
    assert!(release.release_ready);
    machine.begin_jump();
    assert!(machine.jumping());
    // The card finishes expanding, then holds while the jump happens.
    machine.frame(true, now + 600);
    let shift = machine.content_shift();
    assert_eq!(shift, PULL_THRESHOLD + 0.5 + (EXPAND - PULL_THRESHOLD));
    machine.frame(true, now + 900);
    assert_eq!(machine.content_shift(), shift);
    assert!(machine.indicator(now + 900).is_some());
}

#[test]
fn a_flick_past_the_threshold_springs_back() {
    let mut machine = PullMachine::new();
    let (_, now) = pull(&mut machine, 10, 25., 0);
    // Released before the dwell fired.
    let release = machine.wheel(Phase::Ended, 0., true, true, now + 20);
    assert!(!release.release_ready);
    assert!(release.consumed);
    // The edge returns and the pull resets.
    let mut t = now + 20;
    while machine.animating(t) && t < now + 3000 {
        t += 16;
        machine.frame(true, t);
    }
    assert!(!machine.pulling());
    assert_eq!(machine.overscroll(), 0.);
    assert!(machine.indicator(t).is_none());
    assert!(machine.hint(t).is_none());
}

#[test]
fn pulling_back_unwinds_one_to_one_and_resets_ready() {
    let mut machine = PullMachine::new();
    let (_, now) = pull(&mut machine, 12, 25., 0);
    machine.frame(true, now + READY_DWELL_MS);
    assert!(machine.reached());
    let before = machine.overscroll();
    machine.wheel(Phase::Moved, -5., true, true, now + 130);
    assert!((machine.overscroll() - (before - 5.)).abs() < 1e-3);
    // Far enough back past the peak-relative point, it is not ready.
    machine.wheel(Phase::Moved, -40., true, true, now + 140);
    assert!(!machine.reached());
}

#[test]
fn the_hint_shows_only_with_a_next_chat() {
    let mut machine = PullMachine::new();
    let (_, now) = pull(&mut machine, 3, 25., 0);
    assert!(machine.hint(now + 400).is_none());
    machine.set_has_next(true, now);
    machine.wheel(Phase::Moved, 5., true, true, now + 10);
    let hint = machine.hint(now + 400).expect("hint");
    assert_eq!(hint.panel, 1.);
    assert_eq!(hint.release, 0.);
}

#[test]
fn indicator_layout_follows_tdesktop() {
    // Fully ready card: the photo sits at the top of the 110px card.
    let ready = IndicatorLayout::compute(
        IndicatorState {
            offset: EXPAND,
            ready: true,
            release: 1.,
            bounce_offset: 0.,
        },
        400.,
    );
    assert!(ready.capsule.is_none());
    let (avatar, alpha) = ready.avatar.expect("avatar");
    assert_eq!(alpha, 1.);
    assert_eq!(avatar.w, AVATAR);
    assert_eq!(avatar.x, 200. - AVATAR / 2.);
    assert_eq!(avatar.y, MAX_HEIGHT - EXPAND + PADDING);
    let (name_top, name_alpha) = ready.name.expect("name");
    assert_eq!(name_top, MAX_HEIGHT - RISE - PADDING);
    assert_eq!(name_alpha, 1.);
    // Early pull: a capsule growing from the bottom, no name yet.
    let early = IndicatorLayout::compute(
        IndicatorState {
            offset: 40.,
            ready: false,
            release: 0.,
            bounce_offset: 0.,
        },
        400.,
    );
    let capsule = early.capsule.expect("capsule");
    assert_eq!(capsule.rect.y, MAX_HEIGHT - 40.);
    assert_eq!(capsule.rect.y + capsule.rect.h, MAX_HEIGHT - SKIP);
    assert!(early.name.is_none());
    assert!((capsule.alpha - 40. / EXPAND).abs() < 1e-5);
}

#[test]
fn arrow_strokes_scale_around_the_centre() {
    let [shaft, head] = arrow_strokes(100., 100., ARROW_SIZE);
    assert_eq!(shaft, vec![(100.5, 92.), (100.5, 110.)]);
    assert_eq!(head[1], (100.5, 91.5));
    assert_eq!(badge_text(5), "5");
    assert_eq!(badge_text(1000), "999+");
}

fn session() -> Session {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    Session::new(AccountKey::primary(), sink)
}

fn add(session: &mut Session, id: i64, kind: ChatKind, unread: i32, main: bool, archive: bool) {
    let mut chat = placeholder_chat(ChatId(id));
    chat.kind = kind;
    chat.unread_count = unread;
    chat.in_main_list = main;
    chat.in_archive = archive;
    session.chats.insert(id, chat);
    if main {
        session.main_order.push(ChatId(id));
    }
    if archive {
        session.archive_order.push(ChatId(id));
    }
}

fn channel(id: i64) -> ChatKind {
    ChatKind::Supergroup {
        supergroup_id: id,
        is_channel: true,
    }
}

#[test]
fn next_channel_is_the_first_unread_channel_in_list_order() {
    let mut s = session();
    add(&mut s, 1, channel(1), 3, true, false); // current
    add(
        &mut s,
        2,
        ChatKind::Private { user_id: UserId(9) },
        5,
        true,
        false,
    );
    add(
        &mut s,
        3,
        ChatKind::Supergroup {
            supergroup_id: 3,
            is_channel: false,
        },
        5,
        true,
        false,
    );
    add(&mut s, 4, channel(4), 0, true, false);
    add(&mut s, 5, channel(5), 2, true, false);
    add(&mut s, 6, channel(6), 7, true, false);
    assert_eq!(
        find_next_unread_channel(&s, ChatId(1), None),
        Some(ChatId(5))
    );
    // The promoted chat and restricted channels are skipped.
    s.chat_list.promoted_chats.insert(5);
    s.groups.supergroup_restricted.insert(6);
    assert_eq!(find_next_unread_channel(&s, ChatId(1), None), None);
    // The archive comes last.
    add(&mut s, 7, channel(7), 1, false, true);
    assert_eq!(
        find_next_unread_channel(&s, ChatId(1), None),
        Some(ChatId(7))
    );
}

#[test]
fn promotion_comes_from_the_position_source() {
    let mut s = session();
    let seq = std::sync::atomic::AtomicU64::new(0);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let json = r#"{"@type":"updateChatPosition","chat_id":42,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false,"source":{"@type":"chatSourceMtprotoProxy"}}}"#;
    if let Some(owned) = crate::telegram::client::copy_and_parse(json, &seq, &sink) {
        s.apply(owned);
    }
    assert!(s.chat_list.promoted_chats.contains(&42));
    let json = r#"{"@type":"updateChatPosition","chat_id":42,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false,"source":null}}"#;
    if let Some(owned) = crate::telegram::client::copy_and_parse(json, &seq, &sink) {
        s.apply(owned);
    }
    assert!(!s.chat_list.promoted_chats.contains(&42));
}

#[test]
fn restriction_comes_from_the_supergroup() {
    let mut s = session();
    let seq = std::sync::atomic::AtomicU64::new(0);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":77,"is_channel":true,"restriction_info":{"@type":"restrictionInfo","restriction_reason":"Not available here","has_sensitive_content":false}}}"#;
    if let Some(owned) = crate::telegram::client::copy_and_parse(json, &seq, &sink) {
        s.apply(owned);
    }
    assert!(s.groups.supergroup_restricted.contains(&77));
    let mut chat = placeholder_chat(ChatId(-100));
    chat.kind = channel(77);
    assert!(chat_unavailable(&s, &chat));
}

#[test]
fn texts_are_tdesktops() {
    assert_eq!(
        hint_text(Mode::History, false),
        "Pull up to go to the next unread channel"
    );
    assert_eq!(
        hint_text(Mode::Topic, true),
        "Release to go to the next unread topic"
    );
    assert_eq!(
        nothing_left_text(Mode::History, ""),
        "You have no unread channels"
    );
    assert_eq!(
        nothing_left_text(Mode::Topic, "Rustaceans"),
        "No more unread topics in Rustaceans"
    );
}
