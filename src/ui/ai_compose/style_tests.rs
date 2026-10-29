//! UI tests for the AI compose styles over the `ready-ai-compose` demo: the
//! Create pill opens New Style, a missing name warns and sends nothing, a
//! saved style closes the editor with a toast and gets selected, the
//! preview offers Add or Remove, the saved-style limit toasts once, and the
//! apply hotkey with no style bound only toasts. Nothing reaches Telegram:
//! the app has no connection.
use super::tests::{click, close_box, frames, open_main};
use crate::ui::ai_compose_state::{AiComposeView, EditorWarning};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::apply_custom_bindings;
use gpui_kit::component::WindowExt;
use gpui_kit::{
    Entity, Modifiers, ScrollDelta, ScrollEvent, TouchPhase, VisualTestContext, point, px,
};
use quill::ai_compose::{StyleChoice, StyleLimitNotice, prefs};
use quill::settings::CustomKeybinding;
use quill::state::{StyleEditAnswer, StyleNotice};

fn toasts(vcx: &mut VisualTestContext) -> usize {
    vcx.update(|window, cx| window.notifications(cx).len())
}

fn view(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<AiComposeView> {
    app.read_with(vcx, |app, _| {
        app.ai_compose.open.as_ref().map(|open| open.view)
    })
}

fn open_editor(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    click(vcx, "ai-compose-tab-style");
    let row = vcx
        .debug_bounds("ai-compose-tab-style")
        .map(|tab| tab.center().x);
    let pill = vcx
        .debug_bounds("ai-style-create")
        .map(|pill| pill.center().y);
    if let (Some(x), Some(y)) = (row, pill) {
        vcx.simulate_event(ScrollEvent {
            position: point(x, y),
            delta: ScrollDelta::Pixels(point(px(-400.), px(0.))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
            source: gpui_kit::ScrollSource::Pointer,
        });
        frames(vcx);
    }
    click(vcx, "ai-style-create");
    assert_eq!(view(app, vcx), Some(AiComposeView::StyleEditor));
}

#[gpui_kit::test]
fn the_create_pill_opens_a_blank_editor(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut main) = open_main(cx);
    open_editor(&app, &mut main);
    let editing = app.read_with(&main, |app, _| {
        app.ai_compose
            .open
            .as_ref()
            .and_then(|open| open.editor.as_ref())
            .map(|editor| editor.editing.clone())
    });
    assert_eq!(editing, Some(None), "a new style");
    assert!(main.debug_bounds("ai-style-icon").is_some());
    assert!(main.debug_bounds("ai-style-submit").is_some());
}

#[gpui_kit::test]
fn an_empty_name_warns_and_sends_nothing(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut main) = open_main(cx);
    open_editor(&app, &mut main);
    app.update(&mut main, |app, cx| {
        if let Some(editor) = app.ai_style_editor_mut() {
            editor.emoji_id = 4242;
        }
        cx.notify();
    });
    frames(&mut main);
    click(&mut main, "ai-style-submit");
    let editor = app.read_with(&main, |app, _| {
        app.ai_compose
            .open
            .as_ref()
            .and_then(|open| open.editor.as_ref())
            .map(|editor| (editor.warning, editor.pending))
    });
    assert_eq!(editor, Some((Some(EditorWarning::Name), false)), "not sent");
    assert!(main.debug_bounds("ai-style-warning").is_some());
    assert_eq!(view(&app, &mut main), Some(AiComposeView::StyleEditor));
}

#[gpui_kit::test]
fn a_saved_style_closes_the_editor_and_is_selected(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut main) = open_main(cx);
    open_editor(&app, &mut main);
    let before = toasts(&mut main);
    app.update(&mut main, |app, cx| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        let ai = &mut session.messages.ai_compose;
        let pirate = ai.styles.find("pirate").cloned();
        ai.style_edit = pirate.map(|style| StyleEditAnswer { outcome: Ok(style) });
        cx.notify();
    });
    frames(&mut main);
    assert_eq!(view(&app, &mut main), Some(AiComposeView::Card));
    let picked = app.read_with(&main, |app, _| {
        app.ai_compose
            .open
            .as_ref()
            .map(|open| (open.editor.is_none(), open.ai.style().clone()))
    });
    assert_eq!(picked, Some((true, StyleChoice::Style("pirate".into()))));
    assert_eq!(toasts(&mut main), before + 1, "the Created toast");
}

#[gpui_kit::test]
fn the_preview_offers_add_for_a_new_style(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut main) = open_main(cx);
    close_box(&app, &mut main);
    app.update_in(&mut main, |app, window, cx| {
        app.demo_ai_preview("preview", window, cx);
    });
    frames(&mut main);
    assert!(main.debug_bounds("ai-style-example").is_some());
    assert!(main.debug_bounds("ai-style-preview-add").is_some());
    assert!(main.debug_bounds("ai-style-preview-remove").is_none());
}

#[gpui_kit::test]
fn the_preview_offers_remove_for_a_saved_style(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut main) = open_main(cx);
    close_box(&app, &mut main);
    app.update_in(&mut main, |app, window, cx| {
        app.demo_ai_preview("preview-installed", window, cx);
    });
    frames(&mut main);
    assert!(main.debug_bounds("ai-style-preview-remove").is_some());
    assert!(main.debug_bounds("ai-style-preview-add").is_none());
}

#[gpui_kit::test]
fn the_saved_style_limit_toasts_once(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut main) = open_main(cx);
    let before = toasts(&mut main);
    app.update(&mut main, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.messages.ai_compose.style_notice =
                Some(StyleNotice::Limit(StyleLimitNotice::Promo {
                    count: 5,
                    premium_count: 20,
                }));
        }
        cx.notify();
    });
    frames(&mut main);
    frames(&mut main);
    assert_eq!(toasts(&mut main), before + 1);
}

#[gpui_kit::test]
fn the_apply_hotkey_without_a_style_only_toasts(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut main) = open_main(cx);
    close_box(&app, &mut main);
    prefs::update(|prefs| prefs.bound_style_name = None);
    main.update(|_, cx| {
        apply_custom_bindings(
            cx,
            &[CustomKeybinding {
                id: "ai-compose-apply".into(),
                keystroke: "ctrl-alt-y".into(),
            }],
        );
    });
    main.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
    let draft = app.read_with(&main, |app, cx| app.composer.read(cx).value().to_string());
    let before = toasts(&mut main);
    main.simulate_keystrokes("ctrl-alt-y");
    frames(&mut main);
    assert_eq!(toasts(&mut main), before + 1, "the unbound toast");
    let after = app.read_with(&main, |app, cx| {
        (
            app.ai_compose.apply_in_place.is_none(),
            app.composer.read(cx).value().to_string(),
        )
    });
    assert_eq!(after, (true, draft), "nothing was sent or replaced");
}
