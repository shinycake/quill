// Release UI builds on Windows are GUI-subsystem so launching Quill does not
// open a console window; `attach_parent_console` restores CLI output
// (`--version`, ...) when started from a terminal.
#![cfg_attr(
    all(windows, feature = "ui", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(feature = "ui")]
mod ui;

// Phase 1 (kit adoption): embed only the icons the UI needs, composed
// with the kit's default set, instead of the full 1800-icon Lucide catalog.
// (AtSign: the chat-row @ mention badge; kit Phase 5 adds the composer
// icon buttons: attach, emoji/stickers, voice/video record, send; chat
// rows add pinned, read receipt and muted glyphs.)
#[cfg(feature = "ui")]
gpui_kit::assets::icon_assets!(
    QuillIcons,
    [
        X,
        Languages,
        TextSearch,
        RotateCcw,
        AtSign,
        Paperclip,
        FaceSlightlySmiling,
        Mic,
        Video,
        Send,
        Pin,
        CheckCheck,
        BellOff,
        ALargeSmall,
        Reply,
        Pencil,
        Trash,
        Phone,
        AudioLines,
        Lock,
        UserPlus,
        UserX,
        Image,
        Film,
        File,
        ChartBar,
        SquarePlay,
        Timer,
        Play,
        Pause,
        ArrowDown,
        FolderOpen,
        FileText,
        Camera,
        Users,
        Shield,
        Link,
        UserCheck,
        Megaphone,
        PenLine,
        ShieldCheck,
        MessagesSquare,
        Hand,
        LogOut,
        RotateCw,
        Forward,
        Download,
        MessageSquare,
        ChevronLeft,
        ChevronRight,
        Clock,
        CalendarClock,
        Star,
        Volume2,
        VolumeX,
        SkipBack,
        SkipForward,
        Repeat,
        Repeat1,
        Shuffle,
        ArrowDownUp,
        PictureInPicture2,
        Bookmark,
        Bot,
        Quote,
        Copy,
        CircleCheck,
        PinOff,
        CircleStop,
        EllipsisVertical,
        PanelRight,
        List,
        Archive,
        ArchiveRestore,
        Bell,
        Eraser,
        Ban,
        Flag,
        ListChecks,
        MessageSquareDot,
        Crop,
        Undo2,
        MicOff,
        PanelTop,
        VideoOff,
        ScreenShare,
        BadgeCheck,
        Sticker,
        Cake,
        Images,
        Share2,
        NotebookPen,
        Contact,
        Keyboard,
        KeyboardOff,
        Cat,
        Book,
        Banknote,
        Gamepad2,
        Lightbulb,
        Music,
        Luggage,
        Dumbbell,
        GraduationCap,
        Plane,
        MessageCircle,
        Crown,
        Flower,
        House,
        Drama,
        PartyPopper,
        TrendingUp,
        Briefcase,
        BellPlus,
        Tag,
        TagX,
        FaceSlightlySmilingPlus
    ]
);

#[cfg(feature = "ui")]
#[derive(Clone, Copy, Default)]
struct QuillAssets;

#[cfg(feature = "ui")]
impl gpui_kit::gpui::AssetSource for QuillAssets {
    fn load(&self, path: &str) -> gpui_kit::gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        // The default bundle errors on unknown paths; fall through to the
        // scoped icons instead of propagating.
        match gpui_kit::assets::Assets.load(path) {
            Ok(Some(data)) => Ok(Some(data)),
            _ => QuillIcons.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui_kit::gpui::Result<Vec<gpui_kit::gpui::SharedString>> {
        let mut out = gpui_kit::assets::Assets.list(path)?;
        out.extend(QuillIcons.list(path)?);
        Ok(out)
    }
}

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

fn main() {
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
    if args.get(1).is_some_and(|a| a == "--embedded-credentials") {
        // Release pipeline check; prints only whether a pair is embedded.
        let embedded = quill::credentials::has_embedded();
        println!("{}", if embedded { "embedded" } else { "none" });
        return;
    }
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
    if args.iter().skip(1).any(|a| a == "--connect-smoke") {
        std::process::exit(quill::connect_smoke::cli_exit_code());
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

    // Single instance (tdesktop `Core::Sandbox`): a second launch hands its
    // links to the running Quill and exits before touching the account
    // database or the media caches.
    if forwarded_to_running_instance(args) {
        return;
    }

    // Demo windows can run beside the live app without touching its private caches.
    quill::local_path::sweep_media_caches();

    // `parity:platform-deep-links`: a `t.me` / `tg:` launch argument is
    // stashed on the app and resolved via `getDeepLinkInfo` once auth is
    // Ready (see `ui::deep_links`).
    let pending_deep_link = quill::connect::detect_deep_link_arg(args);

    let credentials = quill::credentials::load();
    let appearance = ui::QuillApp::load_appearance();
    let start_in_tray =
        args.iter().any(|arg| arg == "--start-minimized") || appearance.start_in_tray;
    let application = quill_application(appearance.interface_scale_pct).with_assets(QuillAssets);
    // macOS delivers `tg:` / `t.me` URLs (Info.plist CFBundleURLTypes) here,
    // both on cold launch and to the running app; Linux/Windows pass them
    // as argv instead (handled above and via the single-instance socket).
    application.on_open_urls(|urls| {
        for url in urls {
            quill::deep_link_inbox::push_external_link(&url);
        }
    });
    application.run(move |cx| {
        cx.set_app_identity("org.shinycake.quill", "Quill");
        #[cfg(windows)]
        quill::notify::register_toast_icon("org.shinycake.quill");
        gpui_kit::init(cx);
        // kit Phase 8: the kit defaults to its light theme on init;
        // Quill boots dark (kit dialogs match the app from here on).
        ui::set_theme_mode(startup_theme_mode(), None, cx);
        // stories-high-contrast: screenshot demos can opt into the
        // high-contrast palette with `QUILL_DEMO_THEME=high-contrast`.
        ui::set_high_contrast(std::env::var("QUILL_DEMO_THEME").as_deref() == Ok("high-contrast"));
        // kit Phase 9: honor the OS reduce-motion preference.
        cx.set_reduce_motion(os_prefers_reduced_motion());
        ui::bind_keys(cx);
        // kit Phase 7: File / Edit / View / Window / Help — native on
        // macOS, kit `AppMenuBar` data on Linux/Windows.
        ui::setup_app_menus(cx);
        let window_bounds = restored_window_bounds(quill::settings::load_window_state(), cx);
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    app_id: Some("org.shinycake.quill".into()),
                    show: !start_in_tray,
                    focus: !start_in_tray,
                    ..quill_window_options("Quill")
                },
                move |window, cx| {
                    let view = cx.new(|cx| {
                        let mut app = ui::QuillApp::new(window, cx, credentials.clone());
                        app.pending_deep_link = pending_deep_link.clone();
                        app
                    });
                    install_main_window_tray(window, cx, &view);
                    install_link_inbox(window, cx, &view);
                    if window.focused(cx).is_none() {
                        window.focus(&view.focus_handle(cx), cx);
                    }
                    if start_in_tray && !quill::tray::tray_available() {
                        // No tray host must never leave the only window inaccessible.
                        cx.activate(true);
                        window.activate_window();
                    }
                    // The shell adds Quill's dialog hit-test barrier; Root
                    // hosts the kit dialog and notification layers.
                    let shell = cx.new(|_cx| ui::QuillShell::new(view));
                    cx.new(|cx| gpui_kit::component::Root::new(shell, window, cx))
                },
            )
            .expect("failed to open window");
        })
        .detach();
    });
}

