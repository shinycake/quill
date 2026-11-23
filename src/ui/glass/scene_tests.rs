//! Glass in a drawn window: with the look solid no lens is ever painted;
//! with glass effects on, an open menu is exactly one lens, and a steady
//! main window stays inside the budget (at most eight lenses in two
//! bands). Counted from the frame's scene (`Window::rendered_glass`).

use crate::ui::app::QuillApp;
use crate::ui::chat_list_keys::ListRow;
use crate::ui::chat_row::ChatListItem;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, point, px, size};
use quill::glass::{GlassLook, Material, Motion};
use quill::ids::ChatId;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot
        .borrow()
        .clone()
        .expect("the window builder fills the slot");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn glass(vcx: &mut VisualTestContext) -> (usize, usize) {
    vcx.update(|window, _| window.rendered_glass())
}

fn set_material(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, material: Material) {
    app.update(vcx, |app, cx| {
        app.set_appearance(cx, |appearance| appearance.material = material);
    });
    frames(vcx);
}

fn open_archive_menu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| {
        app.message_ui.menu.open_archive(point(px(40.), px(80.)));
        cx.notify();
    });
    frames(vcx);
}

#[gpui_kit::test]
fn the_solid_look_paints_no_glass(cx: &mut TestAppContext) {
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-glass-solid"));
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_material(&app, vcx, Material::Solid);
    open_archive_menu(&app, vcx);
    assert_eq!(glass(vcx), (0, 0), "switch off: no lens anywhere");
    let look = vcx.update(|_, cx| crate::ui::glass::look(cx));
    assert!(!look.is_glass());
}

#[gpui_kit::test]
fn an_open_menu_is_one_lens_with_glass_on(cx: &mut TestAppContext) {
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-glass-menu"));
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    set_material(&app, vcx, Material::Glass);
    let look = vcx.update(|_, cx| crate::ui::glass::look(cx));
    assert_eq!(look, GlassLook::Glass(Motion::Full));
    let (steady, bands) = glass(vcx);
    assert!(steady <= 8, "steady main window: {steady} lenses");
    assert!(bands <= 2, "steady main window: {bands} bands");
    open_archive_menu(&app, vcx);
    let (with_menu, _) = glass(vcx);
    assert_eq!(with_menu, steady + 1, "the open menu adds one lens");
    set_material(&app, vcx, Material::Solid);
    assert_eq!(glass(vcx), (0, 0), "switching off removes every lens");
}

/// The highlighted chat in the list.
fn highlighted(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.update(vcx, |app, _| {
        app.chat_list.items.iter().find_map(|item| match item {
            ChatListItem::Chat { id, .. } if app.chat_list.keys.list_highlighted(ListRow::Chat(*id)) => {
                Some(*id)
            }
            _ => None,
        })
    })
}

/// Clicks just below the bar and says which chat opened.
fn click_below_bar(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    bar: gpui_kit::Bounds<gpui_kit::Pixels>,
) -> Option<ChatId> {
    vcx.simulate_click(
        point(bar.left() + px(120.), bar.bottom() + px(3.)),
        gpui_kit::Modifiers::default(),
    );
    frames(vcx);
    app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat_id()))
}

#[gpui_kit::test]
fn the_chat_list_scrolls_under_a_glass_header_and_the_keys_stay_below_it(
    cx: &mut TestAppContext,
) {
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-glass-bar"));
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            crate::ui::demo::add_stress_chats(session, 40);
        }
        app.notify_sidebar(cx);
        cx.notify();
    });
    set_material(&app, vcx, Material::Solid);
    assert!(vcx.debug_bounds("chat-list-glass-bar").is_none(), "solid: no bar");
    set_material(&app, vcx, Material::Glass);
    let Some(bar) = vcx.debug_bounds("chat-list-glass-bar") else {
        panic!("glass on: the header floats as a bar");
    };
    let inset = app.update(vcx, |app, _| app.chat_list.header_inset.get());
    assert_eq!(inset, bar.size.height, "the rows' padding is the bar's height");
    let Some(rows) = vcx.debug_bounds("chat-list-rows") else {
        panic!("the rows are drawn");
    };
    assert!(rows.top() <= bar.top(), "the rows run under the bar");
    // The first row starts at the bar's lower edge, not under it.
    let first = app.update(vcx, |app, _| {
        app.chat_list.items.iter().find_map(|item| match item {
            ChatListItem::Chat { id, .. } => Some(*id),
            _ => None,
        })
    });
    assert_eq!(click_below_bar(&app, vcx, bar), first);
    // Walking up from the end, the highlight scrolls in below the bar:
    // the row just under its edge is the highlighted one.
    app.update_in(vcx, |app, window, cx| window.focus(&app.focus_sidebar, cx));
    frames(vcx);
    vcx.simulate_keystrokes("end");
    frames(vcx);
    for _ in 0..30 {
        vcx.simulate_keystrokes("up");
        frames(vcx);
    }
    let Some(target) = highlighted(&app, vcx) else {
        panic!("a row is highlighted");
    };
    assert_eq!(click_below_bar(&app, vcx, bar), Some(target));
}
