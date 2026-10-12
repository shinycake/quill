//! App themes on a demo `QuillApp`: the applied palette reaches the kit
//! and Quill's tokens, chats get the theme's bubbles, and the editor
//! previews live and reverts on cancel. Nothing here writes to disk.

use super::editor::{EditorRow, EditorSource};
use super::installed;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::{AppContext, Entity, Hsla, TestAppContext, VisualTestContext, px, rgb, size};
use quill::app_theme::{BuiltinTheme, PaletteKey, ThemeId};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// Apply the prefs as they are now, in memory only.
fn reapply(app: &mut QuillApp, cx: &mut gpui_kit::Context<QuillApp>) {
    app.settings.appearance_applied = None;
    app.apply_appearance(cx);
}

#[gpui_kit::test]
fn built_in_themes_drive_the_kit_tokens_and_chats(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.appearance.night_theme = ThemeId::Builtin(BuiltinTheme::Night);
        app.appearance.theme = quill::settings::ThemeChoice::Dark;
        app.appearance.auto_night = quill::settings::AutoNight::Off;
        reapply(app, cx);
        assert!(cx.theme().is_dark());
        assert_eq!(cx.theme().background, Hsla::from(rgb(0x282e33)));
        assert_eq!(installed(PaletteKey::WindowBgActive), Some(0x3fc1b0ff));
        assert_eq!(
            Hsla::from(crate::ui::chat_theme::accent_strong()),
            Hsla::from(rgb(0x3fc1b0))
        );
        // Chats without a theme get the theme's outgoing bubbles and
        // background.
        let (settings, dark) = app.default_chat_theme().expect("defaults");
        assert!(dark);
        assert_eq!(
            settings.outgoing_fill,
            Some(quill::telegram::envelope::BackgroundFill::Solid(0x2a2f33))
        );
        assert!(app.theme_wallpaper().is_some());

        // An accent recolours the day theme; Classic shows light.
        app.appearance.theme = quill::settings::ThemeChoice::Light;
        app.appearance.day_theme = ThemeId::Builtin(BuiltinTheme::Classic);
        app.appearance
            .theme_accents
            .insert("classic".into(), 0xd46c99);
        reapply(app, cx);
        assert!(!cx.theme().is_dark());
        assert_eq!(app.applied_builtin_theme(), Some(BuiltinTheme::Classic));
        assert_ne!(installed(PaletteKey::WindowBgActive), Some(0x40a7e3ff));

        // High contrast keeps its own palette: nothing is installed.
        app.appearance.theme = quill::settings::ThemeChoice::HighContrast;
        reapply(app, cx);
        assert_eq!(installed(PaletteKey::WindowBg), None);
        assert!(app.default_chat_theme().is_none());
    });
}

/// Nobody who keeps the defaults sees a change: Quill Light and Quill Dark
/// install no palette, so the token pairs, the kit's colours with the old
/// accent override, the accent bubbles and the plain chat area stay.
#[gpui_kit::test]
fn quill_themes_are_the_defaults_and_look_as_before(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        for (choice, theme, accent_strong) in [
            (
                quill::settings::ThemeChoice::Light,
                BuiltinTheme::QuillLight,
                0x0969da,
            ),
            (
                quill::settings::ThemeChoice::Dark,
                BuiltinTheme::QuillDark,
                0x1f6feb,
            ),
        ] {
            app.appearance = quill::settings::AppearancePrefs {
                theme: choice,
                ..Default::default()
            };
            reapply(app, cx);
            assert_eq!(app.applied_builtin_theme(), Some(theme));
            assert_eq!(cx.theme().is_dark(), theme.is_dark());
            assert_eq!(installed(PaletteKey::WindowBg), None);
            assert!(app.default_chat_theme().is_none(), "accent bubbles stay");
            assert!(app.theme_wallpaper().is_none());
            let strong = crate::ui::chat_theme::accent_strong();
            assert_eq!(Hsla::from(strong), Hsla::from(rgb(accent_strong)));
            assert_eq!(cx.theme().primary, Hsla::from(strong));
            // The old single accent still re-points the kit's primary.
            app.appearance.accent_rgb = 0x8957e5;
            reapply(app, cx);
            assert_eq!(cx.theme().primary, Hsla::from(rgb(0x8957e5)));
            assert_eq!(installed(PaletteKey::WindowBg), None);
        }
    });
}

#[gpui_kit::test]
fn the_editor_previews_live_and_cancel_reverts(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.appearance.day_theme = ThemeId::Builtin(BuiltinTheme::Day);
        app.appearance.theme = quill::settings::ThemeChoice::Light;
        app.appearance.auto_night = quill::settings::AutoNight::Off;
        reapply(app, cx);
        app.open_theme_editor(EditorSource::Current, window, cx);
        assert!(app.settings.app_theme.editor.is_some());
        app.select_editor_row(EditorRow::Key(PaletteKey::WindowBg), window, cx);
        app.editor_color_changed(Hsla::from(rgb(0x123456)), cx);
        assert_eq!(installed(PaletteKey::WindowBg), Some(0x123456ff));
        assert_eq!(cx.theme().background, Hsla::from(rgb(0x123456)));
        assert!(!cx.theme().is_dark(), "darkness follows the chat list");
        app.select_editor_row(EditorRow::Key(PaletteKey::DialogsBg), window, cx);
        app.editor_color_changed(Hsla::from(rgb(0x123456)), cx);
        assert!(cx.theme().is_dark(), "a dark chat list makes a dark theme");
        app.select_editor_row(EditorRow::Background, window, cx);
        app.editor_color_changed(Hsla::from(rgb(0x00ff00)), cx);
        assert_eq!(
            app.theme_wallpaper(),
            Some(crate::ui::wallpaper::Wallpaper::Solid(0x00ff00))
        );
        app.close_theme_editor(cx);
        assert_eq!(installed(PaletteKey::WindowBg), Some(0xffffffff));
        assert!(!cx.theme().is_dark());
    });
}
