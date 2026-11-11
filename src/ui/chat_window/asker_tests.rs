//! Round 14 (area B): what opens, opens for the window that asked
//! (`dialog_hosts::Asker`).
//!
//! - The poll loop, the main window's own upkeep and the toast drain are
//!   asked by no window: a box there shows in the window of the last press,
//!   and a link from there has no chat window as its origin (findings 4, 5).
//! - A link answered for a chat window runs in that window's frame, so it
//!   does not wait for a minimized main window (finding 1); the share pick
//!   goes the same way (finding 9).
//! - An answer that follows up on a box, or on a chat's message, opens in
//!   that box's or that chat's window (finding 4).
//! - What is one window's shows in it only: the camera circle, the custom
//!   emoji card (findings 7, 9); a chat's own choices stay with it (11).
#![cfg(feature = "demo-capture")]

use super::tests::{Harness, OPEN, OTHER, new_app};
use crate::ui::recording_state::CaptureFor;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use quill::deep_link_types::{DeepLinkUi, SettingsTarget};

fn draw(window: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
}

fn draw_both(h: &Harness, chat_window: AnyWindowHandle, cx: &mut TestAppContext) {
    for _ in 0..2 {
        draw(h.main, cx);
        draw(chat_window, cx);
    }
}

fn chat_window_shell(h: &Harness, cx: &mut TestAppContext) -> Entity<QuillShell> {
    let view = h
        .app
        .read_with(cx, |app, _| app.windows.view_for(OTHER))
        .expect("the chat window's view");
    view.read_with(cx, |view, _| view.shell.clone())
}

fn shown(shell: &Entity<QuillShell>, cx: &mut TestAppContext) -> Option<DialogKind> {
    shell.read_with(cx, |shell, _| shell.open_dialog)
}

/// The window whose state is in place asks; inside `as_asked_by` the
/// window the flow carried does; in the poll loop's pass nobody does.
#[gpui_kit::test]
fn the_asker_follows_the_flow_not_the_window_in_place(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        app.set_window_scope(Some(OTHER));
        assert_eq!(app.asking_window(), Some(Some(OTHER)));
        app.set_window_scope(None);
        assert_eq!(app.asking_window(), Some(None));
        app.as_asked_by(Some(OTHER), |app| {
            assert_eq!(app.asking_window(), Some(Some(OTHER)));
            app.unasked(|app| assert_eq!(app.asking_window(), None));
            assert_eq!(app.asking_window(), Some(Some(OTHER)), "put back");
        });
        assert_eq!(app.asking_window(), Some(None));
    });
}

/// Finding 5a: a link from the OS or a mini app, queued after the pointer
/// last crossed a chat window, is the main window's. One queued for an
/// answer carries the window its request named (5b).
#[gpui_kit::test]
fn a_queued_link_takes_the_asking_window_not_the_pointers(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        app.note_window_pointer(Some(OTHER));
        app.as_asked_by(None, |app| app.queue_deep_link("https://t.me/os".into()));
        let queued = |app: &crate::ui::app::QuillApp| {
            app.pending_deep_link
                .as_ref()
                .map(|q| (q.link.clone(), q.window))
        };
        assert_eq!(queued(app), Some(("https://t.me/os".to_string(), None)));
        // The poll loop runs with the main window in place; the answer
        // carries the chat window that pressed.
        app.set_window_scope(None);
        app.unasked(|app| {
            app.as_asked_by(Some(OTHER), |app| {
                app.queue_deep_link("https://t.me/answer".into());
            });
        });
        assert_eq!(
            queued(app),
            Some(("https://t.me/answer".to_string(), Some(OTHER)))
        );
        // A click in the chat window, with its state in place.
        app.note_window_press(Some(OTHER));
        app.queue_deep_link("https://t.me/click".into());
        assert_eq!(
            queued(app),
            Some(("https://t.me/click".to_string(), Some(OTHER)))
        );
    });
}

/// Findings 4 and 5: a box opened where no window asked (the poll loop's
/// pass, with the main window in place) shows in the window of the last
/// press, not in the main window.
#[gpui_kit::test]
fn a_box_no_window_asked_for_shows_where_the_user_pressed(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        app.note_window_press(Some(OTHER));
        app.set_window_scope(None);
        app.unasked(|app| {
            app.links.deep_link_dialog = Some("TDLib's text".into());
            app.open_box(DialogKind::DeepLinkInfo);
        });
    });
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), None);
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::DeepLinkInfo));
}

