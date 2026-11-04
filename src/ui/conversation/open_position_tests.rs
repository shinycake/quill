//! Where an opened history lands and how it holds still afterwards, through
//! the real app, scroller and paint. The driver's side (which pages are
//! asked for) is played by hand on the demo session: each test asks for a
//! page the way `ConnectDriver` does and answers it the way TDLib does, so
//! pages can land late, short, or several in one tick.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Modifiers, Pixels, ScrollDelta, ScrollEvent, TestAppContext,
    TouchPhase, VisualTestContext, point, px, size,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId, RequestId};
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const A: ChatId = ChatId(11);
const B: ChatId = ChatId(12);
const DATE: i64 = 1_790_000_000;

struct Sim {
    app: Entity<QuillApp>,
    vcx: VisualTestContext,
}

/// The text of message `id`: one to three lines, so rows differ in height.
fn text(id: i64) -> String {
    let words = "with a few more words to make the row wrap onto another line ";
    format!("Message {id} {}", words.repeat((id % 3) as usize))
}

fn message(id: i64, outgoing: bool) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{},"is_outgoing":{outgoing},"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{}","entities":[]}}}}}}"#,
        A.0,
        DATE + id * 60,
        text(id)
    )
}

/// The reader's own message `id`, still sending.
fn sending(id: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"date":{},"sending_state":{{"@type":"messageSendingStatePending","sending_id":0}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"A reply","entities":[]}}}}}}}}"#,
        A.0,
        DATE + id * 60
    )
}

impl Sim {
    /// The ready-chats demo in a window `height` tall, with chat A known to
    /// end at `last` and read up to `read`, and none of its messages loaded.
    fn new(cx: &mut TestAppContext, height: f32, last: i64, read: i64) -> Self {
        cx.update(gpui_kit::init);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let demo = ScreenshotDemo::named("ready-chats");
        let handle = cx.open_window(size(px(1100.), px(height)), move |window, cx| {
            let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
            *slot_in.borrow_mut() = Some(view.clone());
            let shell = cx.new(|_| QuillShell::new(view));
            Root::new(shell, window, cx)
        });
        let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
        let vcx = VisualTestContext::from_window(handle.into(), cx);
        let mut sim = Sim { app, vcx };
        sim.select(B);
        sim.apply(&[
            format!(
                r#"{{"@type":"updateChatLastMessage","chat_id":{},"last_message":{},"positions":[]}}"#,
                A.0,
                message(last, false)
            ),
            format!(
                r#"{{"@type":"updateChatReadInbox","chat_id":{},"last_read_inbox_message_id":{read},"unread_count":{}}}"#,
                A.0,
                last - read
            ),
        ]);
        sim.session(|s| {
            s.histories.remove(&A.0);
            // A real session: history pages come from the server, so the
            // window can stop short of the chat's end.
            s.history_pruning = Default::default();
        });
        sim
    }

    fn session<R>(&mut self, f: impl FnOnce(&mut quill::state::Session) -> R) -> R {
        self.app.update(&mut self.vcx, |app, cx| {
            cx.notify();
            f(app.demo_session.as_mut().expect("demo session"))
        })
    }

    fn frames(&mut self) {
        for _ in 0..6 {
            self.vcx.run_until_parked();
            self.vcx.update(|window, cx| window.render_frame(cx));
        }
    }

    /// Apply TDLib JSON to the session in one tick, then paint.
    fn apply(&mut self, jsons: &[String]) {
        self.session(|session| {
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            for json in jsons {
                let seq = AtomicU64::new(session.last_seq);
                if let Some(owned) = copy_and_parse(json, &seq, &sink) {
                    session.apply(owned);
                }
            }
        });
        self.frames();
    }

    fn select(&mut self, chat: ChatId) {
        self.app.update_in(&mut self.vcx, |app, window, cx| {
            app.select_listed_chat(chat, window, cx)
        });
        self.frames();
    }

