use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext, px, size};
use quill::connect::ConnectBlocker;
use quill::disk_guard::FLOORS;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::settings_catalog::Page;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    new_demo_app(cx, "ready-chats", 800.)
}

fn new_demo_app(
    cx: &mut TestAppContext,
    demo: &'static str,
    height: f32,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(height)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn read(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, free: Option<u64>) {
    app.update_in(vcx, |app, _, cx| app.apply_disk_reading(free, cx));
}

fn at(free: Option<u64>) -> quill::disk_guard::DataReading {
    quill::disk_guard::DataReading {
        free,
        unwritable: false,
    }
}

/// Two readings below close, as the watcher takes them `CONFIRM_READ` apart.
fn fill(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, free: Option<u64>) {
    read(app, vcx, free);
    read(app, vcx, free);
}

fn shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (bool, bool, u64) {
    app.update_in(vcx, |app, _, cx| {
        (
            app.disk_low_banner(cx).is_some(),
            app.disk_full_view().is_some(),
            app.connection.generation,
        )
    })
}

#[gpui_kit::test]
fn a_ui_test_never_reads_the_real_disk(cx: &mut TestAppContext) {
    let (app, vcx) = new_app(cx);
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| assert_eq!(app.disk.free, None));
}

#[gpui_kit::test]
fn the_bar_shows_only_while_the_disk_is_low(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    read(&app, &mut vcx, Some(FLOORS.warn));
    assert_eq!(shown(&app, &mut vcx), (false, false, start));
    read(&app, &mut vcx, Some(FLOORS.warn - 1));
    assert_eq!(shown(&app, &mut vcx), (true, false, start));
    read(&app, &mut vcx, Some(FLOORS.warn));
    assert_eq!(shown(&app, &mut vcx), (true, false, start), "hysteresis");
    read(&app, &mut vcx, Some(FLOORS.warn_clear));
    assert_eq!(shown(&app, &mut vcx), (false, false, start));
}

#[gpui_kit::test]
fn a_full_disk_shows_the_screen_with_what_is_free_and_what_is_needed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    fill(&app, &mut vcx, Some(FLOORS.close / 2));
    assert!(!shown(&app, &mut vcx).0, "the screen replaces the bar");
    let view = app.update_in(&mut vcx, |app, _, _| app.disk_full_view().unwrap());
    assert_eq!(view.title, tr(keys::Q_DISK_FULL_TITLE));
    let available = format!("Available: {}", super::disk_size(FLOORS.close / 2));
    assert!(view.body.contains(&available), "{}", view.body);
    let needed = format!("Needed: {}", super::disk_size(FLOORS.open));
    assert!(view.body.contains(&needed), "{}", view.body);
}

#[gpui_kit::test]
fn a_refusal_before_the_first_reading_says_only_what_is_needed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_client(Err(ConnectBlocker::DiskFull), cx);
    });
    let view = app.update_in(&mut vcx, |app, _, _| app.disk_full_view().unwrap());
    assert!(!view.body.contains("Available"), "{}", view.body);
    let needed = format!("Needed: {}", super::disk_size(FLOORS.open));
    assert!(view.body.contains(&needed), "{}", view.body);
}

#[gpui_kit::test]
fn the_low_bar_opens_the_storage_usage_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_demo_app(cx, "ready-chats", 320.);
    vcx.run_until_parked();
    read(&app, &mut vcx, Some(FLOORS.warn - 1));
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, cx| window.click("disk-low-storage", cx));
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.update(|window, cx| window.simulate_next_frame(cx));
    }
    app.read_with(&vcx, |app, _| {
        assert!(app.settings.open);
        assert_eq!(app.settings.page, Some(Page::DataStorage));
        assert!(
            app.settings.storage_scroll.page.offset().y < px(0.),
            "the storage usage list is below the fold at this height"
        );
    });
}

