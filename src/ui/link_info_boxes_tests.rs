//! The gift code box keeps its apply: Close while `applyPremiumGiftCode` is
//! out does not drop the stage that owns the answer (needs the
//! `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::RequestId;
use quill::state::{GiftCodeLookup, GiftCodeStage};
use quill::telegram::envelope::GiftCodeInfoData;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn info() -> GiftCodeInfoData {
    GiftCodeInfoData {
        creator: None,
        creation_date: 0,
        is_from_giveaway: false,
        month_count: 3,
        day_count: 0,
        user_id: 0,
        use_date: 0,
    }
}

fn stage_of(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<GiftCodeStage> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.payments.gift_code.as_ref())
            .map(|lookup| lookup.stage.clone())
    })
}

#[gpui_kit::test]
fn close_keeps_a_gift_code_that_is_applying_and_closes_one_that_is_not(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let applying = GiftCodeStage::Applying {
        request: RequestId(7),
        info: info(),
    };
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.gift_code = Some(GiftCodeLookup {
                code: "abc".into(),
                stage: applying.clone(),
            });
        }
        app.close_gift_code_box(cx);
    });
    assert_eq!(
        stage_of(&app, vcx),
        Some(applying),
        "the apply keeps its box and its answer"
    );
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.gift_code = Some(GiftCodeLookup {
                code: "abc".into(),
                stage: GiftCodeStage::Known(info()),
            });
        }
        app.close_gift_code_box(cx);
    });
    assert_eq!(stage_of(&app, vcx), None, "a checked code just closes");
}

#[gpui_kit::test]
fn a_second_gift_link_while_applying_says_it_was_left(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let applying = GiftCodeStage::Applying {
        request: RequestId(7),
        info: info(),
    };
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.gift_code = Some(GiftCodeLookup {
                code: "first".into(),
                stage: applying.clone(),
            });
        }
        app.open_gift_code_link("second".into(), cx);
    });
    assert_eq!(stage_of(&app, vcx), Some(applying), "the first code stays");
    let note = app.read_with(vcx, |app, _| app.connection.note().text().to_string());
    assert_eq!(note, "Wait for the current gift code");
}