    /// Open A the way the driver does (`select_chat_open`, `fetch_history`):
    /// the window is replaced unless it reaches back to the read boundary,
    /// and an empty one asks for its first page. Returns that request.
    fn open(&mut self) -> Option<RequestId> {
        let request = self.app.update_in(&mut self.vcx, |app, window, cx| {
            app.select_listed_chat(A, window, cx);
            let session = app.demo_session.as_mut().expect("demo session");
            session.prepare_history_window(A);
            let history = session.histories.get(&A.0)?;
            if !history.messages.is_empty() {
                return None;
            }
            Some(match history.unread_anchor {
                Some(anchor) => session.request_history_around(A, anchor),
                None => session.request_for_message(RequestPurpose::GetHistory, A, MessageId(0)),
            })
        });
        self.frames();
        request
    }

    fn page_json(request: RequestId, ids: impl DoubleEndedIterator<Item = i64>) -> String {
        let messages: Vec<String> = ids.rev().map(|id| message(id, false)).collect();
        format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":{},"messages":[{}]}}"#,
            request.0,
            messages.len(),
            messages.join(",")
        )
    }

    fn answer(&mut self, request: RequestId, ids: std::ops::RangeInclusive<i64>) {
        self.apply(&[Self::page_json(request, ids)]);
    }

    fn ask_older(&mut self) -> RequestId {
        self.session(|s| {
            let oldest = s.histories.get(&A.0).and_then(|h| h.oldest_id());
            s.request_for_message(RequestPurpose::GetHistory, A, oldest.expect("loaded"))
        })
    }

    fn ask_newer(&mut self) -> RequestId {
        self.session(|s| {
            let newest = s.histories.get(&A.0).and_then(|h| h.newest_id());
            s.request_for_message(RequestPurpose::GetHistoryNewer, A, newest.expect("loaded"))
        })
    }

    fn viewport(&mut self) -> Bounds<Pixels> {
        let viewport = self
            .app
            .update(&mut self.vcx, |app, _| app.history.viewport.get());
        viewport.expect("history painted")
    }

    fn has_bar(&mut self) -> bool {
        self.session(|s| {
            s.histories
                .get(&A.0)
                .is_some_and(|h| h.unread_anchor.is_some())
        })
    }

    /// How far the "Unread messages" bar is below the view's top.
    fn bar_gap(&mut self) -> f32 {
        let viewport = self.viewport();
        let bar = self.vcx.debug_bounds("unread-bar").expect("bar painted");
        f32::from(bar.top() - viewport.top())
    }

    /// Where message `id`'s row was painted.
    fn row(&mut self, id: i64) -> Option<Bounds<Pixels>> {
        self.app.update(&mut self.vcx, |app, _| {
            let rows = app.history.hit_rows.borrow();
            rows.iter()
                .find(|(ix, _)| {
                    app.history
                        .rows
                        .get(*ix)
                        .is_some_and(|row| row.contains(MessageId(id)))
                })
                .map(|(_, bounds)| *bounds)
        })
    }

    /// A landing is still bringing the history to its goal.
    fn landing(&mut self) -> bool {
        self.app
            .update(&mut self.vcx, |app, _| app.history.landing.is_some())
    }

    /// The reader presses the jump-down button.
    fn press_jump_down(&mut self) {
        self.app
            .update(&mut self.vcx, |app, cx| app.jump_down_messages(cx));
        self.frames();
    }

    fn following(&mut self) -> bool {
        self.app.update(&mut self.vcx, |app, cx| {
            app.history.scroller.read(cx).is_following_tail()
        })
    }

    /// The newest row's bottom is inside the view.
    fn shows_the_end(&mut self) -> bool {
        let viewport = self.viewport();
        let last = self.app.update(&mut self.vcx, |app, _| {
            app.history.rows.last().and_then(|row| row.last_id())
        });
        let last = last.expect("rows").0;
        self.row(last)
            .is_some_and(|row| row.bottom() <= viewport.bottom() + px(1.))
    }

    /// The reader turns a mouse wheel over the history (`dy` pixels, > 0
    /// scrolls up). Not a trackpad: its gestures past the end pull toward
    /// the next channel.
    fn wheel(&mut self, dy: f32) {
        self.wheel_event(dy);
        self.frames();
    }

    /// `wheel` without painting after it.
    fn wheel_event(&mut self, dy: f32) {
        let at = self.viewport().center();
        self.vcx.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Lines(point(0., dy / 20.)),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
            source: gpui_kit::ScrollSource::Pointer,
        });
    }

    /// Open A with 100 unread after 1100 and land on the bar.
    fn landed(cx: &mut TestAppContext) -> Self {
        let mut sim = Sim::new(cx, 700., 1_200, 1_100);
        let request = sim.open().expect("first page asked");
        sim.answer(request, 1_076..=1_124);
        sim
    }
}

