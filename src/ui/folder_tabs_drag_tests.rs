use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, ElementId, Entity, TestAppContext, VisualTestContext, px, size};
use quill::folder_icons::FolderTabsView;
use quill::folder_order::FolderTab;
use quill::telegram::envelope::ChatFolderInfo;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn folder(id: i32, name: &str) -> ChatFolderInfo {
    ChatFolderInfo {
        id,
        name: name.into(),
        icon_name: String::new(),
        color_id: -1,
        is_shareable: false,
        has_my_invite_links: false,
    }
}

fn open(
    cx: &mut TestAppContext,
    premium: bool,
    view: FolderTabsView,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, cx| {
        app.appearance.folder_tabs_view = view;
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.premium_option = Some(premium);
            session.chat_list.chat_folders = vec![folder(7, "Work"), folder(8, "Family")];
            session.chat_list.main_list_position = 0;
        }
        cx.notify();
    });
    frames(&mut vcx);
    (app, vcx)
}

fn tabs(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<FolderTab> {
    app.update(vcx, |app, _| {
        app.session()
            .map(|s| s.chat_list.folder_order().tabs())
            .unwrap_or_default()
    })
}

fn rail_item(ix: usize) -> ElementId {
    ("folder-rail-item", ix).into()
}

fn drag_in_strip(vcx: &mut VisualTestContext, from: usize, to: usize) {
    vcx.update(|window, cx| {
        window
            .within("folder-tabs")
            .drag_to(ElementId::from(from), ElementId::from(to), cx)
    });
    frames(vcx);
}

#[gpui_kit::test]
fn premium_drags_all_chats_after_a_folder_in_the_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true, FolderTabsView::Top);
    let vcx = &mut vcx;
    drag_in_strip(vcx, 0, 2);
    assert_eq!(
        tabs(&app, vcx),
        vec![FolderTab::Folder(7), FolderTab::Folder(8), FolderTab::Main]
    );
    drag_in_strip(vcx, 2, 1);
    assert_eq!(
        tabs(&app, vcx),
        vec![FolderTab::Folder(7), FolderTab::Main, FolderTab::Folder(8)]
    );
}

#[gpui_kit::test]
fn free_account_keeps_all_chats_first(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, false, FolderTabsView::Top);
    let vcx = &mut vcx;
    drag_in_strip(vcx, 0, 2);
    assert_eq!(
        tabs(&app, vcx),
        vec![FolderTab::Main, FolderTab::Folder(7), FolderTab::Folder(8)],
        "All chats does not move"
    );
    drag_in_strip(vcx, 2, 0);
    assert_eq!(
        tabs(&app, vcx),
        vec![FolderTab::Main, FolderTab::Folder(7), FolderTab::Folder(8)],
        "nothing takes its slot"
    );
    drag_in_strip(vcx, 2, 1);
    assert_eq!(
        tabs(&app, vcx),
        vec![FolderTab::Main, FolderTab::Folder(8), FolderTab::Folder(7)],
        "folders still reorder"
    );
}

#[gpui_kit::test]
fn premium_drags_all_chats_in_the_folder_column(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true, FolderTabsView::Left);
    let vcx = &mut vcx;
    vcx.update(|window, cx| window.drag_to(rail_item(0), rail_item(1), cx));
    frames(vcx);
    assert_eq!(
        tabs(&app, vcx),
        vec![FolderTab::Folder(7), FolderTab::Main, FolderTab::Folder(8)]
    );
}

#[gpui_kit::test]
fn a_move_from_another_device_reorders_the_tabs(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, true, FolderTabsView::Left);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.chat_list.main_list_position = 2;
        }
        cx.notify();
    });
    frames(vcx);
    let last = vcx.update(|window, _| {
        window
            .try_find(rail_item(2))
            .map(|e| e.label().map(str::to_owned))
    });
    assert_eq!(last.flatten().as_deref(), Some("All"));
}