#[gpui_kit::test]
fn the_low_bar_has_no_button_on_the_sign_in_screen(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_demo_app(cx, "wait-phone", 800.);
    vcx.run_until_parked();
    read(&app, &mut vcx, Some(FLOORS.warn - 1));
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(shown(&app, &mut vcx).0);
    assert!(vcx.update(|window, _| window.try_find("disk-low-storage").is_none()));
}

#[gpui_kit::test]
fn a_full_disk_closes_chat_windows(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    let main_only = vcx.windows().len();
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_chat_in_new_window(ChatId(11), window, cx);
    });
    vcx.run_until_parked();
    assert_eq!(vcx.windows().len(), main_only + 1, "the chat window opened");
    fill(&app, &mut vcx, Some(0));
    vcx.run_until_parked();
    assert_eq!(vcx.windows().len(), main_only);
}

#[gpui_kit::test]
fn going_full_brings_the_main_window_forward_once(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    vcx.deactivate_window();
    fill(&app, &mut vcx, Some(0));
    vcx.run_until_parked();
    assert!(vcx.update(|window, _| window.is_window_active()));
    vcx.deactivate_window();
    fill(&app, &mut vcx, Some(0));
    vcx.run_until_parked();
    assert!(!vcx.update(|window, _| window.is_window_active()), "once");
}

#[gpui_kit::test]
fn a_refused_open_brings_the_main_window_forward(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.deactivate_window();
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_client(Err(ConnectBlocker::DiskFull), cx);
    });
    vcx.run_until_parked();
    assert!(vcx.update(|window, _| window.is_window_active()));
}

#[gpui_kit::test]
fn leaving_a_full_disk_reconnects_once(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    fill(&app, &mut vcx, Some(0));
    assert_eq!(shown(&app, &mut vcx), (false, true, start));
    read(&app, &mut vcx, Some(FLOORS.open - 1));
    assert_eq!(
        shown(&app, &mut vcx),
        (false, true, start),
        "not enough yet"
    );
    read(&app, &mut vcx, None);
    assert_eq!(shown(&app, &mut vcx), (false, true, start), "no reading");
    read(&app, &mut vcx, Some(FLOORS.open));
    assert_eq!(shown(&app, &mut vcx), (true, false, start + 1));
    read(&app, &mut vcx, Some(FLOORS.warn_clear));
    assert_eq!(shown(&app, &mut vcx), (false, false, start + 1));
}

#[gpui_kit::test]
fn a_refused_open_holds_the_screen_until_the_open_floor(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_client(Err(ConnectBlocker::DiskFull), cx);
        assert!(app.connection.status_note().is_empty());
    });
    assert_eq!(shown(&app, &mut vcx), (false, true, start));
    read(&app, &mut vcx, Some(FLOORS.close));
    assert_eq!(shown(&app, &mut vcx), (false, true, start));
    read(&app, &mut vcx, Some(FLOORS.open));
    assert_eq!(shown(&app, &mut vcx), (true, false, start + 1));
}

#[gpui_kit::test]
fn recovering_while_locked_connects_at_the_unlock(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    fill(&app, &mut vcx, Some(0));
    app.update_in(&mut vcx, |app, _, _| app.account.passcode.locked = true);
    read(&app, &mut vcx, Some(FLOORS.warn_clear));
    assert_eq!(shown(&app, &mut vcx), (false, false, start));
    assert!(app.update_in(&mut vcx, |app, _, _| app.account.passcode.deferred_connect));
}