fn assert_at_top(gap: f32) {
    assert!(gap.abs() < 0.5, "the bar is {gap}px below the view's top");
}

/// tdesktop holds the view until the requested first page is in
/// (`_firstLoadRequest`): a live message landing first must not decide
/// where the chat opens.
#[gpui_kit::test]
fn a_live_message_before_the_first_page_does_not_decide_the_landing(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 700., 1_083, 1_060);
    let request = sim.open().expect("first page asked");
    sim.apply(&[format!(
        r#"{{"@type":"updateNewMessage","message":{}}}"#,
        message(1_084, false)
    )]);
    sim.answer(request, 1_036..=1_084);
    assert!(sim.has_bar(), "23 unread messages do not fit on one screen");
    let gap = sim.bar_gap();
    assert_at_top(gap);
}

/// An older and a newer page answered in the same tick grow the window at
/// both ends; the reader's place stays.
#[gpui_kit::test]
fn pages_landing_together_keep_the_bar_at_the_top(cx: &mut TestAppContext) {
    let mut sim = Sim::landed(cx);
    let gap = sim.bar_gap();
    assert_at_top(gap);
    let older = sim.ask_older();
    let newer = sim.ask_newer();
    sim.apply(&[
        Sim::page_json(older, 1_026..=1_076),
        Sim::page_json(newer, 1_124..=1_174),
    ]);
    let gap = sim.bar_gap();
    assert_at_top(gap);
}

/// A first page that stops a few messages after the bar can't bring the
/// bar to the top yet. While newer messages are still to load, that is no
/// reason to drop the bar: it lands once they are in.
#[gpui_kit::test]
fn a_short_first_page_keeps_the_bar_for_the_newer_page(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 1_400., 1_300, 1_100);
    let request = sim.open().expect("first page asked");
    sim.answer(request, 1_076..=1_105);
    assert!(sim.has_bar(), "the window stops short of the chat's end");
    let newer = sim.ask_newer();
    sim.answer(newer, 1_105..=1_154);
    assert!(sim.has_bar());
    let gap = sim.bar_gap();
    assert_at_top(gap);
}

/// The reader reached the end of the loaded window while the next page was
/// on its way: the page joins below without moving what is on screen.
#[gpui_kit::test]
fn a_newer_page_at_the_window_end_leaves_the_rows_in_place(cx: &mut TestAppContext) {
    let mut sim = Sim::landed(cx);
    sim.wheel(-20_000.);
    let before = sim.row(1_124).expect("the window's last row is on screen");
    let newer = sim.ask_newer();
    sim.answer(newer, 1_124..=1_174);
    let after = sim.row(1_124).expect("still on screen");
    assert!(
        (after.top() - before.top()).abs() < px(1.),
        "row moved from {:?} to {:?}",
        before.top(),
        after.top()
    );
}

