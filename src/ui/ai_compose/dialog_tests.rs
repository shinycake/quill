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

pub(super) fn frames(vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

pub(super) fn open_main(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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

pub(super) fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is on screen"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

pub(super) fn close_box(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
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

/// Round 8, 2d: the box opened over a chat window's composer belongs to
/// that window. The main window's frames, which never show that chat,
/// leave it open.
#[gpui_kit::test]
fn a_box_opened_in_a_chat_window_survives_the_main_windows_frames(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    let chat = app
        .read_with(&main, |app, _| app.session().and_then(|s| s.open_chat))
        .expect("the demo opens a chat");
    app.update_in(&mut main, |app, window, cx| {
        app.open_chat_in_new_window(chat, window, cx)
    });
    main.run_until_parked();
    let main_id = main.update(|window, _| window.window_handle().window_id());
    let other = cx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main_id)
        .expect("the chat window");
    cx.update_window(other, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_of(window, cx, |app, window, cx| {
                app.set_composer_markup(DRAFT, window, cx);
                app.open_ai_compose(window, cx);
            });
        });
    })
    .expect("the chat window is open");
    frames(&mut main);
    let open = app.read_with(&main, |app, _| {
        app.ai_compose
            .open
            .as_ref()
            .map(|open| open.conversation.chat_id)
    });
    assert_eq!(open, Some(chat), "the main window's frames keep it");
}

/// Round 15 (area B, findings 2 and 3): the box's answer is taken in the
/// frame of the window it was opened in, so it lands while the main
/// window draws nothing (minimized), and its error toast shows in that
/// window, not behind it in the main one.
#[gpui_kit::test]
fn a_chat_windows_box_takes_its_answer_and_toasts_in_its_own_frame(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    let chat = app
        .read_with(&main, |app, _| app.session().and_then(|s| s.open_chat))
        .expect("the demo opens a chat");
    app.update_in(&mut main, |app, window, cx| {
        app.open_chat_in_new_window(chat, window, cx)
    });
    main.run_until_parked();
    let main_id = main.update(|window, _| window.window_handle().window_id());
    let other = cx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main_id)
        .expect("the chat window");
    cx.update_window(other, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_of(window, cx, |app, window, cx| {
                app.set_composer_markup(DRAFT, window, cx);
                app.open_ai_compose(window, cx);
                app.ai_compose_update(window, cx, |ai| ai.set_mode(AiComposeMode::Fix));
            });
        });
    })
    .expect("the chat window is open");
    app.update(cx, |app, cx| {
        app.demo_ai_answer(Err(AiBoxError::FloodPremium));
        cx.notify();
    });
    // Only the chat window draws: the main window is minimized.
    let mut window = VisualTestContext::from_window(other, cx);
    frames(&mut window);
    assert_eq!(
        footer(&app, &mut window),
        Some(FooterAction::IncreaseLimit),
        "the answer landed without a main-window frame"
    );
    assert_eq!(
        window.update(|window, cx| window.notifications(cx).len()),
        1,
        "the limit toast shows in the chat window"
    );
    assert_eq!(
        main.update(|window, cx| window.notifications(cx).len()),
        0,
        "not in the main window"
    );
}

/// Round 15 follow-up: the style preview a link opens in a chat window
/// takes its answer in that window's frame, so it lands while the main
/// window is minimized, and its toast shows there.
#[gpui_kit::test]
fn a_chat_windows_style_preview_takes_its_answer_in_its_own_frame(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    let chat = app
        .read_with(&main, |app, _| app.session().and_then(|s| s.open_chat))
        .expect("the demo opens a chat");
    app.update_in(&mut main, |app, window, cx| {
        app.open_chat_in_new_window(chat, window, cx)
    });
    main.run_until_parked();
    let main_id = main.update(|window, _| window.window_handle().window_id());
    let other = cx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main_id)
        .expect("the chat window");
    cx.update_window(other, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_of(window, cx, |app, window, cx| {
                app.open_ai_style_preview("formal".into(), window, cx);
            });
        });
    })
    .expect("the chat window is open");
    app.update(cx, |app, cx| {
        let token = app
            .ai_compose
            .preview
            .as_ref()
            .and_then(|p| p.lookup_token());
        if let (Some(token), Some(session)) = (token, app.demo_session.as_mut()) {
            session.messages.ai_compose.style_lookup = Some(quill::state::StyleLookup {
                token,
                name: "formal".into(),
                outcome: Some(Err(quill::state::StyleError::Invalid)),
            });
        }
        cx.notify();
    });
    // Only the chat window draws: the main window is minimized.
    let mut window = VisualTestContext::from_window(other, cx);
    frames(&mut window);
    assert!(
        app.read_with(&window, |app, _| app.ai_compose.preview.is_none()),
        "the answer landed without a main-window frame"
    );
    assert_eq!(
        window.update(|window, cx| window.notifications(cx).len()),
        1,
        "the toast shows in the chat window"
    );
    assert_eq!(
        main.update(|window, cx| window.notifications(cx).len()),
        0,
        "not in the main window"
    );
}