/// Finding 1: a typed link clicked in a chat window runs in that window's
/// own frame. The main window, minimized here, never draws.
#[gpui_kit::test]
fn a_link_clicked_in_a_chat_window_does_not_wait_for_the_main_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.set_window_scope(None);
        app.links.pending_deep_link_ui = Some(DeepLinkUi::ConferenceCall {
            link: "https://t.me/call/abc".into(),
        });
        app.hold_resolved_link(Some(OTHER), cx);
        assert_eq!(app.resolved_link_frame(), Some(OTHER));
    });
    for _ in 0..2 {
        draw(chat_window, cx);
    }
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::ConferenceLink));
    h.app.read_with(cx, |app, _| {
        assert!(app.links.pending_deep_link_ui.is_none(), "it ran once");
    });
}

/// Finding 1: the main window's frame does not run a link that belongs
/// to a chat window, and what opens in the main window (a settings page, a
/// chat with no window of its own) waits for the main frame.
#[gpui_kit::test]
fn a_resolved_link_runs_in_the_frame_it_belongs_to(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.set_window_scope(None);
        app.links.pending_deep_link_ui = Some(DeepLinkUi::ConferenceCall {
            link: "https://t.me/call/abc".into(),
        });
        app.hold_resolved_link(Some(OTHER), cx);
    });
    draw(h.main, cx);
    h.app.update(cx, |app, cx| {
        assert!(
            app.links.pending_deep_link_ui.is_some(),
            "the main frame leaves the chat window's link"
        );
        app.links.pending_deep_link_ui = Some(DeepLinkUi::Settings(SettingsTarget::Language));
        app.hold_resolved_link(Some(OTHER), cx);
        assert_eq!(
            app.resolved_link_frame(),
            None,
            "a settings page is the main window's"
        );
        app.links.pending_deep_link_ui = Some(DeepLinkUi::Settings(SettingsTarget::QrCode));
        assert_eq!(
            app.resolved_link_frame(),
            Some(OTHER),
            "a box shows where it was clicked"
        );
        app.links.pending_deep_link_ui = None;
        app.links.pending_deep_link_open = Some(crate::ui::links_state::PendingOpen::user(
            OPEN,
            quill::state::DeepLinkAction::ShareDraft { text: "hi".into() },
        ));
        assert_eq!(
            app.resolved_link_frame(),
            None,
            "a chat with no window of its own opens in the main window"
        );
        app.links.pending_deep_link_open = Some(crate::ui::links_state::PendingOpen::user(
            OTHER,
            quill::state::DeepLinkAction::ShareDraft { text: "hi".into() },
        ));
        assert_eq!(
            app.resolved_link_frame(),
            Some(OTHER),
            "a windowed chat goes there"
        );
        app.links.pending_deep_link_open = None;
    });
}

/// Finding 9: a share pick in a chat window opens as a link clicked
/// there: its frame runs it when the picked chat has a window.
#[gpui_kit::test]
fn a_share_pick_is_the_windows_that_picked(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.note_window_press(Some(OTHER));
        app.share.link_text = Some("https://example.com".into());
        app.choose_share_chat(OTHER, cx);
        assert_eq!(app.links.pending_flow_window, Some(OTHER));
        assert_eq!(app.resolved_link_frame(), Some(OTHER));
        app.links.pending_deep_link_open = None;
        app.links.pending_flow_window = None;
    });
}

/// Finding 4: the folder limit box opens where the box whose request hit
/// the limit is, not in the main window the poll runs in.
#[gpui_kit::test]
fn the_folder_limit_box_follows_the_box_that_hit_it(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.windows
            .open_box_in(DialogKind::FolderEditor, Some(OTHER));
        app.note_window_press(None);
        app.set_window_scope(None);
        if let Some(session) = app.demo_session.as_mut() {
            session.chat_list.folder_limit_hit =
                Some((quill::folder_limits::FolderLimitKind::Folders, None));
        }
        app.unasked(|app| assert!(app.drain_folder_limit(cx)));
    });
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), None);
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(shown(&in_chat_window, cx), Some(DialogKind::FolderLimit));
}