/// Returns `true` when this launch was handed to an already-running
/// instance (or must not start because that instance is stuck). Falls back
/// to running normally if the socket cannot be set up.
#[cfg(feature = "ui")]
fn forwarded_to_running_instance(args: &[String]) -> bool {
    use quill::single_instance::{Acquired, Endpoint, acquire};
    let Some(root) = quill::settings::safe_app_root() else {
        return false;
    };
    let forwarded: Vec<String> = args
        .iter()
        .skip(1)
        .filter_map(|arg| quill::deep_link_inbox::sanitize_link(arg))
        .chain(
            args.iter()
                .any(|arg| arg == "--start-minimized")
                .then(|| "--start-minimized".to_string()),
        )
        .collect();
    match acquire(&Endpoint::for_root(&root), &forwarded, |args| {
        quill::deep_link_inbox::handle_forwarded_launch(&args)
    }) {
        Ok(Acquired::Forwarded) => true,
        Ok(Acquired::Primary) => false,
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
            eprintln!("quill: {error}; not starting a second instance.");
            true
        }
        Err(error) => {
            eprintln!("quill: single-instance check unavailable ({error}); continuing.");
            false
        }
    }
}

/// Drains links that arrived from the OS or a second launch: raises the
/// window and hands the next link to the deep-link pump (one flow at a time).
#[cfg(feature = "ui")]
fn install_link_inbox(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    view: &gpui_kit::Entity<ui::QuillApp>,
) {
    cx.spawn({
        let view = view.downgrade();
        let window_handle = window.window_handle();
        async move |cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let alive = view
                    .update(cx, |this, _| {
                        if this.pending_deep_link.is_none() {
                            this.pending_deep_link = quill::deep_link_inbox::pop_link();
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
                if quill::deep_link_inbox::take_activation() {
                    let _ = window_handle.update(cx, |_, window, cx| raise_main_window(window, cx));
                }
            }
        }
    })
    .detach();
}

/// Brings the main window to the front, restoring it from the tray, the
/// Dock (minimized) or a hidden app.
#[cfg(feature = "ui")]
fn raise_main_window(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSView;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::AppKit(handle) = handle.as_raw()
        {
            // GPUI retains this native view while the window lives.
            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
            if let Some(native) = view.window() {
                native.deminiaturize(None);
            }
        }
    }
    cx.activate(true);
    window.activate_window();
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
    #[cfg(target_os = "macos")]
    window.on_window_should_close(cx, |_, cx| {
        if quill::tray::tray_available() {
            cx.hide();
            false
        } else {
            true
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
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
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
                            // GPUI owns this live NSView for the lifetime of the window.
                            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
                            if let Some(native) = view.window()
                                && native.isMiniaturized()
                            {
                                // Keep it minimized while hidden. Deminiaturizing
                                // here would finish its animation by unhiding the app.
                                cx.hide();
                            }
                        }
                    }
                });
                for action in quill::tray::take_tray_actions() {
                    let _ = tray_window.update(cx, |_, window, cx| match action {
                        quill::tray::TrayAction::Open => {
                            #[cfg(target_os = "macos")]
                            {
                                use objc2_app_kit::NSView;
                                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                                if let Ok(handle) = HasWindowHandle::window_handle(window)
                                    && let RawWindowHandle::AppKit(handle) = handle.as_raw()
                                {
                                    // GPUI retains this native view while the window lives.
                                    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
                                    if let Some(native) = view.window() {
                                        native.deminiaturize(None);
                                    }
                                }
                            }
                            cx.activate(true);
                            window.activate_window();
                        }
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
    gpui_kit::Application::with_platform(std::rc::Rc::new(ZoomPlatform::new(platform)))
}

/// kit Phase 7: window options compatible with kit's `TitleBar` — the title
/// bar owns dragging (double-click zoom included), so the platform must not
/// also treat it as a system move region.
#[cfg(feature = "ui")]
fn quill_window_options(title: &str) -> gpui_kit::WindowOptions {
    use gpui_kit::component::TitleBar;
    use gpui_kit::{TitlebarOptions, WindowOptions};
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..TitleBar::title_bar_options()
        }),
        app_owns_titlebar_drag: true,
        ..Default::default()
    }
}