/// Moves the view in place to another topic of the chat it shows: a
/// forum's topics share the chat id.
fn switch_topic(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, topic: i32) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_topic = Some(topic);
        }
        cx.notify();
    });
}

/// Round 16 (area B, finding 2): the box is its conversation's. Another
/// topic of the same chat closes it, so Apply or Send never writes one
/// topic's rewrite into the next topic's draft.
#[gpui_kit::test]
fn the_box_closes_when_the_view_moves_to_another_topic(cx: &mut TestAppContext) {
    let (app, mut main) = open_main(cx);
    assert!(card(&app, &mut main).is_some(), "the demo opens the box");
    switch_topic(&app, &mut main, 5);
    frames(&mut main);
    assert!(card(&app, &mut main).is_none(), "it was the other topic's");
}

/// Round 16 (area B, finding 2): the apply-in-place shortcut's answer
/// lands only in the conversation that asked; in another topic of the same
/// chat it is dropped, and that topic's draft stays.
#[gpui_kit::test]
fn the_shortcuts_answer_skips_another_topic(cx: &mut TestAppContext) {
    use crate::ui::ai_compose_state::ApplyInPlace;
    use quill::state::ApplyInPlaceAnswer;
    let (app, mut main) = open_main(cx);
    close_box(&app, &mut main);
    set_draft(&app, &mut main, "the topic's own draft");
    app.update(&mut main, |app, _| {
        let conversation = app.open_conversation().expect("a chat is open");
        app.ai_compose.apply_in_place = Some(ApplyInPlace {
            token: 41,
            account: quill::ids::AccountKey::primary(),
            field: "the topic's own draft".into(),
            conversation,
            window: None,
            selection: None,
            cursor: 0,
        });
    });
    switch_topic(&app, &mut main, 5);
    frames(&mut main);
    set_draft(&app, &mut main, "the next topic's draft");
    app.update(&mut main, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.messages.ai_compose.apply_in_place = Some(ApplyInPlaceAnswer {
                token: 41,
                outcome: Ok(("Rewritten for the first topic".into(), Vec::new())),
            });
        }
        cx.notify();
    });
    frames(&mut main);
    let composer = app.read_with(&main, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(composer, "the next topic's draft");
}

/// Sweep 07-028: text typed while the answer was coming is not overwritten:
/// the answer rewrites a range of the draft it was asked for, so it lands
/// only while the composer still holds that draft.
#[gpui_kit::test]
fn the_shortcuts_answer_skips_a_draft_typed_on_meanwhile(cx: &mut TestAppContext) {
    use crate::ui::ai_compose_state::ApplyInPlace;
    use quill::state::ApplyInPlaceAnswer;
    let (app, mut main) = open_main(cx);
    close_box(&app, &mut main);
    set_draft(&app, &mut main, "Hello there");
    app.update(&mut main, |app, _| {
        let conversation = app.open_conversation().expect("a chat is open");
        app.ai_compose.apply_in_place = Some(ApplyInPlace {
            token: 42,
            account: quill::ids::AccountKey::primary(),
            field: "Hello there".into(),
            conversation,
            window: None,
            selection: None,
            cursor: 0,
        });
    });
    set_draft(&app, &mut main, "Typed on after pressing");
    app.update(&mut main, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.messages.ai_compose.apply_in_place = Some(ApplyInPlaceAnswer {
                token: 42,
                outcome: Ok(("Rewritten".into(), Vec::new())),
            });
        }
        cx.notify();
    });
    frames(&mut main);
    let composer = app.read_with(&main, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(composer, "Typed on after pressing");
}