/// Finding 4: the "No ads" preview a refused Hide asks for opens in the
/// window showing the ad's chat, from that window's own frame.
#[gpui_kit::test]
fn the_no_ads_preview_opens_in_the_window_of_the_ad(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        app.set_window_scope(None);
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.hub.promo_request = Some(quill::premium_hub::PromoRequest {
                feature: quill::premium_hub::PremiumFeature::DisabledAds.into(),
                chat_id: OTHER,
            });
        }
    });
    draw(h.main, cx);
    h.app.read_with(cx, |app, _| {
        assert!(
            app.session()
                .is_some_and(|s| s.payments.hub.promo_request.is_some()),
            "the main frame leaves it for the ad's window"
        );
    });
    for _ in 0..2 {
        draw(chat_window, cx);
    }
    let in_chat_window = chat_window_shell(&h, cx);
    assert_eq!(
        shown(&in_chat_window, cx),
        Some(DialogKind::PremiumFeatures)
    );
    assert_eq!(shown(&h.shell, cx), None);
}

/// Finding 7: the camera circle shows in the window recording, not over
/// the other windows' chats.
#[gpui_kit::test]
fn the_camera_circle_shows_in_the_recording_window_only(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.set_window_scope(Some(OTHER));
        let _ = app.begin_capture_start(true, CaptureFor::Message);
        assert!(app.round_record_overlay(cx).is_some(), "its own window");
        app.set_window_scope(None);
        assert!(
            app.round_record_overlay(cx).is_none(),
            "not the main window"
        );
    });
}

/// Finding 9: the custom emoji card shows in the window of the tap.
#[gpui_kit::test]
fn the_custom_emoji_card_shows_where_the_emoji_was_tapped(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.set_window_scope(Some(OTHER));
        app.message_ui.custom_emoji_tapped = Some(77);
        if let Some(session) = app.demo_session.as_mut() {
            session.stickers.custom_emoji_preview = Some(quill::state::CustomEmojiPreview {
                emoji_id: 77,
                set_id: 5,
                title: "Pack".into(),
            });
        }
        assert!(app.custom_emoji_card(cx).is_some(), "the window of the tap");
        app.set_window_scope(None);
        assert!(app.custom_emoji_card(cx).is_none(), "not the main window");
    });
}

/// Finding 11: an auto-delete picker open in a chat window stays when the
/// main window enters another chat; leaving its own chat closes it.
#[gpui_kit::test]
fn the_auto_delete_picker_closes_with_its_own_chat_only(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.note_window_press(Some(OTHER));
        app.open_ttl_picker(OTHER, cx);
        app.note_window_press(None);
        app.enter_chat_ui(crate::ui::chat_window::tests::CHANNEL, cx);
        assert_eq!(
            app.notify.ttl_picker,
            Some(OTHER),
            "another window's chat change"
        );
        app.set_window_scope(Some(OTHER));
        app.close_ttl_picker_of_left_chat(None);
        assert_eq!(app.notify.ttl_picker, None, "its own chat left");
    });
}

/// Finding 11: a send being prepared holds its chat in every window, so
/// a chat moved into a window meanwhile can't send the same files again.
#[gpui_kit::test]
fn a_send_being_prepared_holds_its_chat_in_every_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        app.set_window_scope(Some(OTHER));
        let slot = app.send_slot().expect("the chat window's slot");
        let job = app.composer_sends.prep.begin(slot.clone());
        assert!(app.tray_preparing());
        app.set_window_scope(None);
        let main_slot = app.send_slot().expect("the main window's slot");
        let main_job = app.composer_sends.prep.begin(main_slot);
        assert_ne!(job, main_job, "one numbering for every window");
        // Its window closed: the job ends in the app's state, and only it.
        app.set_window_scope(None);
        assert_eq!(app.composer_sends.prep.finish(job.unwrap()), Some(slot));
        assert!(
            app.tray_preparing(),
            "the main window's send still prepares"
        );
        let _ = cx;
    });
}

/// Finding 11: the "group media" choice is the tray's, per chat.
#[gpui_kit::test]
fn the_group_media_choice_stays_with_its_chat(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        app.set_window_scope(None);
        let default = app.composer_group_media_effective();
        app.set_composer_group_media(Some(!default));
        assert_eq!(app.composer_group_media_effective(), !default);
        app.set_window_scope(Some(OTHER));
        assert_eq!(
            app.composer_group_media_effective(),
            default,
            "another chat"
        );
        app.set_window_scope(None);
        assert_eq!(
            app.composer_group_media_effective(),
            !default,
            "kept for its chat"
        );
    });
}