/// Single source of truth for `--screenshot-demo` kinds: parsing and the
/// "unknown demo" error both derive from this table. One line per kind,
/// kept sorted, so parallel PRs add lines in different places.
#[cfg(feature = "ui")]
const DEMO_TABLE: &[(&str, ui::ScreenshotDemo)] = {
    use ui::ScreenshotDemo::*;
    &[
        ("connection-closed", ConnectionClosed),
        ("need-tdjson", NeedTdjson),
        ("ready-2fa-forgot", Ready2faForgot),
        ("ready-2fa-manage", Ready2faManage),
        ("ready-2fa-reset", Ready2faReset),
        ("ready-account", ReadyAccountLifecycle),
        ("ready-accounts", ReadyAccounts),
        ("ready-admin-log", ReadyAdminLog),
        ("ready-admin-management", ReadyAdminManagement),
        ("ready-albums", ReadyAlbums),
        ("ready-animated-emoji", ReadyAnimatedEmoji),
        ("ready-appearance", ReadyAppearance),
        ("ready-appearance-wallpapers", ReadyAppearanceWallpapers),
        ("ready-archive-bar", ReadyArchiveBar),
        ("ready-archive-menu", ReadyArchiveMenu),
        ("ready-archive-row", ReadyArchiveRow),
        ("ready-audio", ReadyAudio),
        ("ready-auto-delete", ReadyAutoDelete),
        ("ready-avatar-profile", ReadyAvatarProfile),
        ("ready-background-link", ReadyBackgroundLink),
        ("ready-block-user", ReadyBlockUser),
        ("ready-blockquote-expandable", ReadyBlockquoteExpandable),
        ("ready-bot-chat", ReadyBotChat),
        ("ready-bot-command-menu", ReadyBotCommandMenu),
        ("ready-bot-keyboard", ReadyBotKeyboard),
        ("ready-bot-profile", ReadyBotProfile),
        ("ready-bot-topics", ReadyBotTopics),
        ("ready-bot-topics-bottom", ReadyBotTopicsBottom),
        ("ready-bot-topics-left", ReadyBotTopicsLeft),
        ("ready-bubble-headers", ReadyBubbleHeaders),
        ("ready-call", ReadyCall),
        ("ready-call-devices", ReadyCallDevices),
        ("ready-call-reconnecting", ReadyCallReconnecting),
        ("ready-call-screenshare", ReadyCallScreenShare),
        (
            "ready-call-screenshare-receive",
            ReadyCallScreenShareReceive,
        ),
        ("ready-call-swap", ReadyCallSwap),
        ("ready-call-video", ReadyCallVideo),
        ("ready-calls-settings", ReadyCallsSettings),
        ("ready-caption-position", ReadyCaptionPosition),
        ("ready-channel-stats", ReadyChannelStats),
        ("ready-channels", ReadyChannels),
        ("ready-channels-admin", ReadyChannelsAdmin),
        ("ready-chat-avatars", ReadyChatAvatars),
        ("ready-chat-list", ReadyChatListMenu),
        ("ready-chat-list-2", ReadyChatList),
        ("ready-chat-list-3", ReadyChatList3),
        ("ready-chat-list-archive", ReadyChatListArchive),
        ("ready-chat-list-search", ReadyChatListSearch),
        ("ready-chat-look", ReadyChatLook),
        ("ready-chat-preview", ReadyChatPreview),
        ("ready-chat-rows", ReadyChatRows),
        ("ready-chat-theme", ReadyChatTheme),
        ("ready-chat-ttl", ReadyChatTtl),
        ("ready-chats", ReadyChats),
        ("ready-chats-composer", ReadyChatsComposer),
        ("ready-community-create", ReadyCommunityCreate),
        ("ready-community-hub", ReadyCommunityHub),
        ("ready-community-info", ReadyCommunityInfo),
        ("ready-composer-preview", ReadyComposerPreview),
        ("ready-contacts", ReadyContacts),
        ("ready-contacts-manage", ReadyContactsManage),
        ("ready-custom-emoji", ReadyCustomEmoji),
        ("ready-deep-link-info", ReadyDeepLinkInfo),
        ("ready-deep-link-invite", ReadyDeepLinkInvite),
        ("ready-deep-link-share", ReadyDeepLinkShare),
        ("ready-dice", ReadyDice),
        ("ready-downloads", ReadyDownloads),
        ("ready-drafts", ReadyDrafts),
        ("ready-edit-delete", ReadyEditDelete),
        ("ready-edit-media", ReadyEditMedia),
        ("ready-emoji-packs", ReadyEmojiPacks),
        ("ready-emoji-panel", ReadyEmojiPanel),
        ("ready-file-open-confirm", ReadyFileOpenConfirm),
        ("ready-folder-badges", ReadyFolderBadges),
        ("ready-folders", ReadyFolders),
        ("ready-folders-add-link", ReadyFoldersAddLink),
        ("ready-folders-delete", ReadyFoldersDelete),
        ("ready-folders-icons", ReadyFoldersIcons),
        ("ready-folders-limit", ReadyFoldersLimit),
        ("ready-folders-manage", ReadyFoldersManage),
        ("ready-folders-menu", ReadyFoldersMenu),
        ("ready-folders-new-chats", ReadyFoldersNewChats),
        ("ready-folders-new-chats-join", ReadyFoldersNewChatsJoin),
        ("ready-folders-share", ReadyFoldersShare),
        ("ready-folders-sidebar", ReadyFoldersSidebar),
        ("ready-folders-tag-color", ReadyFoldersTagColor),
        ("ready-folders-tags", ReadyFoldersTags),
        ("ready-forum-topics", ReadyForumTopics),
        ("ready-forums-saved", ReadyForumsSaved),
        ("ready-forward", ReadyForward),
        ("ready-forward-bar", ReadyForwardBar),
        ("ready-game-card", ReadyGameCard),
        ("ready-gif-playback", ReadyGifPlayback),
        ("ready-gifs", ReadyGifs),
        ("ready-gift-cards", ReadyGiftCards),
        ("ready-gifts", ReadyGifts),
        ("ready-group-admin-settings", ReadyGroupAdminSettings),
        ("ready-group-call", ReadyGroupCall),
        ("ready-group-call-invitation", ReadyGroupCallInvitation),
        ("ready-group-call-invite", ReadyGroupCallInvite),
        ("ready-group-call-join-as", ReadyGroupCallJoinAs),
        ("ready-group-call-manage", ReadyGroupCallManage),
        ("ready-group-call-polish", ReadyGroupCallPolish),
        ("ready-group-call-scheduled", ReadyGroupCallScheduled),
        ("ready-group-info-edit", ReadyGroupInfoEdit),
        ("ready-group-manage", ReadyGroupManage),
        ("ready-groups2", ReadyGroups2),
        ("ready-inline-results", ReadyInlineResults),
        ("ready-invite-links", ReadyInviteLinks),
        ("ready-join-bar", ReadyJoinBar),
        ("ready-jump-date", ReadyJumpDate),
        ("ready-key-verification", ReadyKeyVerification),
        ("ready-keybindings", ReadyKeybindings),
        ("ready-link-preview", ReadyLinkPreview),
        ("ready-local-storage", ReadyLocalStorage),
        ("ready-location", ReadyLocation),
        ("ready-lock-screen", ReadyLockScreen),
        ("ready-login-email", ReadyLoginEmail),
        ("ready-login-prevented", ReadyLoginPrevented),
        ("ready-marketplace-gift", ReadyMarketplaceGift),
        ("ready-media", ReadyMedia),
        ("ready-media-viewer", ReadyMediaViewer),
        ("ready-member-moderation", ReadyMemberModeration),
        ("ready-mentions", ReadyMentions),
        ("ready-message-menu", ReadyMessageMenu),
        ("ready-multiline-rows", ReadyMultilineRows),
        ("ready-mute-archive", ReadyMuteArchive),
        ("ready-mute-custom", ReadyMuteCustom),
        ("ready-new-login", ReadyNewLogin),
        ("ready-notification-sound", ReadyNotificationSound),
        ("ready-notify-os", ReadyNotifyOs),
        ("ready-offline", ReadyOffline),
        ("ready-offline-toast", ReadyOfflineToast),
        ("ready-passcode-create", ReadyPasscodeCreate),
        ("ready-passcode-settings", ReadyPasscodeSettings),
        ("ready-paste-image", ReadyPasteImage),
        ("ready-payments", ReadyPayments),
        ("ready-pin", ReadyPin),
        ("ready-pin-drag", ReadyPinDrag),
        ("ready-player-bar", ReadyPlayerBar),
        ("ready-poll", ReadyPoll),
        ("ready-premium", ReadyPremium),
        ("ready-preview-cards", ReadyPreviewCards),
        ("ready-privacy", ReadyPrivacy),
        ("ready-privacy-gifts", ReadyPrivacyGifts),
        ("ready-profile-edit", ReadyProfileEdit),
        ("ready-profile-panels", ReadyProfilePanels),
        ("ready-proxy", ReadyProxy),
        ("ready-reactions", ReadyReactions),
        ("ready-reconnecting", ReadyReconnecting),
        ("ready-recovery-email", ReadyRecoveryEmail),
        ("ready-rendering-leftovers", ReadyRenderingLeftovers),
        ("ready-reply", ReadyReply),
        ("ready-reply-keyboard", ReadyReplyKeyboard),
        ("ready-reply-media", ReadyReplyMedia),
        ("ready-reveal", ReadyReveal),
        ("ready-rich-ai-tools", ReadyRichAiTools),
        ("ready-rich-editor", ReadyRichEditor),
        ("ready-rich-message", ReadyRichMessage),
        ("ready-rich-premium-gate", ReadyRichPremiumGate),
        ("ready-rtl-composer", ReadyRtlComposer),
        ("ready-rtl-polish", ReadyRtlPolish),
        ("ready-scheduled", ReadyScheduled),
        ("ready-search", ReadySearch),
        ("ready-search-filters", ReadySearchFilters),
        ("ready-search-frequent", ReadySearchFrequent),
        ("ready-search-from", ReadySearchFrom),
        ("ready-search-from-hits", ReadySearchFromHits),
        ("ready-search-in-chat", ReadySearchInChat),
        ("ready-search-previews", ReadySearchPreviews),
        ("ready-search-public", ReadySearchPublic),
        ("ready-secret-bot-alert", ReadySecretBotAlert),
        ("ready-secret-chat", ReadySecretChat),
        ("ready-secret-picker", ReadySecretPicker),
        ("ready-seek-bars", ReadySeekBars),
        ("ready-select-keyboard", ReadySelectKeyboard),
        ("ready-select-mode", ReadySelectMode),
        ("ready-self-destruct", ReadySelfDestruct),
        ("ready-send-as", ReadySendAs),
        ("ready-send-media", ReadySendMedia),
        ("ready-service-messages", ReadyServiceMessages),
        ("ready-service-notice", ReadyServiceNotice),
        ("ready-session-details", ReadySessionDetails),
        ("ready-session-toggles", ReadySessionToggles),
        ("ready-sessions", ReadySessions),
        ("ready-share-box", ReadyShareBox),
        ("ready-shared-media", ReadySharedMedia),
        ("ready-shortcuts", ReadyShortcuts),
        ("ready-showcase", ReadyShowcase),
        ("ready-slow-mode", ReadySlowMode),
        ("ready-spellcheck", ReadySpellcheck),
        ("ready-spellcheck-panel", ReadySpellcheckPanel),
        ("ready-spellcheck-toggle", ReadySpellcheckToggle),
        ("ready-sponsored", ReadySponsored),
        ("ready-stars", ReadyStars),
        ("ready-sticker-playback", ReadyStickerPlayback),
        ("ready-stickers", ReadyStickers),
        ("ready-storage-usage", ReadyStorageUsage),
        ("ready-stories", ReadyStories),
        ("ready-stories-collapsed", ReadyStoriesCollapsed),
        ("ready-stories-collapsing", ReadyStoriesCollapsing),
        ("ready-stories-expanded", ReadyStoriesExpanded),
        ("ready-story-albums", ReadyStoryAlbums),
        ("ready-story-areas", ReadyStoryAreas),
        ("ready-story-composer", ReadyStoryComposer),
        ("ready-story-edit", ReadyStoryEdit),
        ("ready-story-more", ReadyStoryMore),
        ("ready-story-post", ReadyStoryPost),
        ("ready-story-video", ReadyStoryVideo),
        ("ready-story-viewers", ReadyStoryViewers),
        ("ready-subscriptions", ReadySubscriptions),
        ("ready-suggest-emoji", ReadySuggestEmoji),
        ("ready-suggest-hashtag", ReadySuggestHashtag),
        ("ready-swipe-mute", ReadySwipeMute),
        ("ready-swipe-reached", ReadySwipeReached),
        ("ready-terms", ReadyTerms),
        ("ready-text-entities", ReadyTextEntities),
        ("ready-threads", ReadyThreads),
        ("ready-top-bars", ReadyTopBars),
        ("ready-topic-post", ReadyTopicPost),
        ("ready-translate", ReadyTranslate),
        ("ready-tray-behavior", ReadyTrayBehavior),
        ("ready-typing", ReadyTyping),
        ("ready-unread", ReadyUnread),
        ("ready-unread-read", ReadyUnreadRead),
        ("ready-unsupported-message", ReadyUnsupportedMessage),
        ("ready-update-changelog", ReadyUpdateChangelog),
        ("ready-update-failure", ReadyUpdateFailure),
        ("ready-update-install", ReadyUpdateInstall),
        ("ready-username", ReadyUsername),
        ("ready-video", ReadyVideo),
        ("ready-video-note", ReadyVideoNote),
        ("ready-video-note-send", ReadyVideoNoteSend),
        ("ready-video-pip", ReadyVideoPip),
        ("ready-video-playback", ReadyVideoPlayback),
        ("ready-video-send", ReadyVideoSend),
        ("ready-viewer-gif", ReadyViewerGif),
        ("ready-viewer-shared", ReadyViewerShared),
        ("ready-voice", ReadyVoice),
        ("ready-web-sessions", ReadyWebSessions),
        ("wait-code", WaitCode),
        ("wait-code-resend", WaitCodeResend),
        ("wait-password", WaitPassword),
        ("wait-phone", WaitPhone),
        ("wait-phone-banned", WaitPhoneBanned),
        ("wait-phone-country", WaitPhoneCountry),
        ("wait-phone-formatted", WaitPhoneFormatted),
        ("wait-premium", WaitPremium),
        ("wait-qr", WaitQr),
    ]
};

