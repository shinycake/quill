// Release UI builds on Windows are GUI-subsystem so launching Quill does not
// open a console window; `attach_parent_console` restores CLI output
// (`--version`, ...) when started from a terminal.
#![cfg_attr(
    all(windows, feature = "ui", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(feature = "ui")]
mod ui;
#[cfg(feature = "ui")]
use ui::link_inbox::{install_link_inbox, raise_main_window};

#[cfg(feature = "ui")]
mod quill_assets;
#[cfg(feature = "ui")]
use quill_assets::QuillAssets;

/// A GUI-subsystem process starts without standard handles. When launched from
/// a terminal, attach to its console and point stdout/stderr at it so CLI flags
/// print. Redirected handles (pipes, files) are left untouched.
#[cfg(windows)]
fn attach_parent_console() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };
    const GENERIC_WRITE: u32 = 0x4000_0000;
    // SAFETY: plain Win32 calls with valid arguments; failures are ignored.
    unsafe {
        let has = |which| {
            let handle = GetStdHandle(which);
            !handle.is_null() && handle != INVALID_HANDLE_VALUE
        };
        let (need_out, need_err) = (!has(STD_OUTPUT_HANDLE), !has(STD_ERROR_HANDLE));
        if !(need_out || need_err) || AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        let conout: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        let console = CreateFileW(
            conout.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if console != INVALID_HANDLE_VALUE {
            if need_out {
                SetStdHandle(STD_OUTPUT_HANDLE, console);
            }
            if need_err {
                SetStdHandle(STD_ERROR_HANDLE, console);
            }
        }
    }
}

/// Exits when this binary carries the test feature (cargo gives it to every
/// target while tests are built): its app root is a temp folder, so starting
/// the app could sign in there and wipe the folder at quit.
#[allow(clippy::exit, reason = "refuses to start a test-feature binary")]
fn refuse_test_build() {
    #[cfg(feature = "test-support")]
    {
        eprintln!(
            "This Quill binary was built with the test-support feature and cannot start the app. Build it without that feature."
        );
        std::process::exit(2);
    }
}

