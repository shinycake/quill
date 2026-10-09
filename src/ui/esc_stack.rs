//! The Escape overlay stack.
//!
//! Escape dismisses exactly one layer: the topmost thing that is open. The
//! layers live in one ordered table (topmost first) so a new overlay is one
//! row here instead of a branch somewhere in a long `if` chain that is easy
//! to forget. Kit dialogs (`DialogKind`) close themselves through the kit's
//! own Escape handling; this table is for everything the app draws by hand.
//!
//! The lock screen is deliberately not a row: while locked, Escape does
//! nothing at all (see `QuillApp::dismiss_topmost_layer`).

use super::app::QuillApp;
use gpui_kit::*;

/// One dismissible layer: how to tell it is open, and how to close it.
pub(super) struct EscLayer {
    /// Used by tests and diagnostics.
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: &'static str,
    pub is_open: fn(&QuillApp) -> bool,
    pub close: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>),
}

macro_rules! layer {
    ($name:literal, $open:expr, $close:expr) => {
        EscLayer {
            name: $name,
            is_open: $open,
            close: $close,
        }
    };
}

/// Topmost first; the first open layer wins. Order follows paint order in
/// `app_render` (later children paint above earlier ones), with transient
/// popups above the panels they were opened from.
pub(super) static ESC_LAYERS: &[EscLayer] = &[
    layer!(
        "instant-view",
        |app| app
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.instant_view.is_some()),
        |app, _, cx| {
            if let Some(live) = app.live.as_mut() {
                live.driver.session.instant_view = None;
            }
            cx.notify();
        }
    ),
    layer!(
        "context-menus",
        |app| app.message_menu.is_some() || app.chat_menu.is_some() || app.archive_menu.is_some(),
        |app, _, cx| {
            app.message_menu = None;
            app.chat_menu = None;
            app.archive_menu = None;
            cx.notify();
        }
    ),
    layer!(
        "link-popup",
        |app| app.link_popup.is_some(),
        |app, _, cx| {
            app.link_popup = None;
            cx.notify();
        }
    ),
    // The profile layer fades out before anything beneath it.
    layer!(
        "profile-modal",
        |app| app.profile_modal_active(),
        |app, _, cx| app.close_profile_modal(cx)
    ),
    layer!(
        "composer-link-dialog",
        |app| app.composer_link_dialog.is_some(),
        |app, window, cx| app.close_composer_link_dialog(window, cx)
    ),
    // Slice CL: the peek preview is the most transient layer.
    layer!(
        "chat-preview",
        |app| app.chat_preview.is_some() || app.preview_press.is_some(),
        |app, _, cx| app.close_chat_preview(cx)
    ),
    // The voice-chat rename box sits on the group-call panel; the panel
    // itself is a live call and never closes on Escape.
    layer!(
        "group-call-title",
        |app| app.group_call_title_dialog.is_some(),
        |app, _, cx| app.close_group_call_title_dialog(cx)
    ),
    // Phase 9.7: the chat story page paints above the story composer.
    layer!(
        "story-page",
        |app| app.story_page.is_some(),
        |app, _, cx| app.close_story_page(cx)
    ),
    // Phase 9.3: the story composer is above the story viewer.
    layer!(
        "story-composer",
        |app| app.story_composer.open,
        |app, _, cx| app.close_story_composer(cx)
    ),
    // Phase 9.1: the story viewer is above the media viewer.
    layer!(
        "story-viewer",
        |app| app.story_viewer.is_open(),
        |app, _, cx| app.close_story_viewer(cx)
    ),
    layer!(
        "photo-editor",
        |app| app.photo_editor.is_some(),
        |app, _, cx| app.close_photo_editor(cx)
    ),
    layer!(
        "media-viewer",
        |app| app.media_viewer.is_open(),
        |app, _, cx| app.close_media_viewer(cx)
    ),
    layer!(
        "poll-dialog",
        |app| app.poll_dialog.is_some(),
        |app, _, cx| app.request_close_poll_dialog(cx)
    ),
    // Slice P1: the checkout and receipt dialogs.
    layer!(
        "payment-dialog",
        |app| app.payment_dialog.is_some(),
        |app, _, cx| app.close_payment_dialog(cx)
    ),
    layer!(
        "payment-receipt",
        |app| app
            .session()
            .is_some_and(|session| session.payment_receipt_open),
        |app, _, cx| app.close_payment_receipt(cx)
    ),
    // MED2: Esc never discards a recording silently. With the confirm row
    // open, Esc dismisses it and keeps recording; otherwise Esc opens the
    // confirm row (locked recordings ignore Esc beyond a hint).
    layer!(
        "record-discard-confirm",
        |app| app.record_discard_confirm,
        |app, _, cx| {
            app.record_discard_confirm = false;
            cx.notify();
        }
    ),
    layer!("recording", |app| app.recording_active(), |app, _, cx| {
        if app.record_locked {
            app.status_note = "recording is locked — unlock it or use Cancel".into();
            cx.notify();
        } else {
            app.request_discard_recording(cx);
        }
    }),
    layer!("gif-panel", |app| app.gif_panel_open(), |app, _, cx| app
        .close_gif_panel(cx)),
    layer!(
        "sticker-panel",
        |app| app.sticker_panel_open(),
        |app, _, cx| app.close_sticker_panel(cx)
    ),
    layer!(
        "notification-defaults",
        |app| app.notification_defaults_open,
        |app, _, cx| {
            app.notification_defaults_open = false;
            app.defaults_sound_picker = None;
            app.defaults_exceptions_scope = None;
            cx.notify();
        }
    ),
    // Settings → Appearance: changes already applied live.
    layer!("appearance", |app| app.appearance_open, |app, _, cx| {
        app.close_appearance();
        cx.notify();
    }),
    // Slice A3: Esc on the sessions overlay cancels a pending terminate
    // confirmation first, then closes the overlay.
    layer!("sessions", |app| app.sessions_open, |app, _, cx| app
        .close_sessions(cx)),
    // Slice S3: privacy peels one level per press (picker, exceptions,
    // editor, then the main overlay), like TGX's back stack.
    layer!(
        "privacy",
        |app| app.privacy_open || app.privacy_editor.is_some() || app.privacy_exceptions.is_some(),
        |app, _, cx| app.close_privacy_top(cx)
    ),
    layer!("mute-menu", |app| app.mute_menu_open, |app, _, cx| app
        .close_mute_menu(cx)),
    // Phase B4: the TTL picker.
    layer!("ttl-picker", |app| app.ttl_picker_open, |app, _, cx| {
        app.ttl_picker_open = false;
        cx.notify();
    }),
    layer!(
        "sponsored-report",
        |app| app
            .session()
            .is_some_and(|session| session.sponsored_report.is_some()),
        |app, _, cx| app.dismiss_sponsored_report_ui(cx)
    ),
    layer!(
        "forward-picker",
        |app| app.forward_picker_open,
        |app, window, cx| app.close_forward_picker(window, cx)
    ),
    layer!(
        "pending-delete",
        |app| app.pending_delete.is_some(),
        |app, _, cx| app.cancel_delete(cx)
    ),
    // B4: Escape cancels the stop-poll confirm too.
    layer!(
        "pending-stop-poll",
        |app| app.pending_stop_poll.is_some(),
        |app, _, cx| app.cancel_stop_poll(cx)
    ),
    // Slice media-shared-gallery: above the chat-search layer (the row-click
    // jump closes the gallery into a chat search).
    layer!(
        "shared-media",
        |app| app
            .session()
            .is_some_and(|session| session.shared_media.open),
        |app, _, cx| app.close_shared_media_ui(cx)
    ),
    layer!(
        "chat-search",
        |app| app.chat_search_is_open(),
        |app, window, cx| app.close_chat_search_ui(window, cx)
    ),
    layer!("search", |app| app.search_is_open(), |app, window, cx| {
        let query = app.search_input.read(cx).value().to_string();
        if query.trim().is_empty() {
            app.close_search_ui(window, cx);
        } else {
            app.search_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            app.sync_search_query("", cx);
        }
    }),
    // Selection mode first: Escape leaves it before a reply or edit header
    // goes (tdesktop `HistoryWidget::escape`).
    layer!(
        "selection-mode",
        |app| app
            .session()
            .and_then(|session| session.open_chat)
            .is_some_and(|chat| app.selecting_in(chat)),
        |app, window, cx| app.clear_forward(window, cx)
    ),
    layer!(
        "pending-reply",
        |app| app.pending_reply.is_some(),
        |app, _, cx| app.clear_reply(cx)
    ),
    layer!(
        "pending-edit",
        |app| app.pending_edit.is_some(),
        |app, window, cx| app.clear_edit(window, cx)
    ),
    layer!(
        "pending-forward",
        |app| app.pending_forward.is_some(),
        |app, window, cx| app.clear_forward(window, cx)
    ),
    layer!(
        "forward-result",
        |app| app.forward_result.is_some(),
        |app, _, cx| {
            app.forward_result = None;
            app.status_note = "forward result dismissed".into();
            cx.notify();
        }
    ),
    // Side panels beside the conversation go last: they are not modal.
    layer!(
        "downloads-panel",
        |app| app
            .session()
            .is_some_and(|session| session.downloads_panel_open),
        |app, _, cx| {
            if let Some(live) = app.live.as_mut() {
                live.driver.session.downloads_panel_open = false;
            } else if let Some(session) = app.demo_session.as_mut() {
                session.downloads_panel_open = false;
            }
            cx.notify();
        }
    ),
    layer!(
        "info-panel",
        |app| !app.profile_modal_active()
            && app
                .session()
                .is_some_and(|session| session.open_info_panel.is_some()),
        |app, _, cx| app.close_info_panel(cx)
    ),
];

