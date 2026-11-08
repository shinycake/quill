//! "Jump to root" in a thread whose older replies are not loaded yet: the
//! page that reaches the root lands in the rows and the view goes to the
//! root, instead of the view hold putting the reader back where they were.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase,
    VisualTestContext, point, px, size,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CHANNEL: ChatId = ChatId(13);
const POST: MessageId = MessageId(101);
const DISCUSSION: ChatId = ChatId(14);
const ROOT: i64 = 501;
const NEWEST: i64 = 560;
const DATE: i64 = 1_790_631_600;

fn reply(id: i64) -> String {
    let content = if id == ROOT {
        "The root post"
    } else {
        "A reply in the thread with a few more words to make it wrap onto a second line"
    };
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{},"is_outgoing":false,"date":{},"topic_id":{{"@type":"messageTopicThread","message_thread_id":{ROOT}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{content}","entities":[]}}}}}}"#,
        DISCUSSION.0,
        DATE + id * 60
    )
}

struct Sim {
    app: Entity<QuillApp>,
    vcx: VisualTestContext,
}

impl Sim {
    /// The comments of a post with replies 502..=560, the newest half
    /// loaded and the view at the bottom.
    fn new(cx: &mut TestAppContext) -> Self {
        cx.update(gpui_kit::init);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let demo = ScreenshotDemo::named("ready-chats");
        let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
            let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
            *slot_in.borrow_mut() = Some(view.clone());
            let shell = cx.new(|_| QuillShell::new(view));
            Root::new(shell, window, cx)
        });
        let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
        let vcx = VisualTestContext::from_window(handle.into(), cx);
        let mut sim = Sim { app, vcx };
        let info = sim.session(|s| {
            s.begin_thread(CHANNEL, POST);
            s.request(
                RequestPurpose::GetMessageThread { message_id: POST.0 },
                Some(CHANNEL),
            )
        });
        sim.apply(&[
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{},"title":"Discussion","type":{{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false}},"unread_count":0}}}}"#,
                DISCUSSION.0
            ),
            format!(
                r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":{},"message_thread_id":{ROOT},"reply_info":{{"@type":"messageReplyInfo","reply_count":59,"recent_replier_ids":[],"last_read_inbox_message_id":{NEWEST},"last_read_outbox_message_id":{NEWEST},"last_message_id":{NEWEST}}},"unread_message_count":0,"messages":[{}]}}"#,
                info.0,
                DISCUSSION.0,
                reply(ROOT)
            ),
        ]);
        sim.session(|s| s.open_chat(DISCUSSION));
        sim.page(531..=NEWEST);
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

    /// Ask for the next older thread page and answer it with `ids`.
    fn page(&mut self, ids: std::ops::RangeInclusive<i64>) {
        let request = self.session(|s| {
            s.request(
                RequestPurpose::GetMessageThreadHistory { message_id: POST.0 },
                Some(CHANNEL),
            )
        });
        let messages: Vec<String> = ids.rev().map(reply).collect();
        self.apply(&[format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":{},"messages":[{}]}}"#,
            request.0,
            messages.len(),
            messages.join(",")
        )]);
    }

    fn loaded_complete(&mut self) -> bool {
        self.session(|s| {
            s.threads
                .thread
                .as_ref()
                .is_some_and(|thread| thread.history.loaded_complete)
        })
    }

    /// The reader turns a mouse wheel over the history (`dy` > 0 scrolls up).
    fn wheel(&mut self, dy: f32) {
        let at = self
            .app
            .update(&mut self.vcx, |app, _| app.history.viewport.get())
            .expect("history painted")
            .center();
        self.vcx.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Lines(point(0., dy / 20.)),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
            source: gpui_kit::ScrollSource::Pointer,
        });
        self.frames();
    }

    /// The row at the view's top and how far into it the view is.
    fn top(&mut self) -> (Option<MessageId>, f32) {
        self.app.update(&mut self.vcx, |app, cx| {
            let top = app.history.scroller.read(cx).logical_scroll_top();
            let message = app
                .history
                .rows
                .get(top.item_ix)
                .and_then(|row| row.first_id());
            (message, f32::from(top.offset_in_item))
        })
    }
}

#[gpui_kit::test]
fn the_page_that_reaches_the_root_lands_the_view_on_it(cx: &mut TestAppContext) {
    let mut sim = Sim::new(cx);
    assert!(!sim.loaded_complete());
    // Away from the latest, so the view hold has a row to keep.
    sim.wheel(300.);
    let (before, _) = sim.top();
    assert!(before.is_some_and(|id| id.0 > ROOT));
    sim.app
        .update(&mut sim.vcx, |app, cx| app.jump_to_thread_root(cx));
    sim.frames();
    // The root is not loaded yet: the view waits where it is.
    assert_eq!(sim.top().0, before);

    sim.page(ROOT..=530);
    assert!(sim.loaded_complete());
    assert_eq!(sim.top(), (Some(MessageId(ROOT)), 0.));
    let pending = sim
        .app
        .update(&mut sim.vcx, |app, _| app.history.scroll_goal.is_some());
    assert!(!pending);
}