#[cfg(feature = "ui")]
fn demo_kinds() -> String {
    DEMO_TABLE
        .iter()
        .map(|(k, _)| *k)
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(feature = "ui")]
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
            let demo = match DEMO_TABLE.iter().find(|(k, _)| *k == kind) {
                Some((_, demo)) => *demo,
                None => {
                    eprintln!(
                        "unknown screenshot demo '{kind}' (expected {})",
                        demo_kinds()
                    );
                    std::process::exit(2);
                }
            };
            return Some((demo, out));
        }
    }
    None
}

/// kit Phase 8: the app boots dark; screenshot demos can opt into the light
/// theme with `QUILL_DEMO_THEME=light` so both themes get captured.
#[cfg(feature = "ui")]
fn startup_theme_mode() -> gpui_kit::component::ThemeMode {
    if std::env::var("QUILL_DEMO_THEME").as_deref() == Ok("light") {
        gpui_kit::component::ThemeMode::Light
    } else {
        gpui_kit::component::ThemeMode::Dark
    }
}

/// kit Phase 9: respect the OS "reduce motion" accessibility preference.
/// GPUI's animation/spring machinery settles instantly when
/// `App::set_reduce_motion(true)` is called, so kit `Skeleton` shimmer,
/// switch springs and spinners all go static with this one call.
/// `QUILL_REDUCED_MOTION=1` overrides for testing.
#[cfg(feature = "ui")]
fn os_prefers_reduced_motion() -> bool {
    if let Ok(v) = std::env::var("QUILL_REDUCED_MOTION") {
        return v == "1" || v.eq_ignore_ascii_case("true");
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("defaults")
            .args(["read", "com.apple.universalaccess", "reduceMotion"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
    }
    #[cfg(target_os = "linux")]
    {
        // GNOME exposes the preference as interface animations off.
        std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "enable-animations"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "false")
    }
    #[cfg(windows)]
    {
        // Settings > Accessibility > Visual effects > Animation effects.
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
        };
        let mut animations: i32 = 1;
        // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer.
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut animations as *mut i32).cast(),
                0,
            )
        };
        ok != 0 && animations == 0
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        false
    }
}