fn main() {
    quill::startup_trace::mark("main");
    quill::crash_log::install_panic_hook();
    quill::fd_limit::raise_open_file_limit();
    #[cfg(windows)]
    attach_parent_console();
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--version") {
        println!("Quill {}", quill::version::APP);
        return;
    }
    if args.get(1).is_some_and(|a| a == "--release-asset-name") {
        println!("{}", quill::updater::binary_asset_name());
        return;
    }
    if args.get(1).is_some_and(|a| a == "--build-info") {
        println!("{}", if cfg!(feature = "ui") { "ui" } else { "core" });
        return;
    }
    // A binary built with the test feature keeps its data in a temp folder, so
    // it must never start the app: it could sign in there and wipe the
    // folder at quit.
    refuse_test_build();
    if args.get(1).is_some_and(|a| a == "--apply-update") {
        let result = args
            .get(2)
            .ok_or_else(|| std::io::Error::other("Missing update plan"))
            .and_then(|p| quill::update_install::apply_update(std::path::Path::new(p)));
        if let Err(error) = result {
            eprintln!("Update failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    if args.get(1).is_some_and(|a| a == "--video-probe") {
        std::process::exit(quill::video_decode::probe_cli(
            args.get(2).map(String::as_str),
        ));
    }
    if let Some(code) = quill::dev_cli::exit_code(&args) {
        std::process::exit(code);
    }

    #[cfg(feature = "ui")]
    {
        if args.iter().any(|a| a == "--check-updates") {
            let state = quill::updater::check_latest_release();
            println!("{}", state.label());
            std::process::exit(i32::from(matches!(
                state,
                quill::updater::UpdateState::Failed(_)
            )));
        }
        ui_main(&args);
    }

    #[cfg(not(feature = "ui"))]
    {
        eprintln!("quill: UI not compiled. Pass --connect-smoke, or rebuild with --features ui.");
        std::process::exit(2);
    }
}

/// The main window's opening bounds: where the user left it, when that
/// still lands on a connected display (its title strip must be reachable),
/// else a centered default.
#[cfg(feature = "ui")]
fn restored_window_bounds(
    state: Option<quill::settings::WindowState>,
    cx: &gpui_kit::App,
) -> gpui_kit::WindowBounds {
    use gpui_kit::*;
    let default = Bounds::centered(None, size(px(1200.), px(760.)), cx);
    let Some(state) = state else {
        return WindowBounds::Windowed(default);
    };
    let bounds = Bounds {
        origin: point(px(state.x), px(state.y)),
        size: size(px(state.width), px(state.height)),
    };
    let title_strip = Bounds {
        origin: bounds.origin,
        size: size(bounds.size.width, px(40.)),
    };
    let reachable = cx
        .displays()
        .iter()
        .any(|display| display.bounds().intersects(&title_strip));
    match (reachable, state.maximized) {
        (false, _) => WindowBounds::Windowed(default),
        (true, true) => WindowBounds::Maximized(bounds),
        (true, false) => WindowBounds::Windowed(bounds),
    }
}

#[cfg(feature = "ui")]
fn ui_main(args: &[String]) {
    use gpui_kit::*;

    if let Some(demo) = parse_screenshot_demo(args) {
        run_screenshot_demo(demo);
        return;
    }

    // An update is being installed beside this executable: its helper starts
    // Quill when it is done.
    if quill::update_install::update_in_progress() {
        return;
    }

    // Single instance (tdesktop `Core::Sandbox`): a second launch hands its
    // links to the running Quill and exits before touching the account
    // database or the media caches. `--quit` with no Quill running is done.
    use quill::single_instance::Launch;
    let instance = single_instance(args);
    let quit = quill::start_files::quit_requested(args);
    if let Some((title, body)) = instance.alert(quit) {
        ui::startup_failure::tell(title, body);
    }
    if matches!(instance, Launch::Forwarded | Launch::OwnerStuck) || quit {
        return;
    }

    // A staged update installs at the next start (tdesktop `checkReadyUpdate`),
    // only once no other Quill can be running from this install.
    if instance == Launch::Primary && quill::update_install::apply_on_start(args) {
        return;
    }

    // Crash handlers go in before GPUI, AppKit and TDLib start (crash_log).
    quill::crash_log::begin_app_run();

    // Demo windows can run beside the live app without touching its private caches.
    quill::local_path::sweep_media_caches();
    // Old editor working files go once per run, here and nowhere else.
    quill::scratch::sweep_work_dirs_once();

    // `parity:platform-deep-links`: a `t.me` / `tg:` launch argument is
    // stashed on the app and resolved via `getDeepLinkInfo` once auth is
    // Ready (see `ui::deep_links`).
    let pending_deep_link = quill::connect::detect_deep_link_arg(args);
    // Files named on the command line ("Send to", `quill photo.png`) wait
    // for a chat to be picked (see `ui::start_files_ui`).
    quill::start_files::push_launch_files(args);

    let credentials = quill::credentials::load();
    ui::early_connect::begin(credentials.clone());
    let appearance = ui::QuillApp::load_appearance();
    quill::tray::set_tray_enabled(appearance.show_tray_icon);
    ui::window_frame::set_native(appearance.native_window_frame);
    let start_in_tray =
        args.iter().any(|arg| arg == "--start-minimized") || appearance.start_in_tray;
    let application = quill_application(appearance.interface_scale_pct).with_assets(QuillAssets);
    // macOS delivers `tg:` / `t.me` URLs (Info.plist CFBundleURLTypes) here,
    // both on cold launch and to the running app; Linux/Windows pass them
    // as argv instead (handled above and via the single-instance socket).
    application.on_open_urls(|urls| {
        for url in urls {
            if !quill::deep_link_inbox::push_external_link(&url) {
                quill::start_files::push_file_url(&url);
            }
        }
    });
    quill::startup_trace::mark("app_launch");
    application.run(move |cx| {
        quill::startup_trace::mark("app_run");
        ui::early_connect::open_if_ready();
        cx.set_app_identity("org.shinycake.quill", "Quill");
        #[cfg(windows)]
        quill::notify::register_toast_icon("org.shinycake.quill");
        gpui_kit::init(cx);
        // kit Phase 8: the kit defaults to its light theme on init;
        // Quill boots dark (kit dialogs match the app from here on).
        ui::set_theme_mode(ui::startup_theme_mode(), None, cx);
        // stories-high-contrast: screenshot demos can opt into the
        // high-contrast palette with `QUILL_DEMO_THEME=high-contrast`.
        ui::set_high_contrast(std::env::var("QUILL_DEMO_THEME").as_deref() == Ok("high-contrast"));
        // kit Phase 9: honor the OS reduce-motion preference.
        ui::reduce_motion_probe::apply_at_startup(cx);
        ui::bind_keys(cx);
        cx.on_system_wake(|_| quill::net_watch::system_woke())
            .detach();
        // kit Phase 7: File / Edit / View / Window / Help — native on
        // macOS, kit `AppMenuBar` data on Linux/Windows.
        ui::setup_app_menus(cx);
        ui::flush_settings_on_quit(cx);
        let window_bounds = restored_window_bounds(quill::settings::load_window_state(), cx);
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    app_id: Some("org.shinycake.quill".into()),
                    show: !start_in_tray,
                    focus: !start_in_tray,
                    ..ui::window_frame::window_options("Quill")
                },
                move |window, cx| {
                    quill::startup_trace::mark("window_open");
                    ui::early_connect::open_if_ready();
                    ui::crash_notice::install(window, cx);
                    let view = cx.new(|cx| {
                        let mut app = ui::QuillApp::new(window, cx, credentials.clone());
                        app.pending_deep_link = pending_deep_link.clone();
                        app
                    });
                    quill::startup_trace::mark("app_new");
                    install_main_window_tray(window, cx, &view);
                    install_link_inbox(window, cx, &view);
                    if window.focused(cx).is_none() {
                        window.focus(&view.focus_handle(cx), cx);
                    }
                    if start_in_tray {
                        // No tray host must never leave the only window
                        // inaccessible. Registration runs off-thread; poll its
                        // outcome without blocking launch.
                        let watch = view.downgrade();
                        let handle = window.window_handle();
                        cx.spawn(async move |cx| {
                            let mut waited = 0u64;
                            loop {
                                let state = watch.update(cx, |this, _| {
                                    quill::tray::sync_tray(this.session());
                                    quill::tray::tray_startup_reveal(
                                        quill::tray::tray_available(),
                                        quill::tray::tray_registering(),
                                        waited,
                                    )
                                });
                                match state {
                                    Ok(None) => {}
                                    Ok(Some(true)) => {
                                        let _ = handle.update(cx, |_, window, cx| {
                                            cx.activate(true);
                                            window.activate_window();
                                        });
                                        break;
                                    }
                                    _ => break,
                                }
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(100))
                                    .await;
                                waited += 100;
                            }
                        })
                        .detach();
                    }
                    view.update(cx, |app, cx| app.poll_before_first_frame(cx));
                    // The shell adds Quill's dialog hit-test barrier; Root
                    // hosts the kit dialog and notification layers.
                    let shell = cx.new(|_cx| ui::QuillShell::new(view));
                    cx.new(|cx| gpui_kit::component::Root::new(shell, window, cx))
                },
            )
            .unwrap_or_else(|error| ui::startup_failure::window_failed(&error));
        })
        .detach();
    });
    log::logger().flush();
}