#[gpui_kit::test]
fn a_client_opened_on_a_low_disk_holds_automatic_downloads_at_once(cx: &mut TestAppContext) {
    use quill::connect::{ConnectDriver, RecordingSender, prepare_connect};
    use quill::credentials::TelegramCredentials;
    use quill::ids::AccountKey;
    use quill::platform::MemorySecretStore;
    use quill::state::Session;
    let (app, mut vcx) = new_app(cx);
    read(&app, &mut vcx, Some(FLOORS.warn - 1));
    let credentials = TelegramCredentials {
        api_id: 99,
        api_hash: "unit-test-hash-not-for-network".into(),
    };
    let dir = quill::scratch::unique_dir("quill-disk-seed");
    let prepared = prepare_connect(
        &dir,
        AccountKey::primary(),
        &MemorySecretStore::new(),
        &credentials,
    )
    .expect("prepare");
    let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
        std::sync::Arc::new(quill::diagnostics::MemorySink::new());
    let session = Session::new(AccountKey::primary(), sink);
    let mut driver = ConnectDriver::new(
        session,
        std::sync::Arc::new(RecordingSender::new()),
        credentials,
        prepared,
    );
    assert!(!driver.session.media.disk_holds_auto_downloads);
    app.read_with(&vcx, |app, _| app.disk.seed(&mut driver));
    assert!(driver.session.media.disk_holds_auto_downloads);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn more_accounts_need_more_room_on_the_full_screen(cx: &mut TestAppContext) {
    use quill::disk_guard::{DataUse, DiskFloors};
    let (app, mut vcx) = new_app(cx);
    let six = DiskFloors::for_data(DataUse {
        accounts: 6,
        ..DataUse::ONE
    });
    for _ in 0..2 {
        app.update_in(&mut vcx, |app, _, cx| {
            app.apply_data_reading(at(Some(FLOORS.close)), six, cx)
        });
    }
    let view = app.update_in(&mut vcx, |app, _, _| app.disk_full_view().unwrap());
    assert!(
        view.body
            .contains(&format!("Needed: {}", super::disk_size(six.open))),
        "{}",
        view.body
    );
}

#[gpui_kit::test]
fn one_low_reading_closes_nothing_until_the_next_agrees(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    let wait = app.update_in(&mut vcx, |app, _, cx| {
        app.apply_data_reading(at(Some(0)), FLOORS, cx)
    });
    assert_eq!(wait, quill::disk_guard::CONFIRM_READ);
    assert_eq!(shown(&app, &mut vcx), (false, false, start), "not full yet");
    read(&app, &mut vcx, Some(FLOORS.warn_clear));
    read(&app, &mut vcx, Some(0));
    assert_eq!(
        shown(&app, &mut vcx),
        (false, false, start),
        "the count restarted"
    );
    read(&app, &mut vcx, Some(0));
    assert!(shown(&app, &mut vcx).1, "two in a row");
}

#[gpui_kit::test]
fn a_disk_falling_fast_is_read_sooner(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let mib = 1u64 << 20;
    let first = app.update_in(&mut vcx, |app, _, cx| {
        app.apply_data_reading(at(Some(FLOORS.close + 20_000 * mib)), FLOORS, cx)
    });
    assert_eq!(first, std::time::Duration::from_secs(30));
    let fell = 30 * 512 * mib;
    let next = app.update_in(&mut vcx, |app, _, cx| {
        app.apply_data_reading(at(Some(FLOORS.close + 20_000 * mib - fell)), FLOORS, cx)
    });
    assert!(
        next < std::time::Duration::from_secs(10),
        "a fall of 512 MiB/s reads again in {next:?}"
    );
}

#[gpui_kit::test]
fn a_data_folder_that_cannot_be_written_closes_with_its_own_screen(cx: &mut TestAppContext) {
    use quill::disk_guard::DataReading;
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    let gone = DataReading {
        free: Some(FLOORS.warn_clear),
        unwritable: true,
    };
    for _ in 0..2 {
        app.update_in(&mut vcx, |app, _, cx| {
            app.apply_data_reading(gone, FLOORS, cx)
        });
    }
    let view = app.update_in(&mut vcx, |app, _, _| app.disk_full_view().unwrap());
    assert_eq!(view.title, tr(keys::Q_DISK_UNWRITABLE_TITLE));
    assert!(!view.body.contains("Needed"), "{}", view.body);
    app.update_in(&mut vcx, |app, _, cx| {
        app.apply_data_reading(at(Some(FLOORS.warn_clear)), FLOORS, cx)
    });
    assert_eq!(shown(&app, &mut vcx), (false, false, start + 1), "back");
}

#[gpui_kit::test]
fn a_call_held_open_at_full_keeps_the_app_with_the_bar(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    fill(&app, &mut vcx, Some(0));
    app.update_in(&mut vcx, |app, _, _| app.disk.held_for_call = true);
    assert!(shown(&app, &mut vcx).0, "the bar");
    assert!(!shown(&app, &mut vcx).1, "no full screen during the call");
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(
        vcx.update(|window, _| window.try_find("disk-low-storage").is_some()),
        "the bar is on screen"
    );
    read(&app, &mut vcx, Some(FLOORS.warn_clear));
    app.read_with(&vcx, |app, _| assert!(!app.disk.held_for_call));
}

#[gpui_kit::test]
fn each_time_the_disk_fills_sends_one_notice(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let notices = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.disk.full_notices)
    };
    fill(&app, &mut vcx, Some(0));
    fill(&app, &mut vcx, Some(0));
    assert_eq!(notices(&app, &mut vcx), 1, "one per edge");
    read(&app, &mut vcx, Some(FLOORS.warn_clear));
    fill(&app, &mut vcx, Some(0));
    assert_eq!(notices(&app, &mut vcx), 2);
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_client(Err(ConnectBlocker::DiskFull), cx);
    });
    assert_eq!(notices(&app, &mut vcx), 2, "the screen was up already");
}