/// Finding 11: a chat window's GIF tab shows the results of its own
/// field, not the main window's search.
#[gpui_kit::test]
fn the_gif_tab_follows_its_windows_field(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stickers.gifs.search_mode = true;
            session.stickers.gifs.search_query = "cats".into();
        }
        app.set_window_scope(Some(OTHER));
        app.follow_gif_field(cx);
        assert!(
            app.session().is_some_and(|s| !s.stickers.gifs.search_mode),
            "an empty field shows the saved GIFs"
        );
    });
}

/// Round 15 (area B, finding 8): a footer emoji empties the field but
/// stays the window's section, so entering the GIF tab again keeps it.
/// And two windows with the tab open each show their own section: the
/// window pressed last asks for its own again, and the other shows its
/// own kept from before, without asking.
#[gpui_kit::test]
fn each_window_keeps_its_own_gif_section(cx: &mut TestAppContext) {
    use crate::ui::gifs_ui::GifSection;
    use crate::ui::media_panel::PanelTab;
    const EMOJI: &str = "\u{1F602}";
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        let searching = |app: &crate::ui::app::QuillApp| {
            app.session().and_then(|s| {
                let gifs = &s.stickers.gifs;
                gifs.search_mode.then(|| gifs.search_query.clone())
            })
        };
        app.choose_gif_section(GifSection::Emoji(EMOJI.into()), cx);
        app.follow_gif_field(cx);
        assert_eq!(
            searching(app).as_deref(),
            Some(EMOJI),
            "the emoji section stays"
        );
        app.set_media_panel_mode(crate::ui::media_panel::PanelMode::Compose);
        app.pickers.media_panel.tab = PanelTab::Gifs;
        // The chat window opens its GIF tab; the main window's stays open.
        app.note_window_press(Some(OTHER));
        app.set_media_panel_mode(crate::ui::media_panel::PanelMode::Compose);
        app.pickers.media_panel.tab = PanelTab::Gifs;
        app.gif_tab_upkeep(cx);
        assert_eq!(
            searching(app),
            None,
            "the chat window's tab shows its saved GIFs"
        );
        app.set_window_scope(None);
        let kept = app.session().is_some_and(|s| {
            matches!(
                app.gif_shown(&s.stickers.gifs),
                crate::ui::gifs_ui::GifShown::Found(_)
            )
        });
        assert!(kept, "the main window shows its emoji section's GIFs");
        app.gif_tab_upkeep(cx);
        assert_eq!(searching(app), None, "and leaves the chat window's list");
        // A press back in the main window asks for its emoji again.
        app.note_window_press(None);
        app.gif_tab_upkeep(cx);
        assert_eq!(searching(app).as_deref(), Some(EMOJI));
    });
}

/// Round 15 follow-up: a window whose GIF section was never answered
/// asks for it on its first frame with the tab open, without a press, and
/// only once; the window in use then takes the list back for paging and
/// the other keeps showing its answer.
#[gpui_kit::test]
fn a_gif_tab_shows_its_section_without_a_press(cx: &mut TestAppContext) {
    use crate::ui::gifs_ui::{GifShown, GifWanted};
    use crate::ui::media_panel::PanelTab;
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        let searching = |app: &crate::ui::app::QuillApp| {
            app.session().and_then(|s| {
                let gifs = &s.stickers.gifs;
                gifs.search_mode.then(|| gifs.search_query.clone())
            })
        };
        let shown_found = |app: &crate::ui::app::QuillApp| {
            app.session()
                .is_some_and(|s| matches!(app.gif_shown(&s.stickers.gifs), GifShown::Found(_)))
        };
        // The main window searched "dogs" and is in use.
        app.note_window_press(None);
        app.set_media_panel_mode(crate::ui::media_panel::PanelMode::Compose);
        app.pickers.media_panel.tab = PanelTab::Gifs;
        app.pickers.gif_wanted = GifWanted::Search("dogs".into());
        app.gif_tab_upkeep(cx);
        assert_eq!(searching(app).as_deref(), Some("dogs"));
        // The chat window's tab wants "cats", never answered.
        app.set_window_scope(Some(OTHER));
        app.set_media_panel_mode(crate::ui::media_panel::PanelMode::Compose);
        app.pickers.media_panel.tab = PanelTab::Gifs;
        app.pickers.gif_wanted = GifWanted::Search("cats".into());
        app.gif_tab_upkeep(cx);
        assert_eq!(
            searching(app).as_deref(),
            Some("cats"),
            "it asks without a press"
        );
        // The main window in use takes the list back.
        app.set_window_scope(None);
        app.gif_tab_upkeep(cx);
        assert_eq!(searching(app).as_deref(), Some("dogs"));
        // The chat window shows its answer and does not ask again.
        app.set_window_scope(Some(OTHER));
        assert!(shown_found(app), "the chat window shows its cats");
        app.gif_tab_upkeep(cx);
        assert_eq!(searching(app).as_deref(), Some("dogs"));
    });
}