/// tdesktop `fastShowAtEnd`: whatever is sent, the history goes to its
/// latest message.
#[gpui_kit::test]
fn sending_while_scrolled_up_returns_to_the_latest(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 700., 1_100, 1_100);
    let request = sim.open().expect("first page asked");
    sim.answer(request, 1_051..=1_100);
    assert!(sim.following(), "nothing unread: opened at the bottom");
    sim.wheel(1_500.);
    assert!(!sim.following(), "scrolled up");
    sim.apply(&[sending(1_101)]);
    assert!(sim.following());
    assert!(sim.shows_the_end());
}

/// tdesktop `newItemAdded`: a message the reader sends takes the "Unread
/// messages" bar away.
#[gpui_kit::test]
fn sending_takes_the_unread_bar_away(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 700., 1_120, 1_060);
    let request = sim.open().expect("first page asked");
    sim.answer(request, 1_056..=1_120);
    assert!(sim.has_bar(), "60 unread messages do not fit on one screen");
    sim.apply(&[sending(1_121)]);
    assert!(!sim.has_bar());
    assert!(sim.following());
    assert!(sim.shows_the_end());
}

/// A message deleted right above the top row: the rows on screen stay.
#[gpui_kit::test]
fn a_deletion_next_to_the_top_row_leaves_the_view(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 700., 1_100, 1_100);
    let request = sim.open().expect("first page asked");
    sim.answer(request, 1_051..=1_100);
    sim.wheel(1_200.);
    let top = sim.app.update(&mut sim.vcx, |app, cx| {
        let (ix, _) = app
            .history
            .scroller
            .read(cx)
            .top_anchor()
            .expect("scrolled up");
        app.history.rows.get(ix).and_then(|row| row.first_id())
    });
    let watched = top.expect("a top row").0;
    let before = sim.row(watched).expect("painted");
    sim.apply(&[format!(
        r#"{{"@type":"updateDeleteMessages","chat_id":{},"message_ids":[{}],"is_permanent":true,"from_cache":false}}"#,
        A.0,
        watched - 1
    )]);
    let after = sim.row(watched).expect("still painted");
    assert!(
        (after.top() - before.top()).abs() < px(1.),
        "row moved from {:?} to {:?}",
        before.top(),
        after.top()
    );
}

/// tdesktop `itemTopForHighlight`: a message jumped to sits in the middle
/// of the view when it fits.
#[gpui_kit::test]
fn a_jump_target_is_centred(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 700., 1_100, 1_100);
    let request = sim.open().expect("first page asked");
    sim.answer(request, 1_051..=1_100);
    sim.session(|s| {
        let _ = s.begin_chat_search_jump(MessageId(1_070));
    });
    sim.frames();
    let viewport = sim.viewport();
    let row = sim.row(1_070).expect("target painted");
    let off = f32::from(row.center().y - viewport.center().y);
    assert!(
        off.abs() < 2.,
        "target centre is {off}px off the view's centre"
    );
}

/// The reader scrolls while the landing waits for newer messages: the page
/// that arrives after joins below and doesn't pull them back to the bar.
#[gpui_kit::test]
fn scrolling_while_the_bar_waits_ends_the_landing(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 1_400., 1_300, 1_100);
    let request = sim.open().expect("first page asked");
    sim.answer(request, 1_076..=1_105);
    assert!(sim.landing(), "the bar waits for newer messages");
    // Checked before a paint could spend the landing's corrections.
    sim.wheel_event(300.);
    assert!(!sim.landing(), "the reader's scroll ended the landing");
    sim.frames();
    let watched = 1_090;
    let before = sim.row(watched).expect("on screen");
    let newer = sim.ask_newer();
    sim.answer(newer, 1_105..=1_154);
    let after = sim.row(watched).expect("still on screen");
    assert!(
        (after.top() - before.top()).abs() < px(1.),
        "row moved from {:?} to {:?}",
        before.top(),
        after.top()
    );
}

/// tdesktop `insideJumpToEndInsteadOfToUnread`: choosing the open chat
/// again with nothing unread below goes to the latest message.
#[gpui_kit::test]
fn choosing_the_open_chat_again_goes_to_the_latest(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 700., 1_100, 1_100);
    let request = sim.open().expect("first page asked");
    sim.answer(request, 1_051..=1_100);
    sim.wheel(1_500.);
    assert!(!sim.following(), "scrolled up");
    sim.select(A);
    assert!(sim.following());
    assert!(sim.shows_the_end());
}