#[test]
fn the_full_disk_notice_offers_no_reply_or_mark_as_read() {
    for unwritable in [false, true] {
        let notice = super::full_disk_notice(unwritable);
        assert!(!notice.offers_actions(false), "{notice:?}");
        assert!(quill::notify::action_buttons(&notice, false).is_empty());
    }
}

#[gpui_kit::test]
fn the_full_screen_links_to_the_cached_files(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_demo_app(cx, "wait-phone", 800.);
    let folder = quill::scratch::unique_dir("quill-disk-cache");
    app.update_in(&mut vcx, |app, _, _| {
        app.disk.cache_folder = Some(folder.clone())
    });
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(vcx.update(|window, _| window.try_find("disk-full-reveal").is_none()));
    fill(&app, &mut vcx, Some(0));
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(vcx.update(|window, _| window.try_find("disk-full-reveal").is_some()));
    app.update_in(&mut vcx, |app, _, _| app.disk.cache_folder = None);
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(
        vcx.update(|window, _| window.try_find("disk-full-reveal").is_none()),
        "no folder, no link"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[gpui_kit::test]
fn the_storage_list_stays_in_view_when_rows_above_arrive(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_demo_app(cx, "ready-chats", 320.);
    vcx.run_until_parked();
    let frames = |vcx: &mut VisualTestContext| {
        for _ in 0..3 {
            vcx.run_until_parked();
            vcx.update(|window, cx| window.render_frame(cx));
            vcx.update(|window, cx| window.simulate_next_frame(cx));
        }
    };
    let rows = |app: &mut QuillApp, arrived: bool| {
        if let Some(demo) = app.demo_session.as_mut() {
            demo.settings.data_storage.seeded = arrived;
            demo.settings.auto_download_presets_loading = !arrived;
        }
    };
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_local_storage(cx);
        rows(app, false);
    });
    frames(&mut vcx);
    let before = app.read_with(&vcx, |app, _| app.settings.storage_scroll.page.offset().y);
    app.update_in(&mut vcx, |app, _, cx| {
        rows(app, true);
        cx.notify();
    });
    frames(&mut vcx);
    let after = app.read_with(&vcx, |app, _| app.settings.storage_scroll.page.offset().y);
    assert!(
        after < before,
        "the rows pushed the list down and the page followed: {before:?} -> {after:?}"
    );
}

#[gpui_kit::test]
fn manage_local_storage_shows_the_message_database_size(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, cx| app.open_local_storage(cx));
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(vcx.debug_bounds("storage-database-size").is_none());
    app.update_in(&mut vcx, |app, _, cx| {
        app.disk.database_bytes = Some(3 << 20);
        cx.notify();
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(vcx.debug_bounds("storage-database-size").is_some());
}