/// Round 15 (area B, finding 4): work whose window is gone finds the
/// window that shows its target now: the main window for its chat, a
/// chat window for its own.
#[gpui_kit::test]
fn window_where_finds_the_window_showing_a_chat(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, cx| {
        let found = app.window_where(cx, |app| app.open_chat_id() == Some(OTHER));
        assert_eq!(found, Some((Some(OTHER), chat_window)));
        let found = app.window_where(cx, |app| app.open_chat_id() == Some(OPEN));
        assert_eq!(found.map(|(scope, _)| scope), Some(None));
        let found = app.window_where(cx, |app| app.open_chat_id() == Some(quill::ids::ChatId(-1)));
        assert_eq!(found, None);
        assert_eq!(app.windows.scope, None, "the scope in place is put back");
    });
}

/// A task started for the screen (`spawn_screen`) runs as the main window
/// but no window asked for it; one started for a window (`spawn_scoped`)
/// keeps the window it started in as its asker.
#[gpui_kit::test]
fn a_screen_task_asks_nobody_and_a_window_task_its_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    h.app.update(cx, |app, cx| {
        app.set_window_scope(Some(OTHER));
        let screen = seen.clone();
        app.spawn_screen(cx, async move |this, cx| {
            let _ = this.update(cx, |this, _| {
                screen.borrow_mut().push(("screen", this.asking_window()));
            });
        })
        .detach();
        let scoped = seen.clone();
        app.spawn_scoped(cx, async move |this, cx| {
            let _ = this.update(cx, |this, _| {
                scoped.borrow_mut().push(("scoped", this.asking_window()));
            });
        })
        .detach();
        app.set_window_scope(None);
    });
    cx.run_until_parked();
    let mut seen = seen.borrow().clone();
    seen.sort();
    assert_eq!(seen, vec![("scoped", Some(Some(OTHER))), ("screen", None)]);
}

/// Area C's QR finding: Share in a QR box shown in a chat window opens
/// the chat chooser there, though the card renders off the UI thread and
/// lands as the main window.
#[gpui_kit::test]
fn a_qr_share_opens_the_chooser_in_the_window_that_pressed(cx: &mut TestAppContext) {
    // `ready-peer-qr`: the own QR box can share; Maya's chat is 502.
    let maya = quill::ids::ChatId(502);
    let h = super::tests::new_demo_app(cx, "ready-peer-qr");
    let chat_window = h.open_in_new_window(maya, cx);
    let app = h.app.clone();
    cx.update_window(chat_window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.set_window_scope(Some(maya));
            app.note_window_press(Some(maya));
            app.open_own_qr_box(cx);
            app.qr_share(window, cx);
            app.set_window_scope(None);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(250));
    cx.run_until_parked();
    h.app.update(cx, |app, _| {
        app.set_window_scope(None);
        assert_eq!(app.share.link_text, None, "not the main window's chooser");
        app.set_window_scope(Some(maya));
        assert!(app.share.link_text.is_some(), "the chat window's chooser");
        app.set_window_scope(None);
    });
}

/// Round 17 (area B, finding 2): a menu-bar or Dock command runs as the
/// main window's, which it raises, even after a press in a chat window
/// left that window's state in place: Window > Add Contact shows its box
/// in main, not in the chat window now behind it.
#[gpui_kit::test]
fn a_menu_bar_command_runs_as_the_main_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app
        .update(cx, |app, _| app.note_window_press(Some(OTHER)));
    let app = h.app.clone();
    cx.update_window(h.main, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.as_main_window_command(window, cx, |app, window, cx| {
                assert_eq!(app.asking_window(), Some(None));
                app.open_new_contact_dialog(window, cx);
            });
            assert_eq!(app.asking_window(), Some(Some(OTHER)), "put back");
        })
    })
    .unwrap();
    draw_both(&h, chat_window, cx);
    assert_eq!(shown(&h.shell, cx), Some(DialogKind::AddContact));
    assert_eq!(shown(&chat_window_shell(&h, cx), cx), None);
}
