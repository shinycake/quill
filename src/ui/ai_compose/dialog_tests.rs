//! UI tests for the AI compose box over the `ready-ai-compose` demo: the
//! composer's AI button needs a three-line draft and known styles, the
//! button opens the box on the Style tab, the Fix tab asks for a fix, Apply
//! puts the result in the composer, and the daily limit turns the footer
//! into Increase Limit. Nothing reaches Telegram: the app has no connection.
use super::demo::{DRAFT, fix_result};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::ai_compose::{AiBoxError, AiComposeMode, CardState, FooterAction, prefs};

fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The demo opens the box on Translate over a three-line draft.
fn open_main(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    prefs::set_persistence(false);
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
                Some(ScreenshotDemo::named("ready-ai-compose")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut main = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut main);
    (app, main)
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is on screen"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

fn close_box(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| app.close_ai_compose(cx));
    frames(vcx);
}

fn set_draft(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, draft: &str) {
    app.update_in(vcx, |app, window, cx| {
        app.set_composer_markup(draft, window, cx);
    });
    frames(vcx);
}

fn card(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<(AiComposeMode, CardState)> {
    app.read_with(vcx, |app, _| {
        app.ai_compose
            .open
            .as_ref()
            .map(|open| (open.ai.mode(), open.ai.card().clone()))
    })
}

fn footer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<FooterAction> {
    app.read_with(vcx, |app, _| {
        app.ai_compose.open.as_ref().map(|open| open.ai.footer())
    })
}

#[gpui_kit::test]
fn the_button_needs_three_lines_and_styles(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    close_box(&app, &mut main);
    set_draft(&app, &mut main, DRAFT);
    assert!(main.debug_bounds("composer-ai").is_some(), "three lines");
    set_draft(&app, &mut main, "One line is too short");
    assert!(main.debug_bounds("composer-ai").is_none(), "one line");
    set_draft(&app, &mut main, DRAFT);
    app.update(&mut main, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.messages.ai_compose.styles.set_server(Vec::new());
        }
        cx.notify();
    });
    frames(&mut main);
    assert!(main.debug_bounds("composer-ai").is_none(), "no styles");
}

#[gpui_kit::test]
fn the_button_opens_the_box_on_the_style_tab(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    close_box(&app, &mut main);
    set_draft(&app, &mut main, DRAFT);
    click(&mut main, "composer-ai");
    assert_eq!(
        card(&app, &mut main),
        Some((AiComposeMode::Style, CardState::Waiting)),
        "nothing is asked until a style is picked"
    );
    assert_eq!(footer(&app, &mut main), Some(FooterAction::SelectStyle));
    assert!(main.debug_bounds("ai-compose-tab-style").is_some());
    assert!(main.debug_bounds("ai-compose-main-button").is_some());
}

#[gpui_kit::test]
fn the_fix_tab_asks_for_a_fix(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    click(&mut main, "ai-compose-tab-fix");
    assert!(matches!(
        card(&app, &mut main),
        Some((AiComposeMode::Fix, CardState::Loading { .. }))
    ));
}

#[gpui_kit::test]
fn apply_puts_the_result_in_the_composer(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    click(&mut main, "ai-compose-tab-fix");
    app.update(&mut main, |app, cx| {
        app.demo_ai_answer(Ok(fix_result()));
        cx.notify();
    });
    frames(&mut main);
    assert!(matches!(
        card(&app, &mut main),
        Some((AiComposeMode::Fix, CardState::Ready(_)))
    ));
    click(&mut main, "ai-compose-main-button");
    assert!(card(&app, &mut main).is_none(), "the box closed");
    let composer = app.read_with(&main, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(composer, fix_result().text);
}

#[gpui_kit::test]
fn the_daily_limit_offers_increase_limit(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    click(&mut main, "ai-compose-tab-fix");
    app.update(&mut main, |app, cx| {
        app.demo_ai_answer(Err(AiBoxError::FloodPremium));
        cx.notify();
    });
    frames(&mut main);
    assert_eq!(footer(&app, &mut main), Some(FooterAction::IncreaseLimit));
    assert_eq!(
        main.update(|window, cx| window.notifications(cx).len()),
        1,
        "the limit toast"
    );
    click(&mut main, "ai-compose-main-button");
    assert!(
        card(&app, &mut main).is_none(),
        "Increase Limit leaves the box"
    );
}