/// tdesktop `insideJumpToEndInsteadOfToUnread`: scrolled up above the
/// first unread message, choosing the open chat again brings the bar back
/// to the top.
#[gpui_kit::test]
fn choosing_the_open_chat_again_from_above_the_bar_goes_back_to_it(cx: &mut TestAppContext) {
    let mut sim = Sim::landed(cx);
    // In steps: one wheel event goes no farther than the rows measured.
    for _ in 0..3 {
        sim.wheel(500.);
    }
    let viewport = sim.viewport();
    assert!(
        sim.row(1_101)
            .is_none_or(|row| row.top() >= viewport.bottom()),
        "the first unread message is below the view"
    );
    sim.select(A);
    let gap = sim.bar_gap();
    assert_at_top(gap);
}

/// tdesktop answers "to the unread" until the history is in: choosing the
/// chat again while its first page loads leaves the landing to it.
#[gpui_kit::test]
fn choosing_the_chat_again_while_it_loads_keeps_the_bar(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 700., 1_200, 1_100);
    let request = sim.open().expect("first page asked");
    sim.select(A);
    sim.answer(request, 1_076..=1_124);
    assert!(sim.has_bar());
    let gap = sim.bar_gap();
    assert_at_top(gap);
}

/// A send that fails after the reader scrolled away leaves the view: only
/// a message still on its way brings the history to its end.
#[gpui_kit::test]
fn a_failed_send_leaves_the_view(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx, 700., 1_100, 1_100);
    let request = sim.open().expect("first page asked");
    sim.answer(request, 1_051..=1_100);
    sim.apply(&[sending(1_101)]);
    sim.wheel(1_500.);
    assert!(!sim.following(), "scrolled up");
    let viewport = sim.viewport();
    let (watched, before) = (1_051..=1_100)
        .find_map(|id| {
            sim.row(id)
                .filter(|row| row.top() >= viewport.top())
                .map(|row| (id, row))
        })
        .expect("a row on screen");
    sim.apply(&[format!(
        r#"{{"@type":"updateMessageSendFailed","old_message_id":1101,"message":{{"id":1102,"chat_id":{},"is_outgoing":true,"date":{},"sending_state":{{"@type":"messageSendingStateFailed","can_retry":true}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"A reply","entities":[]}}}}}},"error":{{"code":500}}}}"#,
        A.0,
        DATE + 1_101 * 60
    )]);
    let after = sim.row(watched).expect("still on screen");
    assert!(
        (after.top() - before.top()).abs() < px(1.),
        "row moved from {:?} to {:?}",
        before.top(),
        after.top()
    );
}

/// tdesktop `CornerButtons::downClick` → `insideJumpToEndInsteadOfToUnread`:
/// landed on the bar, the first unread message is already on screen, so
/// the jump-down button goes to the latest message.
#[gpui_kit::test]
fn the_jump_button_on_the_bar_goes_to_the_latest(cx: &mut TestAppContext) {
    let mut sim = Sim::landed(cx);
    assert!(!sim.following(), "landed on the bar");
    sim.press_jump_down();
    assert!(sim.following());
    assert!(sim.shows_the_end());
}

/// Scrolled up above the first unread message, the first press brings the
/// bar back to the top and the second goes to the latest.
#[gpui_kit::test]
fn the_jump_button_goes_to_the_bar_then_the_latest(cx: &mut TestAppContext) {
    let mut sim = Sim::landed(cx);
    for _ in 0..3 {
        sim.wheel(500.);
    }
    sim.press_jump_down();
    let gap = sim.bar_gap();
    assert_at_top(gap);
    sim.press_jump_down();
    assert!(sim.following());
    assert!(sim.shows_the_end());
}