#[cfg(feature = "ui")]
fn single_instance(args: &[String]) -> quill::single_instance::Launch {
    use quill::single_instance::{Endpoint, Launch, acquire};
    let Some(root) = quill::settings::safe_app_root() else {
        return Launch::Unchecked;
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let forwarded = quill::start_files::forwarded_args(args.get(1..).unwrap_or_default(), &cwd);
    let acquired = acquire(&Endpoint::for_root(&root), &forwarded, |args| {
        quill::start_files::handle_forwarded_launch(&args)
    });
    if let Err(error) = &acquired {
        eprintln!("quill: single-instance check: {error}");
    }
    Launch::after(&acquired)
}

#[cfg(feature = "ui")]
fn install_main_window_tray(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    view: &gpui_kit::Entity<ui::QuillApp>,
) {
    #[cfg(windows)]
    {
        // The taskbar overlay badge targets this window's taskbar button.
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            quill::icon_badge::set_native_window(handle.hwnd.get());
        }
    }
    quill::notify_focus::warm();
    // "Show taskbar icon" off: the window starts out of the taskbar.
    view.update(cx, |this, _| this.apply_taskbar_icon(window));
    // The title-bar close button: "Run in the background" hides the app
    // (macOS) or the window (Windows, X11; Wayland can only minimize),
    // otherwise the window closes.
    window.on_window_should_close(cx, {
        let view = view.downgrade();
        move |window, cx| {
            use quill::tray::{CloseOutcome, close_outcome};
            let background = view
                .update(cx, |this, _| this.minimize_to_tray())
                .unwrap_or(false);
            match close_outcome(
                background,
                quill::tray::tray_available(),
                cfg!(target_os = "macos"),
                ui::window_control::hide_supported(window),
            ) {
                CloseOutcome::Quit => true,
                CloseOutcome::HideApp => {
                    cx.hide();
                    false
                }
                CloseOutcome::Hide => {
                    if !ui::window_control::set_visible(window, false) {
                        window.minimize_window();
                    }
                    false
                }
                CloseOutcome::Minimize => {
                    window.minimize_window();
                    false
                }
            }
        }
    });
    // parity:platform-tray-icon — system tray icon with
    // unread count, synced on a 1s UI-thread timer. The
    // tray module no-ops when the count is unchanged or
    // the OS exposes no system tray.
    cx.spawn({
        let tray_view = view.downgrade();
        let tray_window = window.window_handle();
        async move |cx| {
            // Fallback for what no event announces (a tray host that
            // appears late, the menu bar theme); everything else pokes
            // `wake::CHROME` (applied updates, tray menu clicks, settings).
            let mut wait = std::time::Duration::from_secs(10);
            let mut seen = quill::wake::CHROME.epoch();
            loop {
                ui::sleep_or_wake(cx.background_executor(), &quill::wake::CHROME, seen, wait).await;
                // Taken before the work: a poke during it runs the loop again.
                seen = quill::wake::CHROME.epoch();
                wait = std::time::Duration::from_secs(10);
                let alive = tray_view
                    .update(cx, |this, _| {
                        quill::tray::sync_tray(this.session());
                        // parity:platform-app-icon-badge — unread
                        // badge on the app/taskbar icon
                        // (Linux LauncherEntry D-Bus signal,
                        // macOS dock tile, Windows taskbar overlay).
                        quill::icon_badge::sync_icon_badge(this.session())
                    })
                    .is_ok();
                if !alive {
                    break;
                }
                #[cfg(target_os = "macos")]
                let _ = tray_window.update(cx, |_, window, cx| {
                    let enabled = tray_view
                        .update(cx, |this, _| this.minimize_to_tray())
                        .unwrap_or(false);
                    if enabled && quill::tray::tray_available() {
                        use objc2_app_kit::NSView;
                        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                        if let Ok(handle) = HasWindowHandle::window_handle(window)
                            && let RawWindowHandle::AppKit(handle) = handle.as_raw()
                        {
                            // SAFETY: GPUI owns this live NSView for the lifetime of the window.
                            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
                            if let Some(native) = view.window()
                                && native.isMiniaturized()
                            {
                                // Check again soon; a minimized window
                                // has no event to announce its state.
                                wait = std::time::Duration::from_secs(1);
                                // Keep it minimized while hidden. Deminiaturizing
                                // here would finish its animation by unhiding the app.
                                cx.hide();
                            }
                        }
                    }
                });
                for action in quill::tray::take_tray_actions() {
                    let _ = tray_window.update(cx, |_, window, cx| match action {
                        quill::tray::TrayAction::Open => raise_main_window(window, cx),
                        quill::tray::TrayAction::Lock => {
                            let _ =
                                tray_view.update(cx, |this, cx| this.lock_from_tray(window, cx));
                        }
                        quill::tray::TrayAction::ToggleNotifications => {
                            let _ = tray_view.update(cx, |this, cx| {
                                let on = this.desktop_notifications_enabled();
                                this.set_desktop_notifications(!on, cx);
                            });
                        }
                        quill::tray::TrayAction::ToggleSounds => {
                            let _ = tray_view.update(cx, |this, cx| {
                                let on = this.notification_sounds_enabled();
                                this.set_inapp_sounds_enabled(!on, cx);
                            });
                        }
                        quill::tray::TrayAction::Quit => cx.quit(),
                    });
                }
            }
        }
    })
    .detach();
    view.update(cx, |this, _| quill::tray::sync_tray_startup(this.session()));
}