impl QuillApp {
    /// The topmost open layer, if any.
    pub(super) fn topmost_esc_layer(&self) -> Option<&'static EscLayer> {
        ESC_LAYERS.iter().find(|layer| (layer.is_open)(self))
    }

    /// Escape: close the topmost open layer and nothing else. Returns
    /// whether a layer was closed. The lock screen swallows Escape.
    pub(super) fn dismiss_topmost_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.passcode_ui.locked {
            return false;
        }
        let Some(layer) = self.topmost_esc_layer() else {
            return false;
        };
        (layer.close)(self, window, cx);
        true
    }
}

#[cfg(test)]
mod table_tests {
    #[test]
    fn layer_names_are_unique() {
        let mut names: Vec<_> = super::ESC_LAYERS.iter().map(|layer| layer.name).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "duplicate Escape layer name");
    }

    /// The overlays that used to have a close button but no Escape path.
    #[test]
    fn previously_missing_overlays_are_listed() {
        let names: Vec<_> = super::ESC_LAYERS.iter().map(|layer| layer.name).collect();
        for needed in [
            "privacy",
            "story-page",
            "group-call-title",
            "photo-editor",
            "link-popup",
            "instant-view",
            "context-menus",
            "downloads-panel",
            "info-panel",
        ] {
            assert!(names.contains(&needed), "missing Escape layer {needed}");
        }
    }
}

