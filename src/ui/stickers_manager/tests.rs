//! UI tests on the `ready-stickers-manager` demo: tabs, rows with their
//! buttons, remove with Undo, drag reorder, Add, opening a set, what closing
//! the box applies, and the Settings entry.

use super::{Kind, ManagerTab};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::settings_catalog::Page;
use quill::state::{MasksTab, StickerTab};
use quill::telegram::envelope::StickerSetInfo;
use std::time::Duration;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_named(
    cx: &mut TestAppContext,
    kind: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-stickers-manager");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(kind);
    let handle = cx.open_window(size(px(1200.), px(3200.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    vcx.executor().advance_clock(Duration::from_millis(600));
    frames(&mut vcx);
    (app, shell, vcx)
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    open_named(cx, "ready-stickers-manager")
}

fn has(vcx: &mut VisualTestContext, id: &str) -> bool {
    vcx.update(|window, _| window.try_find(id.to_string()).is_some())
}

fn click(vcx: &mut VisualTestContext, id: &str) {
    vcx.update(|window, cx| window.click(id.to_string(), cx));
    frames(vcx);
}

fn installed_ids(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<i64> {
    app.read_with(vcx, |app, _| {
        app.session()
            .expect("session")
            .stickers
            .stickers
            .sets
            .iter()
            .map(|set| set.id)
            .collect()
    })
}

fn select(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, kind: Kind, tab: ManagerTab) {
    app.update(vcx, |app, cx| app.manager_select_tab(kind, tab, cx));
    frames(vcx);
}

fn close_box(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| app.close_stickers_manager(cx));
    frames(vcx);
}

#[gpui_kit::test]
fn installed_rows_have_a_handle_a_count_and_a_remove_button(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    assert_eq!(
        shell.read_with(vcx, |shell, _| shell.open_dialog),
        Some(DialogKind::ArchivedStickers)
    );
    for id in [101, 102, 103, 104] {
        assert!(has(vcx, &format!("manager-open-{id}")));
        assert!(has(vcx, &format!("manager-remove-{id}")));
        assert!(!has(vcx, &format!("manager-add-{id}")));
    }
    // The composer picker's management is gone from the box.
    assert!(!has(vcx, "sticker-panel-more"));
    assert!(!has(vcx, "more-archived-stickers"));
    let view = app.read_with(vcx, |app, _| app.manager_view(Kind::Stickers));
    assert_eq!(
        view.tabs,
        vec![
            ManagerTab::Installed,
            ManagerTab::Trending,
            ManagerTab::Archived
        ]
    );
}

#[gpui_kit::test]
fn counts_are_plural_forms_not_a_fixed_english_string(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut()
            && let Some(set) = session.stickers.stickers.sets.first_mut()
        {
            set.size = 1;
        }
        cx.notify();
    });
    frames(vcx);
    assert_eq!(
        quill::lang::trn(quill::lang::keys::STICKERS_COUNT, 1),
        "1 sticker"
    );
    assert_eq!(
        quill::lang::trn(quill::lang::keys::STICKERS_COUNT, 24),
        "24 stickers"
    );
    assert_eq!(
        quill::lang::trn(quill::lang::keys::MASKS_COUNT, 1),
        "1 mask"
    );
}

#[gpui_kit::test]
fn trending_has_add_and_added_with_unread_sets_counted(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    select(&app, vcx, Kind::Stickers, ManagerTab::Trending);
    assert_eq!(
        app.read_with(vcx, |app, _| app
            .session()
            .expect("s")
            .stickers
            .stickers
            .tab),
        StickerTab::Trending
    );
    assert!(has(vcx, "manager-add-201"));
    assert!(has(vcx, "manager-add-202"));
    assert!(has(vcx, "manager-added-102"));
    assert!(!has(vcx, "manager-remove-201"));
    assert_eq!(
        app.read_with(vcx, |app, _| app.manager_view(Kind::Stickers).unread),
        2
    );
}

#[gpui_kit::test]
fn archived_rows_offer_add(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    select(&app, vcx, Kind::Stickers, ManagerTab::Archived);
    assert!(has(vcx, "manager-add-301"));
    assert!(has(vcx, "manager-add-302"));
    assert!(!has(vcx, "manager-remove-301"));
}

#[gpui_kit::test]
fn removing_marks_the_row_and_undo_takes_the_mark_off(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    click(vcx, "manager-remove-102");
    assert!(has(vcx, "manager-undo-102"));
    assert!(!has(vcx, "manager-remove-102"));
    assert!(app.read_with(vcx, |app, _| {
        app.pickers.sticker_manager.stickers.is_removed(102)
    }));
    // Nothing is removed until the box closes.
    assert_eq!(installed_ids(&app, vcx), vec![101, 102, 103, 104]);
    click(vcx, "manager-undo-102");
    assert!(has(vcx, "manager-remove-102"));
    assert!(!has(vcx, "manager-undo-102"));
    assert!(!app.read_with(vcx, |app, _| {
        app.pickers.sticker_manager.stickers.is_removed(102)
    }));
}

#[gpui_kit::test]
fn a_removal_marked_on_trending_shows_on_installed_and_add_undoes_it(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    select(&app, vcx, Kind::Stickers, ManagerTab::Trending);
    click(vcx, "manager-added-102");
    assert!(has(vcx, "manager-add-102"), "the pill turns into Add");
    select(&app, vcx, Kind::Stickers, ManagerTab::Installed);
    assert!(has(vcx, "manager-undo-102"));
    select(&app, vcx, Kind::Stickers, ManagerTab::Trending);
    click(vcx, "manager-add-102");
    assert!(has(vcx, "manager-added-102"));
    assert_eq!(installed_ids(&app, vcx), vec![101, 102, 103, 104]);
}

#[gpui_kit::test]
fn closing_applies_removals_and_the_dragged_order(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    click(vcx, "manager-remove-102");
    app.update(vcx, |app, cx| {
        app.manager_dropped(Kind::Stickers, 101, 103, cx)
    });
    frames(vcx);
    // The dragged row takes the slot of the row it lands on, at once.
    let order: Vec<i64> = app.read_with(vcx, |app, _| {
        let view = app.manager_view(Kind::Stickers);
        let ids: Vec<i64> = view.installed.iter().map(|set| set.id).collect();
        app.pickers.sticker_manager.stickers.arranged(&ids)
    });
    assert_eq!(order, vec![102, 103, 101, 104]);
    close_box(&app, vcx);
    assert_eq!(installed_ids(&app, vcx), vec![103, 101, 104]);
    assert!(
        app.read_with(vcx, |app, _| !app
            .pickers
            .sticker_manager
            .stickers
            .is_removed(102)),
        "the marks are forgotten"
    );
    vcx.executor().advance_clock(Duration::from_millis(300));
    frames(vcx);
    assert_eq!(shell.read_with(vcx, |shell, _| shell.open_dialog), None);
}

#[gpui_kit::test]
fn a_row_opens_the_set_box(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx);
    let vcx = &mut vcx;
    click(vcx, "manager-open-103");
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu_ui.sticker_set_open));
}