/// The GPUI application on the OS platform wrapped in the interface-scale
/// decorator (`ui::interface_zoom`), so every window draws at
/// `interface_scale_pct` from its first frame.
#[cfg(feature = "ui")]
fn quill_application(interface_scale_pct: u16) -> gpui_kit::Application {
    use ui::interface_zoom::{ZoomPlatform, set_initial_zoom, zoom_for_percent};
    set_initial_zoom(zoom_for_percent(interface_scale_pct));
    let platform = gpui_kit::platform::current_platform(false);
    // The kit's theme init lists the installed fonts; started now, the list
    // is ready by then instead of being built on the main thread.
    gpui_kit::component::theme::prefetch_installed_font_names(
        std::sync::Arc::new(gpui_kit::TextSystem::new(platform.text_system())),
        &platform.background_executor(),
    );
    gpui_kit::Application::with_platform(std::rc::Rc::new(ZoomPlatform::new(platform)))
}

#[cfg(feature = "ui")]
fn demo_kinds() -> String {
    ui::ScreenshotDemo::all()
        .iter()
        .map(|demo| demo.kind())
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(feature = "ui")]
#[expect(clippy::exit, reason = "CLI usage error for an unknown demo kind")]
fn parse_screenshot_demo(args: &[String]) -> Option<(ui::ScreenshotDemo, std::path::PathBuf)> {
    use std::path::PathBuf;

    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--screenshot-demo" {
            let kind = iter.next().map(String::as_str)?;
            let out = iter
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("docs/screenshots"));
            let Some(demo) = ui::ScreenshotDemo::from_kind(kind) else {
                eprintln!(
                    "unknown screenshot demo '{kind}' (expected {})",
                    demo_kinds()
                );
                std::process::exit(2);
            };
            return Some((demo, out));
        }
    }
    None
}