/// End-to-end: real Escape keystrokes through GPUI's dispatcher into a demo
/// `QuillApp` (needs the `demo-capture` feature for gpui-kit's test support).
#[cfg(all(test, feature = "demo-capture"))]
mod dispatch_tests {
    use crate::ui::app::QuillApp;
    use crate::ui::keybindings::bind_keys;
    use crate::ui::screenshot_demo::ScreenshotDemo;
    use crate::ui::shell::QuillShell;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Entity, Focusable, TestAppContext, VisualTestContext, point, px, size,
    };
    use quill::ids::ChatId;

    fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
        cx.update(gpui_kit::init);
        cx.update(bind_keys);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
            let view = cx.new(|cx| {
                QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::ReadyChats))
            });
            *slot_in.borrow_mut() = Some(view.clone());
            let focus = view.focus_handle(cx);
            window.focus(&focus, cx);
            let shell = cx.new(|_| QuillShell::new(view));
            Root::new(shell, window, cx)
        });
        let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
        let vcx = VisualTestContext::from_window(handle.into(), cx);
        (app, vcx)
    }

    fn top(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<&'static str> {
        app.read_with(cx, |app, _| app.topmost_esc_layer().map(|l| l.name))
    }

    type Opener = fn(&mut QuillApp, &mut gpui_kit::Window, &mut gpui_kit::Context<QuillApp>);

    /// One opener per hand-drawn overlay, with the layer name it must show.
    fn openers() -> Vec<(&'static str, Opener)> {
        vec![
            ("privacy", |app, _, _| app.privacy_open = true),
            ("privacy", |app, _, _| {
                app.privacy_open = true;
                app.exception_picker_open = true;
            }),
            ("story-page", |app, window, cx| {
                app.story_page = Some(crate::ui::story_page::StoryPage::new(ChatId(1), window, cx));
            }),
            ("group-call-title", |app, window, cx| {
                app.group_call_title_dialog = Some(
                    crate::ui::dialogs::group_call::GroupCallTitleDialog::new(window, cx, "x"),
                );
            }),
            ("link-popup", |app, _, _| {
                app.link_popup = Some(crate::ui::entity_links::LinkPopup {
                    position: point(px(10.), px(10.)),
                    link: quill::text::LinkTarget::Mention("@x".into()),
                });
            }),
            ("context-menus", |app, _, _| {
                app.archive_menu = Some(point(px(10.), px(10.)));
            }),
            ("downloads-panel", |app, _, _| {
                if let Some(session) = app.demo_session.as_mut() {
                    session.downloads_panel_open = true;
                }
            }),
        ]
    }

    #[gpui_kit::test]
    fn escape_closes_each_overlay(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        for (name, open) in openers() {
            app.update_in(vcx, |app, window, cx| open(app, window, cx));
            assert_eq!(top(&app, vcx), Some(name), "{name} should be on top");
            vcx.simulate_keystrokes("escape");
            // The privacy picker peels first; a second press closes the rest.
            if top(&app, vcx) == Some("privacy") {
                vcx.simulate_keystrokes("escape");
            }
            assert_eq!(top(&app, vcx), None, "Escape should close {name}");
        }
    }

    #[gpui_kit::test]
    fn escape_closes_only_the_topmost(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        app.update_in(vcx, |app, window, cx| {
            app.privacy_open = true;
            app.story_page = Some(crate::ui::story_page::StoryPage::new(ChatId(1), window, cx));
            app.link_popup = Some(crate::ui::entity_links::LinkPopup {
                position: point(px(10.), px(10.)),
                link: quill::text::LinkTarget::Mention("@x".into()),
            });
        });
        assert_eq!(top(&app, vcx), Some("link-popup"));
        vcx.simulate_keystrokes("escape");
        assert_eq!(top(&app, vcx), Some("story-page"));
        assert!(app.read_with(vcx, |app, _| app.privacy_open));
        vcx.simulate_keystrokes("escape");
        assert_eq!(top(&app, vcx), Some("privacy"));
        vcx.simulate_keystrokes("escape");
        assert_eq!(top(&app, vcx), None);
    }

    #[gpui_kit::test]
    fn lock_screen_ignores_escape(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        app.update_in(vcx, |app, _, _| {
            app.privacy_open = true;
            app.passcode_ui.locked = true;
        });
        vcx.simulate_keystrokes("escape");
        let (still_open, still_locked) =
            app.read_with(vcx, |app, _| (app.privacy_open, app.passcode_ui.locked));
        assert!(still_open, "Escape must not touch layers under the lock");
        assert!(still_locked, "Escape must never unlock");
    }

    /// Kit dialogs close through the kit's own Escape handling and clear
    /// the app flag via `on_close`.
    #[gpui_kit::test]
    fn escape_closes_kit_dialogs(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(bind_keys);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
            let view = cx.new(|cx| {
                QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::ReadyChats))
            });
            *slot_in.borrow_mut() = Some(view.clone());
            let shell = cx.new(|_| QuillShell::new(view));
            Root::new(shell, window, cx)
        });
        let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
        let flags: [(&str, fn(&mut QuillApp), fn(&QuillApp) -> bool); 3] = [
            (
                "proxy list",
                |a| a.proxy_ui.list_open = true,
                |a| a.proxy_ui.list_open,
            ),
            (
                "marketplace",
                |a| a.marketplace_open = true,
                |a| a.marketplace_open,
            ),
            (
                "passcode settings",
                |a| a.passcode_ui.open = true,
                |a| a.passcode_ui.open,
            ),
        ];
        for (name, set, get) in flags {
            cx.update_window(handle.into(), |_, window, cx| {
                app.update(cx, |a, cx| {
                    set(a);
                    cx.notify();
                });
                window.render_frame(cx);
                window.press("escape", cx);
            })
            .unwrap();
            cx.update(|cx| assert!(!get(app.read(cx)), "Escape should close the {name}"));
        }
    }
}