#[gpui_kit::test]
fn masks_open_in_their_own_box_with_masks_and_archived_tabs(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_named(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        let set = |id: i64, title: &str| StickerSetInfo {
            id,
            title: title.into(),
            name: title.into(),
            size: 3,
            is_installed: true,
            is_official: false,
            is_owned: false,
            covers: Vec::new(),
            cover_files: Vec::new(),
            is_viewed: true,
        };
        if let Some(session) = app.demo_session.as_mut() {
            session.stickers.masks.installed = vec![set(501, "Party hats"), set(502, "Beards")];
            session.stickers.masks.installed_loaded = true;
            session.stickers.masks.archived = vec![set(503, "Old masks")];
            session.stickers.masks.archived_loaded = true;
        }
        app.open_mask_sets(cx);
    });
    frames(vcx);
    vcx.executor().advance_clock(Duration::from_millis(300));
    frames(vcx);
    assert_eq!(
        shell.read_with(vcx, |shell, _| shell.open_dialog),
        Some(DialogKind::MaskSets)
    );
    assert!(has(vcx, "manager-open-501"));
    assert!(has(vcx, "manager-open-502"));
    let view = app.read_with(vcx, |app, _| app.manager_view(Kind::Masks));
    assert_eq!(view.tabs, vec![ManagerTab::Installed, ManagerTab::Archived]);
    click(vcx, "manager-remove-501");
    assert!(has(vcx, "manager-undo-501"));
    select(&app, vcx, Kind::Masks, ManagerTab::Archived);
    assert!(has(vcx, "manager-add-503"));
    assert_eq!(
        app.read_with(vcx, |app, _| app.session().expect("s").stickers.masks.tab),
        MasksTab::Archived
    );
    app.update(vcx, |app, cx| app.close_mask_sets(cx));
    frames(vcx);
    let left: Vec<i64> = app.read_with(vcx, |app, _| {
        app.session()
            .expect("s")
            .stickers
            .masks
            .installed
            .iter()
            .map(|set| set.id)
            .collect()
    });
    assert_eq!(left, vec![502]);
}

#[gpui_kit::test]
fn settings_chats_page_opens_the_box_and_holds_the_suggestion_choice(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_named(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Chats));
        cx.notify();
    });
    frames(vcx);
    assert!(has(vcx, "settings-row-ArchivedStickers"));
    app.update(vcx, |app, cx| {
        app.set_sticker_suggest_mode(quill::sticker_suggest::StickerSuggestMode::None, cx)
    });
    assert_eq!(
        app.read_with(vcx, |app, _| app
            .session()
            .expect("s")
            .settings
            .media_prefs
            .sticker_suggest_mode),
        quill::sticker_suggest::StickerSuggestMode::None
    );
    click(vcx, "settings-row-ArchivedStickers");
    vcx.executor().advance_clock(Duration::from_millis(300));
    frames(vcx);
    assert_eq!(
        shell.read_with(vcx, |shell, _| shell.open_dialog),
        Some(DialogKind::ArchivedStickers)
    );
    assert!(app.read_with(vcx, |app, _| app.settings.open));
}