/// Open a real GPUI window in the requested demo state, linger so an external
/// capture (ffmpeg x11grab) can snap docs/screenshots/*.png, then quit.
#[cfg(feature = "ui")]
fn run_screenshot_demo(demo: (ui::ScreenshotDemo, std::path::PathBuf)) {
    use gpui_kit::*;
    use std::time::Duration;
    use ui::ScreenshotDemo;

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
    let marker = out_dir.join(match kind {
        ScreenshotDemo::NeedTdjson => ".quill-ready-need-tdjson",
        ScreenshotDemo::WaitPhone => ".quill-ready-wait-phone",
        ScreenshotDemo::WaitCode => ".quill-ready-wait-code",
        ScreenshotDemo::WaitPhoneCountry => ".quill-ready-wait-phone-country",
        ScreenshotDemo::WaitPhoneFormatted => ".quill-ready-wait-phone-formatted",
        ScreenshotDemo::WaitCodeResend => ".quill-ready-wait-code-resend",
        ScreenshotDemo::WaitPhoneBanned => ".quill-ready-wait-phone-banned",
        ScreenshotDemo::WaitPassword => ".quill-ready-wait-password",
        ScreenshotDemo::WaitPremium => ".quill-ready-wait-premium",
        ScreenshotDemo::WaitQr => ".quill-ready-wait-qr",
        ScreenshotDemo::ConnectionClosed => ".quill-ready-connection-closed",
        ScreenshotDemo::ReadyDeepLinkInfo => ".quill-ready-ready-deep-link-info",
        ScreenshotDemo::ReadyDeepLinkInvite => ".quill-ready-ready-deep-link-invite",
        ScreenshotDemo::ReadyDeepLinkShare => ".quill-ready-ready-deep-link-share",
        ScreenshotDemo::ReadyChats => ".quill-ready-ready-chats",
        ScreenshotDemo::ReadyTrayBehavior => ".quill-ready-ready-tray-behavior",
        ScreenshotDemo::ReadyUpdateInstall => ".quill-ready-ready-update-install",
        ScreenshotDemo::ReadyUpdateChangelog => ".quill-ready-ready-update-changelog",
        ScreenshotDemo::ReadyUpdateFailure => ".quill-ready-ready-update-failure",
        ScreenshotDemo::ReadyOffline => ".quill-ready-ready-offline",
        ScreenshotDemo::ReadyOfflineToast => ".quill-ready-ready-offline-toast",
        ScreenshotDemo::ReadyReconnecting => ".quill-ready-ready-reconnecting",
        ScreenshotDemo::ReadyChatsComposer => ".quill-ready-ready-chats-composer",
        ScreenshotDemo::ReadySuggestHashtag => ".quill-ready-ready-suggest-hashtag",
        ScreenshotDemo::ReadySuggestEmoji => ".quill-ready-ready-suggest-emoji",
        ScreenshotDemo::ReadyUnread => ".quill-ready-ready-unread",
        ScreenshotDemo::ReadyUnreadRead => ".quill-ready-ready-unread-read",
        ScreenshotDemo::ReadyMedia => ".quill-ready-ready-media",
        ScreenshotDemo::ReadyDownloads => ".quill-ready-ready-downloads",
        ScreenshotDemo::ReadySendMedia => ".quill-ready-ready-send-media",
        ScreenshotDemo::ReadyPasteImage => ".quill-ready-ready-paste-image",
        ScreenshotDemo::ReadySearch => ".quill-ready-ready-search",
        ScreenshotDemo::ReadySearchInChat => ".quill-ready-ready-search-in-chat",
        ScreenshotDemo::ReadyReply => ".quill-ready-ready-reply",
        ScreenshotDemo::ReadyEditDelete => ".quill-ready-ready-edit-delete",
        ScreenshotDemo::ReadyForward => ".quill-ready-ready-forward",
        ScreenshotDemo::ReadyShareBox => ".quill-ready-ready-share-box",
        ScreenshotDemo::ReadyForwardBar => ".quill-ready-ready-forward-bar",
        ScreenshotDemo::ReadySendAs => ".quill-ready-ready-send-as",
        ScreenshotDemo::ReadySelectMode => ".quill-ready-ready-select-mode",
        ScreenshotDemo::ReadySelectKeyboard => ".quill-ready-ready-select-keyboard",
        ScreenshotDemo::ReadyReplyMedia => ".quill-ready-ready-reply-media",
        ScreenshotDemo::ReadyEditMedia => ".quill-ready-ready-edit-media",
        ScreenshotDemo::ReadyReveal => ".quill-ready-ready-reveal",
        ScreenshotDemo::ReadyReactions => ".quill-ready-ready-reactions",
        ScreenshotDemo::ReadyPin => ".quill-ready-ready-pin",
        ScreenshotDemo::ReadyMuteArchive => ".quill-ready-ready-mute-archive",
        ScreenshotDemo::ReadyChatListMenu => ".quill-ready-ready-chat-list",
        ScreenshotDemo::ReadyChatList => ".quill-ready-ready-chat-list-2",
        ScreenshotDemo::ReadyChatList3 => ".quill-ready-ready-chat-list-3",
        ScreenshotDemo::ReadyChatListArchive => ".quill-ready-ready-chat-list-archive",
        ScreenshotDemo::ReadyChatListSearch => ".quill-ready-ready-chat-list-search",
        ScreenshotDemo::ReadyArchiveRow => ".quill-ready-ready-archive-row",
        ScreenshotDemo::ReadyArchiveBar => ".quill-ready-ready-archive-bar",
        ScreenshotDemo::ReadyArchiveMenu => ".quill-ready-ready-archive-menu",
        ScreenshotDemo::ReadyPinDrag => ".quill-ready-ready-pin-drag",
        ScreenshotDemo::ReadySwipeMute => ".quill-ready-ready-swipe-mute",
        ScreenshotDemo::ReadySwipeReached => ".quill-ready-ready-swipe-reached",
        ScreenshotDemo::ReadyStoriesExpanded => ".quill-ready-ready-stories-expanded",
        ScreenshotDemo::ReadyStoriesCollapsing => ".quill-ready-ready-stories-collapsing",
        ScreenshotDemo::ReadyStoriesCollapsed => ".quill-ready-ready-stories-collapsed",
        ScreenshotDemo::ReadySharedMedia => ".quill-ready-ready-shared-media",
        ScreenshotDemo::ReadyTyping => ".quill-ready-ready-typing",
        ScreenshotDemo::ReadyChatRows => ".quill-ready-ready-chat-rows",
        ScreenshotDemo::ReadyJoinBar => ".quill-ready-ready-join-bar",
        ScreenshotDemo::ReadyTopBars => ".quill-ready-ready-top-bars",
        ScreenshotDemo::ReadySearchPreviews => ".quill-ready-ready-search-previews",
        ScreenshotDemo::ReadyMultilineRows => ".quill-ready-ready-multiline-rows",
        ScreenshotDemo::ReadyStickers => ".quill-ready-ready-stickers",
        ScreenshotDemo::ReadyStickerPlayback => ".quill-ready-ready-sticker-playback",
        ScreenshotDemo::ReadyVoice => ".quill-ready-ready-voice",
        ScreenshotDemo::ReadyGameCard => ".quill-ready-ready-game-card",
        ScreenshotDemo::ReadyLinkPreview => ".quill-ready-ready-link-preview",
        ScreenshotDemo::ReadyComposerPreview => ".quill-ready-ready-composer-preview",
        ScreenshotDemo::ReadyPreviewCards => ".quill-ready-ready-preview-cards",
        ScreenshotDemo::ReadyCaptionPosition => ".quill-ready-ready-caption-position",
        ScreenshotDemo::ReadyGifs => ".quill-ready-ready-gifs",
        ScreenshotDemo::ReadyGifPlayback => ".quill-ready-ready-gif-playback",
        ScreenshotDemo::ReadyVideo => ".quill-ready-ready-video",
        ScreenshotDemo::ReadyVideoNote => ".quill-ready-ready-video-note",
        ScreenshotDemo::ReadyAudio => ".quill-ready-ready-audio",
        ScreenshotDemo::ReadyPlayerBar => ".quill-ready-ready-player-bar",
        ScreenshotDemo::ReadyVideoSend => ".quill-ready-ready-video-send",
        ScreenshotDemo::ReadyVideoNoteSend => ".quill-ready-ready-video-note-send",
        ScreenshotDemo::ReadyDrafts => ".quill-ready-ready-drafts",
        ScreenshotDemo::ReadyAlbums => ".quill-ready-ready-albums",
        ScreenshotDemo::ReadyMentions => ".quill-ready-ready-mentions",
        ScreenshotDemo::ReadyEmojiPanel => ".quill-ready-ready-emoji-panel",
        ScreenshotDemo::ReadySponsored => ".quill-ready-ready-sponsored",
        ScreenshotDemo::ReadyCustomEmoji => ".quill-ready-ready-custom-emoji",
        ScreenshotDemo::ReadyAnimatedEmoji => ".quill-ready-ready-animated-emoji",
        ScreenshotDemo::ReadyEmojiPacks => ".quill-ready-ready-emoji-packs",
        ScreenshotDemo::ReadyChannels => ".quill-ready-ready-channels",
        ScreenshotDemo::ReadyChannelsAdmin => ".quill-ready-ready-channels-admin",
        ScreenshotDemo::ReadyChannelStats => ".quill-ready-ready-channel-stats",
        ScreenshotDemo::ReadyInviteLinks => ".quill-ready-ready-invite-links",
        ScreenshotDemo::ReadyAdminManagement => ".quill-ready-ready-admin-management",
        ScreenshotDemo::ReadyAdminLog => ".quill-ready-ready-admin-log",
        ScreenshotDemo::ReadyGroups2 => ".quill-ready-ready-groups2",
        ScreenshotDemo::ReadyGroupManage => ".quill-ready-ready-group-manage",
        ScreenshotDemo::ReadyGroupInfoEdit => ".quill-ready-ready-group-info-edit",
        ScreenshotDemo::ReadyCommunityCreate => ".quill-ready-ready-community-create",
        ScreenshotDemo::ReadyCommunityHub => ".quill-ready-ready-community-hub",
        ScreenshotDemo::ReadyCommunityInfo => ".quill-ready-ready-community-info",
        ScreenshotDemo::ReadyBotChat => ".quill-ready-ready-bot-chat",
        ScreenshotDemo::ReadyBotKeyboard => ".quill-ready-ready-bot-keyboard",
        ScreenshotDemo::ReadyBotCommandMenu => ".quill-ready-ready-bot-command-menu",
        ScreenshotDemo::ReadyInlineResults => ".quill-ready-ready-inline-results",
        ScreenshotDemo::ReadyBotProfile => ".quill-ready-ready-bot-profile",
        ScreenshotDemo::ReadyTextEntities => ".quill-ready-ready-text-entities",
        ScreenshotDemo::ReadyUnsupportedMessage => ".quill-ready-ready-unsupported-message",
        ScreenshotDemo::ReadyBlockquoteExpandable => ".quill-ready-ready-blockquote-expandable",
        ScreenshotDemo::ReadyAvatarProfile => ".quill-ready-ready-avatar-profile",
        ScreenshotDemo::ReadyRtlComposer => ".quill-ready-ready-rtl-composer",
        ScreenshotDemo::ReadyRtlPolish => ".quill-ready-ready-rtl-polish",
        ScreenshotDemo::ReadyServiceMessages => ".quill-ready-ready-service-messages",
        ScreenshotDemo::ReadyThreads => ".quill-ready-ready-threads",
        ScreenshotDemo::ReadyForumsSaved => ".quill-ready-ready-forums-saved",
        ScreenshotDemo::ReadyBubbleHeaders => ".quill-ready-ready-bubble-headers",
        ScreenshotDemo::ReadyRenderingLeftovers => ".quill-ready-ready-rendering-leftovers",
        ScreenshotDemo::ReadyTranslate => ".quill-ready-ready-translate",
        ScreenshotDemo::ReadyReplyKeyboard => ".quill-ready-ready-reply-keyboard",
        ScreenshotDemo::ReadyShowcase => ".quill-ready-ready-showcase",
        ScreenshotDemo::ReadyMessageMenu => ".quill-ready-ready-message-menu",
        ScreenshotDemo::ReadyPoll => ".quill-ready-ready-poll",
        ScreenshotDemo::ReadyLocation => ".quill-ready-ready-location",
        ScreenshotDemo::ReadyDice => ".quill-ready-ready-dice",
        ScreenshotDemo::ReadyMediaViewer => ".quill-ready-ready-media-viewer",
        ScreenshotDemo::ReadyVideoPlayback => ".quill-ready-ready-video-playback",
        ScreenshotDemo::ReadyViewerGif => ".quill-ready-ready-viewer-gif",
        ScreenshotDemo::ReadyViewerShared => ".quill-ready-ready-viewer-shared",
        ScreenshotDemo::ReadyVideoPip => ".quill-ready-ready-video-pip",
        ScreenshotDemo::ReadyStories => ".quill-ready-ready-stories",
        ScreenshotDemo::ReadyStoryPost => ".quill-ready-ready-story-post",
        ScreenshotDemo::ReadyStoryComposer => ".quill-ready-ready-story-composer",
        ScreenshotDemo::ReadyStoryViewers => ".quill-ready-ready-story-viewers",
        ScreenshotDemo::ReadyStoryEdit => ".quill-ready-ready-story-edit",
        ScreenshotDemo::ReadyStoryAlbums => ".quill-ready-ready-story-albums",
        ScreenshotDemo::ReadyStoryAreas => ".quill-ready-ready-story-areas",
        ScreenshotDemo::ReadyStoryVideo => ".quill-ready-ready-story-video",
        ScreenshotDemo::ReadyStoryMore => ".quill-ready-ready-story-more",
        ScreenshotDemo::ReadySeekBars => ".quill-ready-ready-seek-bars",
        ScreenshotDemo::ReadyForumTopics => ".quill-ready-ready-forum-topics",
        ScreenshotDemo::ReadyTopicPost => ".quill-ready-ready-topic-post",
        ScreenshotDemo::ReadyBotTopics => ".quill-ready-ready-bot-topics",
        ScreenshotDemo::ReadyBotTopicsBottom => ".quill-ready-ready-bot-topics-bottom",
        ScreenshotDemo::ReadyBotTopicsLeft => ".quill-ready-ready-bot-topics-left",
        ScreenshotDemo::ReadyContacts => ".quill-ready-ready-contacts",
        ScreenshotDemo::ReadyContactsManage => ".quill-ready-ready-contacts-manage",
        ScreenshotDemo::ReadyBlockUser => ".quill-ready-ready-block-user",
        ScreenshotDemo::ReadyFolders => ".quill-ready-ready-folders",
        ScreenshotDemo::ReadyFoldersManage => ".quill-ready-ready-folders-manage",
        ScreenshotDemo::ReadyFoldersShare => ".quill-ready-ready-folders-share",
        ScreenshotDemo::ReadyFoldersSidebar => ".quill-ready-ready-folders-sidebar",
        ScreenshotDemo::ReadyFoldersAddLink => ".quill-ready-ready-folders-add-link",
        ScreenshotDemo::ReadyFoldersIcons => ".quill-ready-ready-folders-icons",
        ScreenshotDemo::ReadyFoldersTags => ".quill-ready-ready-folders-tags",
        ScreenshotDemo::ReadyFoldersTagColor => ".quill-ready-ready-folders-tag-color",
        ScreenshotDemo::ReadyFoldersMenu => ".quill-ready-ready-folders-menu",
        ScreenshotDemo::ReadyFoldersNewChats => ".quill-ready-ready-folders-new-chats",
        ScreenshotDemo::ReadyFoldersNewChatsJoin => ".quill-ready-ready-folders-new-chats-join",
        ScreenshotDemo::ReadyFoldersLimit => ".quill-ready-ready-folders-limit",
        ScreenshotDemo::ReadyFoldersDelete => ".quill-ready-ready-folders-delete",
        ScreenshotDemo::ReadyChatAvatars => ".quill-ready-ready-chat-avatars",
        ScreenshotDemo::ReadyNotificationSound => ".quill-ready-ready-notification-sound",
        ScreenshotDemo::ReadyNotifyOs => ".quill-ready-ready-notify-os",
        ScreenshotDemo::ReadyFolderBadges => ".quill-ready-ready-folder-badges",
        ScreenshotDemo::ReadySlowMode => ".quill-ready-ready-slow-mode",
        ScreenshotDemo::ReadySecretChat => ".quill-ready-ready-secret-chat",
        ScreenshotDemo::ReadyPayments => ".quill-ready-ready-payments",
        ScreenshotDemo::ReadySubscriptions => ".quill-ready-ready-subscriptions",
        ScreenshotDemo::ReadyStars => ".quill-ready-ready-stars",
        ScreenshotDemo::ReadyGifts => ".quill-ready-ready-gifts",
        ScreenshotDemo::ReadyPremium => ".quill-ready-ready-premium",
        ScreenshotDemo::ReadyGiftCards => ".quill-ready-ready-gift-cards",
        ScreenshotDemo::ReadyMarketplaceGift => ".quill-ready-ready-marketplace-gift",
        ScreenshotDemo::ReadySecretPicker => ".quill-ready-ready-secret-picker",
        ScreenshotDemo::ReadySecretBotAlert => ".quill-ready-ready-secret-bot-alert",
        ScreenshotDemo::ReadyStorageUsage => ".quill-ready-ready-storage-usage",
        ScreenshotDemo::ReadyAppearance => ".quill-ready-ready-appearance",
        ScreenshotDemo::ReadyAppearanceWallpapers => ".quill-ready-ready-appearance-wallpapers",
        ScreenshotDemo::ReadyChatLook => ".quill-ready-ready-chat-look",
        ScreenshotDemo::ReadyChatTheme => ".quill-ready-ready-chat-theme",
        ScreenshotDemo::ReadyBackgroundLink => ".quill-ready-ready-background-link",
        ScreenshotDemo::ReadySpellcheck => ".quill-ready-ready-spellcheck",
        ScreenshotDemo::ReadySpellcheckPanel => ".quill-ready-ready-spellcheck-panel",
        ScreenshotDemo::ReadySpellcheckToggle => ".quill-ready-ready-spellcheck-toggle",
        ScreenshotDemo::ReadyKeybindings => ".quill-ready-ready-keybindings",
        ScreenshotDemo::ReadyAccounts => ".quill-ready-ready-accounts",
        ScreenshotDemo::Ready2faManage => ".quill-ready-ready-2fa-manage",
        ScreenshotDemo::ReadyRecoveryEmail => ".quill-ready-ready-recovery-email",
        ScreenshotDemo::ReadyNewLogin => ".quill-ready-ready-new-login",
        ScreenshotDemo::ReadyLoginPrevented => ".quill-ready-ready-login-prevented",
        ScreenshotDemo::ReadyServiceNotice => ".quill-ready-ready-service-notice",
        ScreenshotDemo::ReadyTerms => ".quill-ready-ready-terms",
        ScreenshotDemo::ReadyLocalStorage => ".quill-ready-ready-local-storage",
        ScreenshotDemo::Ready2faForgot => ".quill-ready-ready-2fa-forgot",
        ScreenshotDemo::Ready2faReset => ".quill-ready-ready-2fa-reset",
        ScreenshotDemo::ReadyLoginEmail => ".quill-ready-ready-login-email",
        ScreenshotDemo::ReadyAccountLifecycle => ".quill-ready-ready-account",
        ScreenshotDemo::ReadySessions => ".quill-ready-ready-sessions",
        ScreenshotDemo::ReadyWebSessions => ".quill-ready-ready-web-sessions",
        ScreenshotDemo::ReadySessionToggles => ".quill-ready-ready-session-toggles",
        ScreenshotDemo::ReadyKeyVerification => ".quill-ready-ready-key-verification",
        ScreenshotDemo::ReadySelfDestruct => ".quill-ready-ready-self-destruct",
        ScreenshotDemo::ReadyCall => ".quill-ready-ready-call",
        ScreenshotDemo::ReadyCallSwap => ".quill-ready-ready-call-swap",
        ScreenshotDemo::ReadyCallVideo => ".quill-ready-ready-call-video",
        ScreenshotDemo::ReadyCallScreenShare => ".quill-ready-ready-call-screenshare",
        ScreenshotDemo::ReadyCallScreenShareReceive => {
            ".quill-ready-ready-call-screenshare-receive"
        }
        ScreenshotDemo::ReadyCallDevices => ".quill-ready-ready-call-devices",
        ScreenshotDemo::ReadyCallReconnecting => ".quill-ready-ready-call-reconnecting",
        ScreenshotDemo::ReadyChatTtl => ".quill-ready-ready-chat-ttl",
        ScreenshotDemo::ReadyMuteCustom => ".quill-ready-ready-mute-custom",
        ScreenshotDemo::ReadyAutoDelete => ".quill-ready-ready-auto-delete",
        ScreenshotDemo::ReadyGroupCall => ".quill-ready-ready-group-call",
        ScreenshotDemo::ReadyGroupCallInvite => ".quill-ready-ready-group-call-invite",
        ScreenshotDemo::ReadyGroupCallInvitation => ".quill-ready-ready-group-call-invitation",
        ScreenshotDemo::ReadyGroupCallManage => ".quill-ready-ready-group-call-manage",
        ScreenshotDemo::ReadyGroupCallScheduled => ".quill-ready-ready-group-call-scheduled",
        ScreenshotDemo::ReadyGroupCallPolish => ".quill-ready-ready-group-call-polish",
        ScreenshotDemo::ReadyGroupCallJoinAs => ".quill-ready-ready-group-call-join-as",
        ScreenshotDemo::ReadyCallsSettings => ".quill-ready-ready-calls-settings",
        ScreenshotDemo::ReadyPrivacy => ".quill-ready-ready-privacy",
        ScreenshotDemo::ReadyPrivacyGifts => ".quill-ready-ready-privacy-gifts",
        ScreenshotDemo::ReadySessionDetails => ".quill-ready-ready-session-details",
        ScreenshotDemo::ReadyFileOpenConfirm => ".quill-ready-ready-file-open-confirm",
        ScreenshotDemo::ReadyRichMessage => ".quill-ready-ready-rich-message",
        ScreenshotDemo::ReadyRichEditor => ".quill-ready-ready-rich-editor",
        ScreenshotDemo::ReadyRichAiTools => ".quill-ready-ready-rich-ai-tools",
        ScreenshotDemo::ReadyRichPremiumGate => ".quill-ready-ready-rich-premium-gate",
        ScreenshotDemo::ReadyProfileEdit => ".quill-ready-ready-profile-edit",
        ScreenshotDemo::ReadyProfilePanels => ".quill-ready-ready-profile-panels",
        ScreenshotDemo::ReadyMemberModeration => ".quill-ready-ready-member-moderation",
        ScreenshotDemo::ReadyGroupAdminSettings => ".quill-ready-ready-group-admin-settings",
        ScreenshotDemo::ReadyUsername => ".quill-ready-ready-username",
        ScreenshotDemo::ReadyShortcuts => ".quill-ready-ready-shortcuts",
        ScreenshotDemo::ReadyProxy => ".quill-ready-ready-proxy",
        ScreenshotDemo::ReadyScheduled => ".quill-ready-ready-scheduled",
        ScreenshotDemo::ReadyJumpDate => ".quill-ready-ready-jump-date",
        ScreenshotDemo::ReadySearchFrom => ".quill-ready-ready-search-from",
        ScreenshotDemo::ReadySearchFromHits => ".quill-ready-ready-search-from-hits",
        ScreenshotDemo::ReadySearchFilters => ".quill-ready-ready-search-filters",
        ScreenshotDemo::ReadySearchFrequent => ".quill-ready-ready-search-frequent",
        ScreenshotDemo::ReadySearchPublic => ".quill-ready-ready-search-public",
        ScreenshotDemo::ReadyPasscodeSettings => ".quill-ready-ready-passcode-settings",
        ScreenshotDemo::ReadyPasscodeCreate => ".quill-ready-ready-passcode-create",
        ScreenshotDemo::ReadyLockScreen => ".quill-ready-ready-lock-screen",
        ScreenshotDemo::ReadyChatPreview => ".quill-ready-ready-chat-preview",
    });
    let _ = std::fs::remove_file(&marker);
    let marker_for_spawn = marker.clone();

    eprintln!("quill screenshot-demo: {kind:?} → {}", out_dir.display());

    // Demo-only window size override (`QUILL_DEMO_WINDOW_SIZE=1200x1100`)
    // for slices whose fixture needs more vertical room than the default
    // 1200x740 (e.g. ready-location's four rows). Unset = unchanged, so
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
            ui::set_theme_mode(startup_theme_mode(), None, cx);
            // stories-high-contrast: screenshot demos can opt into the
            // high-contrast palette with `QUILL_DEMO_THEME=high-contrast`.
            ui::set_high_contrast(
                std::env::var("QUILL_DEMO_THEME").as_deref() == Ok("high-contrast"),
            );
            // kit Phase 9: honor the OS reduce-motion preference.
            cx.set_reduce_motion(os_prefers_reduced_motion());
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
                            ..quill_window_options(if kind == ScreenshotDemo::ReadyCallDevices {
                                "Quill — Call audio devices"
                            } else {
                                "Quill"
                            })
                        },
                        move |window, cx| {
                            let view = cx.new(|cx| {
                                ui::QuillApp::new_with_demo(window, cx, None, Some(kind))
                            });
                            if kind == ScreenshotDemo::ReadyTrayBehavior {
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
                    .expect("failed to open screenshot demo window");

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
                    .timer(Duration::from_millis(
                        if kind == ScreenshotDemo::ReadyStickerPlayback {
                            400
                        } else {
                            1500
                        },
                    ))
                    .await;
                // `QUILL_DEMO_CLICK=x,y[;x,y…]` (demo-capture only): left-click
                // at window points before the capture, to verify click paths.
                #[cfg(feature = "demo-capture")]
                if let Ok(clicks) = std::env::var("QUILL_DEMO_CLICK") {
                    // Through the untyped handle: the typed one leases the
                    // root view while the event dispatches, and a handler
                    // reading it (a kit button) panicked.
                    use gpui_kit::gpui::AnyWindowHandle;
                    // `QUILL_DEMO_CLICK_LOG=path` records when each step ran
                    // (Unix ms), so a recording can be lined up with it.
                    let mut step_log = std::env::var_os("QUILL_DEMO_CLICK_LOG").and_then(|path| {
                        std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(path)
                            .ok()
                    });
                    for point in clicks.split(';') {
                        if let Some(log) = step_log.as_mut() {
                            use std::io::Write;
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or_default();
                            let _ = writeln!(log, "{now} {}", point.trim());
                        }
                        // `p:ms` waits, to pace a scripted recording.
                        if let Some(ms) = point.strip_prefix("p:") {
                            if let Ok(ms) = ms.trim().parse::<u64>() {
                                cx.background_executor()
                                    .timer(Duration::from_millis(ms))
                                    .await;
                            }
                            continue;
                        }
                        // `m:x,y` moves the pointer without clicking (hover).
                        if let Some(hover) = point.strip_prefix("m:") {
                            if let Some((x, y)) = hover.split_once(',')
                                && let (Ok(x), Ok(y)) =
                                    (x.trim().parse::<f32>(), y.trim().parse::<f32>())
                            {
                                let _ = AnyWindowHandle::from(demo_window).update(
                                    cx,
                                    |_, window, cx| {
                                        use gpui_kit::gpui::{
                                            Modifiers, MouseMoveEvent, PlatformInput, point, px,
                                        };
                                        window.dispatch_event(
                                            PlatformInput::MouseMove(MouseMoveEvent {
                                                position: point(px(x), px(y)),
                                                pressed_button: None,
                                                modifiers: Modifiers::default(),
                                            }),
                                            cx,
                                        );
                                    },
                                );
                            }
                            continue;
                        }
                        // `k:key` presses one key (GPUI key names, e.g. `escape`).
                        if let Some(key) = point.strip_prefix("k:") {
                            let key = key.trim().to_string();
                            let _ =
                                AnyWindowHandle::from(demo_window).update(cx, |_, window, cx| {
                                    use gpui_kit::gpui::{
                                        KeyDownEvent, KeyUpEvent, Keystroke, Modifiers,
                                        PlatformInput,
                                    };
                                    let keystroke = Keystroke {
                                        modifiers: Modifiers::default(),
                                        key: key.clone(),
                                        key_char: None,
                                    };
                                    window.dispatch_event(
                                        PlatformInput::KeyDown(KeyDownEvent {
                                            keystroke: keystroke.clone(),
                                            is_held: false,
                                            prefer_character_input: false,
                                        }),
                                        cx,
                                    );
                                    window.dispatch_event(
                                        PlatformInput::KeyUp(KeyUpEvent { keystroke }),
                                        cx,
                                    );
                                });
                            cx.background_executor()
                                .timer(Duration::from_millis(120))
                                .await;
                            continue;
                        }
                        // `w:x,y,dx,dy,s|m|e` sends one phased trackpad scroll
                        // event (started / moved / ended), to script a swipe.
                        if let Some(wheel) = point.strip_prefix("w:") {
                            let parts: Vec<&str> = wheel.split(',').map(str::trim).collect();
                            if let [x, y, dx, dy, phase] = parts[..]
                                && let (Ok(x), Ok(y), Ok(dx), Ok(dy)) = (
                                    x.parse::<f32>(),
                                    y.parse::<f32>(),
                                    dx.parse::<f32>(),
                                    dy.parse::<f32>(),
                                )
                            {
                                let _ = AnyWindowHandle::from(demo_window).update(
                                    cx,
                                    |_, window, cx| {
                                        use gpui_kit::gpui::{
                                            Modifiers, PlatformInput, ScrollDelta,
                                            ScrollWheelEvent, TouchPhase, point, px,
                                        };
                                        let touch_phase = match phase {
                                            "s" => TouchPhase::Started,
                                            "e" => TouchPhase::Ended,
                                            _ => TouchPhase::Moved,
                                        };
                                        window.dispatch_event(
                                            PlatformInput::ScrollWheel(ScrollWheelEvent {
                                                position: point(px(x), px(y)),
                                                delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
                                                modifiers: Modifiers::default(),
                                                touch_phase,
                                            }),
                                            cx,
                                        );
                                    },
                                );
                            }
                            cx.background_executor()
                                .timer(Duration::from_millis(40))
                                .await;
                            continue;
                        }
                        // `s:x,y,dy` scrolls by `dy` px at the point instead.
                        if let Some(scroll) = point.strip_prefix("s:") {
                            let parts: Vec<f32> = scroll
                                .split(',')
                                .filter_map(|v| v.trim().parse().ok())
                                .collect();
                            if let [x, y, dy] = parts[..] {
                                let _ = AnyWindowHandle::from(demo_window).update(
                                    cx,
                                    |_, window, cx| {
                                        use gpui_kit::gpui::{
                                            Modifiers, PlatformInput, ScrollDelta,
                                            ScrollWheelEvent, TouchPhase, point, px,
                                        };
                                        window.dispatch_event(
                                            PlatformInput::ScrollWheel(ScrollWheelEvent {
                                                position: point(px(x), px(y)),
                                                delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                                                modifiers: Modifiers::default(),
                                                touch_phase: TouchPhase::Moved,
                                            }),
                                            cx,
                                        );
                                    },
                                );
                            }
                            cx.background_executor()
                                .timer(Duration::from_millis(400))
                                .await;
                            continue;
                        }
                        let Some((x, y)) = point.split_once(',') else {
                            continue;
                        };
                        let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>())
                        else {
                            continue;
                        };
                        let position =
                            gpui_kit::gpui::point(gpui_kit::gpui::px(x), gpui_kit::gpui::px(y));
                        let _ = AnyWindowHandle::from(demo_window).update(cx, |_, window, cx| {
                            use gpui_kit::gpui::{
                                Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
                                PlatformInput,
                            };
                            window.dispatch_event(
                                PlatformInput::MouseMove(MouseMoveEvent {
                                    position,
                                    pressed_button: None,
                                    modifiers: Modifiers::default(),
                                }),
                                cx,
                            );
                            window.dispatch_event(
                                PlatformInput::MouseDown(MouseDownEvent {
                                    button: MouseButton::Left,
                                    position,
                                    modifiers: Modifiers::default(),
                                    click_count: 1,
                                    first_mouse: false,
                                }),
                                cx,
                            );
                            window.refresh();
                        });
                        // A real click spans frames: let one render between
                        // press and release.
                        cx.background_executor()
                            .timer(Duration::from_millis(120))
                            .await;
                        let _ = AnyWindowHandle::from(demo_window).update(cx, |_, window, cx| {
                            use gpui_kit::gpui::{
                                Modifiers, MouseButton, MouseUpEvent, PlatformInput,
                            };
                            window.dispatch_event(
                                PlatformInput::MouseUp(MouseUpEvent {
                                    button: MouseButton::Left,
                                    position,
                                    modifiers: Modifiers::default(),
                                    click_count: 1,
                                }),
                                cx,
                            );
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(400))
                            .await;
                    }
                }
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
                                .update(cx, |_, window, _| window.render_to_image())
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
                                    PlatformInput::ScrollWheel(ScrollWheelEvent {
                                        position: point(px(x), px(y)),
                                        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                                        modifiers: Modifiers::default(),
                                        touch_phase: TouchPhase::Moved,
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
                            .unwrap_or(if kind == ScreenshotDemo::ReadyStickerPlayback {
                                7500
                            } else {
                                3500
                            })
                            .clamp(3500, 600000),
                    ))
                    .await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
}

#[cfg(all(test, feature = "ui"))]
mod demo_kind_tests {
    use crate::{DEMO_TABLE, demo_kinds, parse_screenshot_demo};

    #[test]
    fn every_demo_kind_parses() {
        for (kind, demo) in DEMO_TABLE {
            let args = vec![
                "quill".to_string(),
                "--screenshot-demo".to_string(),
                (*kind).to_string(),
            ];
            let parsed = parse_screenshot_demo(&args).expect(kind);
            assert_eq!(parsed.0, *demo, "{kind}");
        }
    }

    #[test]
    fn demo_kinds_are_unique_sorted_and_listed() {
        let kinds: Vec<&str> = DEMO_TABLE.iter().map(|(k, _)| *k).collect();
        let mut sorted = kinds.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(kinds, sorted);
        assert_eq!(demo_kinds().split('|').count(), kinds.len());
    }
}