/// Open a real GPUI window in the requested demo state, linger so an external
/// capture (ffmpeg x11grab) can snap docs/screenshots/*.png, then quit.
#[cfg(feature = "ui")]
fn run_screenshot_demo(demo: (ui::ScreenshotDemo, std::path::PathBuf)) {
    use gpui_kit::*;
    use std::time::Duration;
    let (kind, out_dir) = demo;
    let _ = std::fs::create_dir_all(&out_dir);
    // Fixtures must be deterministic and must never touch the user's real
    // settings: give this process a throwaway data root.
    let demo_root = std::env::temp_dir().join(format!("quill-demo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&demo_root);
    quill::settings::use_isolated_app_root(demo_root.clone());
    // `QUILL_DEMO_THEME=dark` captures any fixture in the dark theme (the
    // default follows the system appearance).
    if std::env::var("QUILL_DEMO_THEME").as_deref() == Ok("dark") {
        let paths =
            quill::settings::AccountPaths::for_root(&demo_root, &quill::ids::AccountKey::primary());
        let _ = std::fs::create_dir_all(&paths.root);
        let _ = std::fs::write(
            paths.root.join("appearance_prefs.json"),
            br#"{"theme":"dark","auto_night":"off"}"#,
        );
    }
    let marker = out_dir.join(format!(".quill-ready-{}", kind.kind()));
    let _ = std::fs::remove_file(&marker);
    let marker_for_spawn = marker.clone();

    eprintln!("quill screenshot-demo: {kind:?} → {}", out_dir.display());

    // Demo-only window size override (`QUILL_DEMO_WINDOW_SIZE=1200x1100`)
    // for slices whose fixture needs more vertical room than the default
    // 1200x740 (a fixture with many rows). Unset = unchanged, so
    // existing captures are unaffected.
    let (demo_w, demo_h) = std::env::var("QUILL_DEMO_WINDOW_SIZE")
        .ok()
        .and_then(|value| {
            let (w, h) = value.split_once('x')?;
            let w: f32 = w.parse().ok()?;
            let h: f32 = h.parse().ok()?;
            (w > 0.0 && h > 0.0).then_some((w, h))
        })
        .unwrap_or((1200.0, 740.0));
    // `QUILL_DEMO_WINDOW_ORIGIN=x,y` moves the demo window (points), e.g. to
    // record it somewhere other windows don't open.
    let (demo_x, demo_y) = std::env::var("QUILL_DEMO_WINDOW_ORIGIN")
        .ok()
        .and_then(|value| {
            let (x, y) = value.split_once(',')?;
            Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))
        })
        .unwrap_or((20.0, 20.0));

    quill_application(ui::interface_zoom::demo_interface_scale().unwrap_or(100))
        .with_assets(QuillAssets)
        .run(move |cx| {
            cx.set_app_identity("org.shinycake.quill", "Quill");
            #[cfg(windows)]
            quill::notify::register_toast_icon("org.shinycake.quill");
            gpui_kit::init(cx);
            // kit Phase 8: the kit defaults to its light theme on init;
            // Quill boots dark (kit dialogs match the app from here on).
            ui::set_theme_mode(ui::startup_theme_mode(), None, cx);
            // stories-high-contrast: screenshot demos can opt into the
            // high-contrast palette with `QUILL_DEMO_THEME=high-contrast`.
            ui::set_high_contrast(
                std::env::var("QUILL_DEMO_THEME").as_deref() == Ok("high-contrast"),
            );
            // kit Phase 9: honor the OS reduce-motion preference.
            ui::reduce_motion_probe::apply_at_startup(cx);
            ui::bind_keys(cx);
            ui::setup_app_menus(cx);
            cx.spawn(async move |cx| {
                let demo_window = cx
                    .open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(Bounds {
                                origin: point(px(demo_x), px(demo_y)),
                                size: size(px(demo_w), px(demo_h)),
                            })),
                            app_id: Some("org.shinycake.quill".into()),
                            ..ui::window_frame::window_options(kind.window_title())
                        },
                        move |window, cx| {
                            let view = cx.new(|cx| {
                                ui::QuillApp::new_with_demo(window, cx, None, Some(kind))
                            });
                            if kind.installs_tray() {
                                install_main_window_tray(window, cx, &view);
                            }
                            if window.focused(cx).is_none() {
                                window.focus(&view.focus_handle(cx), cx);
                            }
                            // The shell adds Quill's dialog hit-test barrier; Root
                            // hosts the kit dialog and notification layers.
                            let shell = cx.new(|_cx| ui::QuillShell::new(view));
                            cx.new(|cx| gpui_kit::component::Root::new(shell, window, cx))
                        },
                    )
                    .unwrap_or_else(|error| ui::startup_failure::window_failed(&error));

                // `QUILL_DEMO_BACKGROUND=1`: leave the window behind the
                // frontmost app (inactive), to measure the inactive path.
                if std::env::var_os("QUILL_DEMO_BACKGROUND").is_none() {
                    let _ = demo_window.update(cx, |_, window, cx| {
                        cx.activate(true);
                        window.activate_window();
                    });
                }

                // Allow a couple of frames to paint, then signal the capture script.
                cx.background_executor()
                    .timer(Duration::from_millis(kind.ready_after_ms()))
                    .await;
                // `QUILL_DEMO_CLICK` (demo-capture only): scripted input
                // before the capture, to verify click paths.
                #[cfg(feature = "demo-capture")]
                ui::screenshot_demo_input::run_steps(AnyWindowHandle::from(demo_window), cx).await;
                #[cfg(feature = "demo-capture")]
                if let Some(path) = std::env::var_os("QUILL_DEMO_CAPTURE") {
                    let captured = demo_window
                        .update(cx, |_, window, _| window.render_to_image())
                        .map_err(|err| err.to_string())
                        .and_then(|image| image.map_err(|err| err.to_string()))
                        .and_then(|image| image.save(&path).map_err(|err| err.to_string()));
                    if let Err(err) = captured {
                        eprintln!("quill screenshot-demo: capture failed: {err}");
                    }
                    // `QUILL_DEMO_CAPTURE_WINDOWS=1`: also capture every other
                    // window (the voice chat window) next to the main shot,
                    // as `<path>.window<N>.png`.
                    if std::env::var_os("QUILL_DEMO_CAPTURE_WINDOWS").is_some() {
                        cx.background_executor()
                            .timer(Duration::from_millis(800))
                            .await;
                        let handles = cx.update(|cx| cx.windows());
                        let mut n = 0;
                        for handle in handles {
                            if handle.window_id() == AnyWindowHandle::from(demo_window).window_id()
                            {
                                continue;
                            }
                            n += 1;
                            let mut other = std::path::PathBuf::from(&path).into_os_string();
                            other.push(format!(".window{n}.png"));
                            let result = handle
                                .update(cx, |_, window, cx| {
                                    // A second window that is not on screen
                                    // gets no display-link frames, so its
                                    // last painted frame can predate state
                                    // changes: draw it now, then capture.
                                    window.draw(cx).clear(cx);
                                    window.render_to_image()
                                })
                                .map_err(|err| err.to_string())
                                .and_then(|image| image.map_err(|err| err.to_string()))
                                .and_then(|image| {
                                    image.save(&other).map_err(|err| err.to_string())
                                });
                            if let Err(err) = result {
                                eprintln!("quill screenshot-demo: window capture failed: {err}");
                            }
                        }
                    }
                }
                let _ = std::fs::write(&marker_for_spawn, b"ready\n");
                // `QUILL_DEMO_DEACTIVATE=1`: once ready, a second (blank)
                // window takes key status, so the demo window goes from
                // active to inactive, as when another app comes forward.
                if std::env::var_os("QUILL_DEMO_DEACTIVATE").is_some() {
                    cx.update(|cx| {
                        if let Ok(other) = cx.open_window(
                            WindowOptions {
                                window_bounds: Some(WindowBounds::Windowed(Bounds {
                                    origin: point(px(40.), px(40.)),
                                    size: size(px(200.), px(120.)),
                                })),
                                ..Default::default()
                            },
                            |_, cx| cx.new(|_| EmptyView),
                        ) {
                            let _ = other.update(cx, |_, window, _| window.activate_window());
                        }
                    });
                }
                // Performance fixture:
                // `QUILL_DEMO_AUTOSCROLL=<x>,<y>[,<dy>[,<steps>[,<stop>]]]`
                // scrolls whatever sits under that window point with
                // synthetic wheel events (~60/s, `dy` px each, turning
                // around every `steps`, stopping after `stop` events when
                // given), to profile scrolling and what stays after it.
                if let Some((x, y, step_dy, turn, stop)) =
                    std::env::var("QUILL_DEMO_AUTOSCROLL").ok().and_then(|v| {
                        let mut parts = v.split(',').map(|p| p.trim().parse::<f32>().ok());
                        let x = parts.next()??;
                        let y = parts.next()??;
                        let dy = parts.next().flatten().unwrap_or(24.);
                        let turn = parts.next().flatten().unwrap_or(120.).max(1.) as u32;
                        let stop = parts.next().flatten().map_or(u32::MAX, |stop| stop as u32);
                        Some((x, y, dy, turn, stop))
                    })
                {
                    cx.spawn(async move |cx| {
                        for step in 0_u32..stop {
                            cx.background_executor()
                                .timer(Duration::from_millis(16))
                                .await;
                            let dy = if (step / turn) % 2 == 0 {
                                -step_dy
                            } else {
                                step_dy
                            };
                            let scrolled = demo_window.update(cx, |_, window, cx| {
                                window.dispatch_event(
                                    PlatformInput::Scroll(ScrollEvent {
                                        position: point(px(x), px(y)),
                                        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                                        modifiers: Modifiers::default(),
                                        touch_phase: TouchPhase::Moved,
                                        source: gpui_kit::ScrollSource::Pointer,
                                    }),
                                    cx,
                                );
                            });
                            if scrolled.is_err() {
                                break;
                            }
                        }
                    })
                    .detach();
                }
                cx.background_executor()
                    .timer(Duration::from_millis(
                        std::env::var("QUILL_DEMO_LINGER_MS")
                            .ok()
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(kind.linger_ms())
                            .clamp(3500, 600000),
                    ))
                    .await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
}

#[cfg(test)]
#[cfg(feature = "ui")]
mod demo_kind_tests {
    use crate::{demo_kinds, parse_screenshot_demo, ui::ScreenshotDemo};

    #[test]
    fn every_demo_kind_parses() {
        for demo in ScreenshotDemo::all() {
            let args = vec![
                "quill".to_string(),
                "--screenshot-demo".to_string(),
                demo.kind().to_string(),
            ];
            let parsed = parse_screenshot_demo(&args).unwrap_or_else(|| panic!("{}", demo.kind()));
            assert_eq!(parsed.0, demo, "{}", demo.kind());
        }
    }

    #[test]
    fn demo_kinds_are_unique_sorted_and_listed() {
        let kinds: Vec<&str> = ScreenshotDemo::all().iter().map(|d| d.kind()).collect();
        let mut sorted = kinds.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(kinds, sorted);
        assert_eq!(demo_kinds().split('|').count(), kinds.len());
    }
}
