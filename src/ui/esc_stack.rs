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
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::lang::{keys, tr};

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
        "first-run-intro",
        |app| app.first_run.intro_skippable(std::time::Instant::now()),
        |app, _, cx| app.skip_first_run_intro(cx)
    ),
    layer!(
        "instant-view",
        |app| app.live.as_ref().is_some_and(|live| live
            .driver
            .session
            .messages
            .instant_view
            .is_some()),
        |app, _, cx| {
            if let Some(live) = app.live.as_mut() {
                live.driver.session.messages.instant_view = None;
            }
            cx.notify();
        }
    ),
    layer!(
        "context-menus",
        |app| app.context_menu_open(),
        |app, _, cx| {
            app.dismiss_context_menus(cx);
        }
    ),
    layer!(
        "link-popup",
        |app| app.message_ui.link_popup.is_some(),
        |app, _, cx| {
            app.message_ui.link_popup = None;
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
        "earn-layer",
        |app| app.earn_layer_open(),
        |app, window, cx| app.earn_layer_escape(window, cx)
    ),
    layer!(
        "composer-code-language",
        |app| app.composer_ui.code_language.is_some(),
        |app, window, cx| app.close_code_language_dialog(window, cx)
    ),
    layer!(
        "composer-date-dialog",
        |app| app.composer_ui.date_dialog.is_some(),
        |app, window, cx| app.close_composer_date_dialog(window, cx)
    ),
    layer!(
        "composer-link-dialog",
        |app| app.composer_ui.link_dialog.is_some(),
        |app, window, cx| app.close_composer_link_dialog(window, cx)
    ),
    // Slice CL: the peek preview is the most transient layer.
    layer!(
        "chat-preview",
        |app| app.chat_list.preview.is_some() || app.chat_list.preview_press.is_some(),
        |app, _, cx| app.close_chat_preview(cx)
    ),
    // The voice-chat rename box sits on the group-call panel; the panel
    // itself is a live call and never closes on Escape.
    layer!(
        "screen-privacy",
        |app| app.screen_share.privacy_for.is_some(),
        |app, _, cx| app.close_screen_privacy(cx)
    ),
    layer!(
        "group-call-settings",
        |app| app.group_call.settings_open,
        |app, _, cx| app.close_group_call_settings(cx)
    ),
    layer!(
        "group-call-title",
        |app| app.group_call.title_dialog.is_some(),
        |app, _, cx| app.close_group_call_title_dialog(cx)
    ),
    // Phase 9.7: the chat story page paints above the story composer.
    layer!(
        "story-page",
        |app| app.stories.page.is_some(),
        |app, _, cx| app.close_story_page(cx)
    ),
    // Phase 9.3: the story composer is above the story viewer.
    layer!(
        "story-composer",
        |app| app.stories.composer.open,
        |app, _, cx| app.close_story_composer(cx)
    ),
    // The story reply row's voice recording and emoji / sticker / GIF
    // panel close before the viewer does.
    layer!(
        "story-reply-recording",
        |app| app.story_reply_recording_active(),
        |app, _, cx| {
            if app.recording.locked {
                app.connection.set_note(StatusNote::silent(tr(
                    keys::Q_MISC_RECORDING_IS_LOCKED_UNLOCK_IT,
                )));
                cx.notify();
            } else {
                app.request_discard_recording(cx);
            }
        }
    ),
    layer!(
        "story-reply-panel",
        |app| app.story_reply_panel_active(),
        |app, _, cx| {
            app.stories.reply_panel = false;
            app.close_media_panel(cx);
        }
    ),
    // Phase 9.1: the story viewer is above the media viewer.
    layer!(
        "story-viewer",
        |app| app.stories.viewer.is_open(),
        |app, _, cx| app.close_story_viewer(cx)
    ),
    layer!(
        "photo-editor",
        |app| app.viewer.photo_editor.is_some(),
        |app, _, cx| app.close_photo_editor(cx)
    ),
    layer!(
        "media-viewer",
        |app| app.viewer.state.is_open(),
        |app, _, cx| app.close_media_viewer(cx)
    ),
    layer!(
        "checklist-dialog",
        |app| app.composer_ui.checklist_dialog.is_some(),
        |app, _, cx| app.close_checklist_dialog(cx)
    ),
    layer!(
        "share-content-dialog",
        |app| app.share.content_dialog.is_some(),
        |app, _, cx| app.close_share_content_dialog(cx)
    ),
    layer!(
        "poll-add-option",
        |app| app.message_ui.poll_add_option.is_some(),
        |app, _, cx| app.close_poll_add_option(cx)
    ),
    layer!(
        "poll-deadline",
        |app| app.poll_deadline_box_open(),
        |app, _, cx| app.close_poll_deadline_box(cx)
    ),
    layer!("country-box", |app| app.country_box_open(), |app, _, cx| {
        app.close_open_country_boxes(cx)
    }),
    layer!(
        "poll-dialog",
        |app| app.composer_ui.poll_dialog.is_some(),
        |app, _, cx| app.request_close_poll_dialog(cx)
    ),
    // Slice P1: the checkout and receipt dialogs.
    layer!(
        "payment-dialog",
        |app| app.payments.dialog.is_some(),
        |app, _, cx| app.close_payment_dialog(cx)
    ),
    layer!(
        "payment-receipt",
        |app| app
            .session()
            .is_some_and(|session| session.payments.receipt_open),
        |app, _, cx| app.close_payment_receipt(cx)
    ),
    // MED2: Esc never discards a recording silently. With the confirm row
    // open, Esc dismisses it and keeps recording; otherwise Esc opens the
    // confirm row (locked recordings ignore Esc beyond a hint).
    layer!(
        "record-discard-confirm",
        |app| app.recording.discard_confirm && app.recording_here(),
        |app, _, cx| {
            app.recording.discard_confirm = false;
            cx.notify();
        }
    ),
    layer!("recording", |app| app.recording_here(), |app, _, cx| {
        if app.recording.locked {
            app.connection.set_note(StatusNote::silent(tr(
                keys::Q_MISC_RECORDING_IS_LOCKED_UNLOCK_IT,
            )));
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
    // Devices page: Esc cancels a pending terminate confirmation first,
    // then leaves a session's details.
    layer!("sessions", |app| app.sessions_layer_open(), |app, _, cx| {
        app.close_sessions_top(cx)
    }),
    // Slice S3: privacy peels one level per press (picker, exceptions,
    // then the editor), like TGX's back stack.
    layer!(
        "privacy",
        |app| app.privacy.editor.is_some()
            || app.privacy.exceptions.is_some()
            || app.privacy.block_picker_open,
        |app, _, cx| app.close_privacy_top(cx)
    ),
    layer!(
        "mute-menu",
        |app| app.notify.mute_menu_open,
        |app, _, cx| app.close_mute_menu(cx)
    ),
    // Phase B4: the TTL picker.
    layer!(
        "ttl-picker",
        |app| app.notify.ttl_picker_open,
        |app, _, cx| {
            app.notify.ttl_picker_open = false;
            cx.notify();
        }
    ),
    layer!(
        "sponsored-report",
        |app| app
            .session()
            .is_some_and(|session| session.messages.sponsored_report.is_some()),
        |app, _, cx| app.dismiss_sponsored_report_ui(cx)
    ),
    layer!(
        "send-as",
        |app| app.composer_ui.send_as_open,
        |app, _, cx| {
            app.composer_ui.send_as_open = false;
            cx.notify();
        }
    ),
    layer!(
        "reply-panels",
        |app| (app.share.reply_elsewhere_open || app.share.reply_quote_open)
            && app.composer_ui.has_reply(),
        |app, window, cx| app.close_reply_panels(window, cx)
    ),
    layer!(
        "forward-picker",
        |app| app.share.forward_picker_open,
        |app, window, cx| app.close_forward_picker(window, cx)
    ),
    layer!(
        "pending-delete",
        |app| app.message_ui.pending_delete.is_some(),
        |app, _, cx| app.cancel_delete(cx)
    ),
    // B4: Escape cancels the stop-poll confirm too.
    layer!(
        "pending-stop-poll",
        |app| app.message_ui.pending_stop_poll.is_some(),
        |app, _, cx| app.cancel_stop_poll(cx)
    ),
    // The chat list peeking over the conversation (`sidebar_reveal`).
    layer!(
        "sidebar-peek",
        |app| app.frame.sidebar.pointer() != super::sidebar_reveal::Peek::Off,
        |app, window, cx| app.dismiss_sidebar_peek(window, cx)
    ),
    // Slice media-shared-gallery: above the chat-search layer (the row-click
    // jump closes the gallery into a chat search).
    layer!(
        "shared-media",
        |app| app.session().is_some_and(|session| {
            session.media.shared_media.open || session.media.shared_media.playlist_user.is_some()
        }),
        |app, _, cx| app.close_shared_media_ui(cx)
    ),
    layer!(
        "chat-search",
        |app| app.chat_search_is_open(),
        |app, window, cx| app.close_chat_search_ui(window, cx)
    ),
    layer!("search", |app| app.search_is_open(), |app, window, cx| {
        let query = app.search_ui.input.read(cx).value().to_string();
        if query.trim().is_empty() {
            app.close_search_ui(window, cx);
        } else {
            app.search_ui
                .input
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
        |app| app.composer_ui.has_reply(),
        |app, _, cx| app.clear_reply(cx)
    ),
    layer!(
        "pending-edit",
        |app| app.composer_ui.pending_edit.is_some(),
        |app, window, cx| app.request_cancel_edit(window, cx)
    ),
    layer!(
        "pending-forward",
        |app| app.share.pending_forward.is_some(),
        |app, window, cx| app.clear_forward(window, cx)
    ),
    layer!(
        "forward-result",
        |app| app.share.forward_result.is_some(),
        |app, _, cx| {
            app.share.forward_result = None;
            app.connection.set_note(StatusNote::silent(tr(
                keys::Q_MISC_FORWARD_RESULT_DISMISSED,
            )));
            cx.notify();
        }
    ),
    // Side panels beside the conversation go last: they are not modal.
    layer!(
        "downloads-panel",
        |app| app
            .session()
            .is_some_and(|session| session.media.downloads_panel_open),
        |app, _, cx| {
            if let Some(live) = app.live.as_mut() {
                live.driver.session.media.downloads_panel_open = false;
            } else if let Some(session) = app.demo_session.as_mut() {
                session.media.downloads_panel_open = false;
            }
            cx.notify();
        }
    ),
    layer!(
        "info-panel",
        |app| !app.profile_modal_active()
            && app
                .session()
                .is_some_and(|session| session.users_state.open_info_panel.is_some()),
        |app, _, cx| app.close_info_panel(cx)
    ),
];

/// Layers that give focus back to whatever had it before they opened.
const FOCUS_RETURN_LAYERS: &[&str] = &[
    "profile-modal",
    "earn-layer",
    "composer-code-language",
    "composer-date-dialog",
    "composer-link-dialog",
    "gif-panel",
    "sticker-panel",
    "reply-panels",
    "forward-picker",
];

/// Bookkeeping for [`QuillApp::track_layer_focus`]. Weak handles only: a
/// strong one would keep a closed dialog's focus handle alive.
#[derive(Default)]
pub(super) struct LayerFocus {
    /// Focus as of the last frame with no tracked layer open.
    base: Option<WeakFocusHandle>,
    /// Focus as of the last frame with a tracked layer open.
    open_focus: Option<WeakFocusHandle>,
    was_open: bool,
}

impl QuillApp {
    fn focus_return_layer_open(&self) -> bool {
        self.pickers.media_panel.open
            || ESC_LAYERS
                .iter()
                .any(|layer| FOCUS_RETURN_LAYERS.contains(&layer.name) && (layer.is_open)(self))
    }

    /// One shared focus return for the hand-drawn layers. Runs every frame:
    /// while no tracked layer is open it remembers the focused element; when
    /// the last one closes, focus goes back there unless the close path
    /// already moved it somewhere on purpose (context menus do the same).
    pub(super) fn track_layer_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.focus_return_layer_open();
        let focused = window.focused(cx);
        let state = &mut self.frame.layer_focus;
        if open {
            state.open_focus = focused.map(|handle| handle.downgrade());
        } else if state.was_open {
            let open_focus = state.open_focus.take().and_then(|weak| weak.upgrade());
            let untouched = focused.is_none() || focused == open_focus;
            let base = state.base.take().and_then(|weak| weak.upgrade());
            if untouched && let Some(base) = base {
                window.focus(&base, cx);
            }
            state.base = window.focused(cx).map(|handle| handle.downgrade());
        } else {
            state.base = focused.map(|handle| handle.downgrade());
        }
        self.frame.layer_focus.was_open = open;
    }
}

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
        if self.account.passcode.locked {
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
#[cfg(test)]
#[cfg(feature = "demo-capture")]
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
                QuillApp::new_with_demo(
                    window,
                    cx,
                    None,
                    Some(ScreenshotDemo::named("ready-chats")),
                )
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

    fn any_rule() -> crate::ui::privacy::PrivacyEditorTarget {
        crate::ui::privacy::PrivacyEditorTarget::Rule(
            quill::telegram::requests_privacy::PrivacySettingKey::ShowStatus,
        )
    }

    type Opener = fn(&mut QuillApp, &mut gpui_kit::Window, &mut gpui_kit::Context<QuillApp>);

    /// One opener per hand-drawn overlay, with the layer name it must show.
    fn openers() -> Vec<(&'static str, Opener)> {
        vec![
            ("privacy", |app, _, _| app.privacy.editor = Some(any_rule())),
            ("privacy", |app, _, _| {
                app.privacy.editor = Some(any_rule());
                app.privacy.exception_picker_open = true;
            }),
            ("story-page", |app, window, cx| {
                app.stories.page =
                    Some(crate::ui::story_page::StoryPage::new(ChatId(1), window, cx));
            }),
            ("group-call-title", |app, window, cx| {
                app.group_call.title_dialog = Some(
                    crate::ui::dialogs::group_call::GroupCallTitleDialog::new(window, cx, "x"),
                );
            }),
            ("link-popup", |app, _, _| {
                app.message_ui.link_popup = Some(crate::ui::entity_links::LinkPopup {
                    position: point(px(10.), px(10.)),
                    link: quill::text::LinkTarget::Mention("@x".into()),
                });
            }),
            ("context-menus", |app, _, _| {
                app.chat_list.archive_menu = Some(point(px(10.), px(10.)));
            }),
            ("downloads-panel", |app, _, _| {
                if let Some(session) = app.demo_session.as_mut() {
                    session.media.downloads_panel_open = true;
                }
            }),
        ]
    }

    #[gpui_kit::test]
    fn escape_closes_each_overlay(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        for (name, open) in openers() {
            app.update_in(vcx, open);
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
            app.privacy.editor = Some(any_rule());
            app.stories.page = Some(crate::ui::story_page::StoryPage::new(ChatId(1), window, cx));
            app.message_ui.link_popup = Some(crate::ui::entity_links::LinkPopup {
                position: point(px(10.), px(10.)),
                link: quill::text::LinkTarget::Mention("@x".into()),
            });
        });
        assert_eq!(top(&app, vcx), Some("link-popup"));
        vcx.simulate_keystrokes("escape");
        assert_eq!(top(&app, vcx), Some("story-page"));
        assert!(app.read_with(vcx, |app, _| app.privacy.editor.is_some()));
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
            app.privacy.editor = Some(any_rule());
            app.account.passcode.locked = true;
        });
        vcx.simulate_keystrokes("escape");
        let (still_open, still_locked) = app.read_with(vcx, |app, _| {
            (app.privacy.editor.is_some(), app.account.passcode.locked)
        });
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
                QuillApp::new_with_demo(
                    window,
                    cx,
                    None,
                    Some(ScreenshotDemo::named("ready-chats")),
                )
            });
            *slot_in.borrow_mut() = Some(view.clone());
            let shell = cx.new(|_| QuillShell::new(view));
            Root::new(shell, window, cx)
        });
        let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
        let flags: [(&str, fn(&mut QuillApp), fn(&QuillApp) -> bool); 3] = [
            (
                "marketplace",
                |a| a.payments.marketplace_open = true,
                |a| a.payments.marketplace_open,
            ),
            (
                "passcode settings",
                |a| a.account.passcode.open = true,
                |a| a.account.passcode.open,
            ),
            (
                "file open warning",
                |a| {
                    a.privacy.extra.file_open = Some(crate::ui::privacy_extra::FileOpenConfirm {
                        path: std::path::PathBuf::from("setup.bin"),
                        warning: quill::file_prefs::OpenWarning::Executable,
                    });
                },
                |a| a.privacy.extra.file_open.is_some(),
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

/// Closing each surface lets go of what opening it made.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "entity_leaks_tests.rs"]
pub(in crate::ui) mod entity_leaks_tests;
