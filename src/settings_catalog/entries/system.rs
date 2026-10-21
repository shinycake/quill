//! System page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page, Platforms};

const P: Page = Page::System;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("system.launch_at_login", P, keys::Q_PREFS_LAUNCH_AT_LOGIN).keywords(&[
        "autostart",
        "startup",
        "boot",
    ]),
    Entry::new("system.start_in_tray", P, keys::Q_PREFS_START_IN_TRAY)
        .keywords(&["minimized", "hidden"]),
    Entry::new("system.tray_icon", P, keys::SETTINGS_WORKMODE_TRAY).keywords(&[
        "menu bar",
        "notification area",
        "system tray",
    ]),
    Entry::new("system.taskbar_icon", P, keys::Q_MISC_SHOW_TASKBAR_ICON)
        .keywords(&["dock"])
        .only(Platforms::WINDOWS.or(Platforms::LINUX)),
    Entry::new(
        "system.monochrome_icon",
        P,
        keys::Q_MISC_USE_MONOCHROME_ICON,
    )
    .keywords(&["tray", "dark taskbar"])
    .only(Platforms::WINDOWS),
    Entry::new(
        "system.when_closed",
        P,
        keys::Q_PREFS_WHEN_THE_WINDOW_IS_CLOSED,
    )
    .keywords(&["close button", "quit", "background"]),
    Entry::new("system.window_frame", P, keys::SETTINGS_NATIVE_FRAME)
        .keywords(&["title bar", "native window"])
        .only(Platforms::LINUX.or(Platforms::WINDOWS)),
    Entry::new(
        "system.open_links",
        P,
        keys::Q_PREFS_OPEN_TELEGRAM_LINKS_WITH_QUILL,
    )
    .keywords(&["tg://", "default app", "url handler"]),
    Entry::new(
        "system.send_to_menu",
        P,
        keys::Q_PREFS_PLACE_QUILL_IN_THE_SEND,
    )
    .keywords(&["explorer", "context menu"])
    .only(Platforms::WINDOWS),
    Entry::new("system.hardware_decoding", P, keys::SETTINGS_ENABLE_HWACCEL).keywords(&[
        "gpu",
        "video performance",
        "acceleration",
    ]),
    Entry::new("system.touch_bar", P, keys::Q_SETTINGS_SYSTEM_TOUCH_BAR)
        .keywords(&["macbook", "function row"])
        .only(Platforms::MACOS),
];
