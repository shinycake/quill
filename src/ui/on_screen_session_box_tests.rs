//! Boxes whose open flag lives in the `Session` close on a switch. The
//! session goes to the background with the account and comes back with
//! it, so a box left open there would be open again on switch-back.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::boost_box::BoostBoxState;
use quill::calls::rtmp_start::RtmpStart;
use quill::collectible_info::{LookupSpec, LookupStage};
use quill::ids::{AccountKey, ChatId};
use quill::state::LanguageLinkLookup;

const CHAT: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

#[gpui_kit::test]
fn a_switch_closes_the_boxes_whose_flag_is_the_sessions(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(CHAT);
            session.stickers.masks.set_dialog_open(true);
            session.stickers.emoji.open = true;
            // The stickers manager, the sticker panel it opens and the GIF
            // gate are the session's too.
            session.stickers.stickers.open = true;
            session.stickers.manager.covers.show();
            session.stickers.gifs.open = true;
            session.calls.rtmp_start = Some(RtmpStart::new(CHAT, quill::ids::RequestId(1)));
            session.groups.boost_box = Some(BoostBoxState::new(CHAT.0, false));
            session.settings.language_link = Some(LanguageLinkLookup::looking_up(
                "de".into(),
                quill::ids::RequestId(1),
            ));
            session.payments.hub.collectible = Some(
                LookupSpec::new("durov", "Pavel", None, String::new())
                    .into_lookup(LookupStage::Loading(quill::ids::RequestId(1))),
            );
        }
        app.settings.set_box(
            crate::ui::settings_state::SettingsBox::ArchiveSettings,
            true,
        );
        app.open_location_picker(cx);
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.location_picker_open()));

    app.update_in(vcx, |app, window, cx| {
        app.switch_account(&AccountKey("other".into()), "", window, cx);
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(!app.location_picker_open(), "Send Location");
        assert!(
            !app.settings
                .box_open(crate::ui::settings_state::SettingsBox::ArchiveSettings),
            "Archive settings"
        );
        let session = app.session().expect("the demo session");
        assert!(!session.stickers.masks.dialog_open(), "Manage masks");
        assert!(!session.stickers.emoji.open, "Emoji sets");
        assert!(!session.stickers.stickers.open, "Sticker panel");
        assert!(!session.stickers.manager.covers.shown(), "Stickers manager");
        assert!(!session.stickers.gifs.open, "GIF gate");
        assert!(session.calls.rtmp_start.is_none(), "RTMP start");
        assert!(session.groups.boost_box.is_none(), "Boost box");
        assert!(session.settings.language_link.is_none(), "Language link");
        assert!(session.payments.hub.collectible.is_none(), "Collectible");
    });
}

/// The earn layer's flag is the session's info column, its back stack and
/// covered column the screen's: a switch closes it, so it does not return
/// without them.
#[gpui_kit::test]
fn a_switch_closes_the_earn_layer(cx: &mut TestAppContext) {
    use quill::earn::{EarnPanel, EarnPeer, EarnScreen};
    use quill::state::InfoPanelTarget;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    app.update_in(vcx, |app, window, cx| {
        // Opened from Settings (My Stars), with no chat whose leave would
        // take the column.
        if let Some(session) = app.demo_session.as_mut() {
            session.open = quill::state::OpenView::of(None);
        }
        let panel = EarnPanel::Revenue {
            peer: EarnPeer::User(11),
            screen: EarnScreen::Stars,
        };
        app.open_earn(panel, window, cx);
        assert!(app.earn_layer_open());
    });
    app.update_in(vcx, |app, window, cx| {
        app.switch_account(&AccountKey("other".into()), "", window, cx);
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        // The layer is not what the session shows on switch-back.
        let open = app
            .session()
            .and_then(|session| session.users_state.open_info_panel);
        assert!(!matches!(open, Some(InfoPanelTarget::Earn(_))));
        assert!(!app.earn_layer_open());
    });
}

/// A refused change's line belongs to the screen it was shown on: leaving
/// the page (or closing the box) ends it, so a reopen hours later shows a
/// clean screen instead of a stale refusal.
#[gpui_kit::test]
fn a_refused_changes_line_ends_with_its_screen(cx: &mut TestAppContext) {
    use quill::settings_catalog::Page;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let refuse = |app: &mut QuillApp| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.block_note.refused(None, "block refused");
            session
                .settings
                .sessions_note
                .refused(None, "terminate refused");
            session
                .settings
                .websites_note
                .refused(None, "disconnect refused");
            session.stickers.masks.set_dialog_open(true);
            session
                .stickers
                .masks
                .change_note
                .refused(None, "masks refused");
        }
    };
    let notes = |app: &QuillApp| {
        let session = app.demo_session.as_ref().expect("the demo session");
        [
            session.settings.block_note.line().is_some(),
            session.settings.sessions_note.line().is_some(),
            session.settings.websites_note.line().is_some(),
            session.stickers.masks.change_note.line().is_some(),
        ]
    };
    app.update_in(vcx, |app, _, _| refuse(app));
    app.read_with(vcx, |app, _| assert_eq!(notes(app), [true; 4]));
    // The blocked list's page.
    app.update_in(vcx, |app, _, _| app.leave_settings_page(Page::Privacy));
    app.read_with(vcx, |app, _| {
        assert_eq!(notes(app), [false, true, true, true])
    });
    // The Devices page.
    app.update_in(vcx, |app, _, _| app.leave_settings_page(Page::Devices));
    app.read_with(vcx, |app, _| {
        assert_eq!(notes(app), [false, false, true, true])
    });
    // The connected websites box.
    app.update_in(vcx, |app, _, cx| app.close_websites(cx));
    app.read_with(vcx, |app, _| {
        assert_eq!(notes(app), [false, false, false, true])
    });
    // The mask manager.
    app.update_in(vcx, |app, _, cx| app.close_mask_sets(cx));
    app.read_with(vcx, |app, _| assert_eq!(notes(app), [false; 4]));
    // Each screen reopens without its old line.
    app.update_in(vcx, |app, _, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stickers.masks.set_dialog_open(true);
        }
    });
    app.read_with(vcx, |app, _| assert_eq!(notes(app), [false; 4]));
}

/// The account's farewell closes the connected websites box too.
#[gpui_kit::test]
fn the_farewell_ends_the_websites_refusal(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    app.update_in(vcx, |app, _, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session
                .settings
                .websites_note
                .refused(None, "disconnect refused");
        }
    });
    app.update_in(vcx, |app, window, cx| app.farewell_screen(window, cx));
    app.read_with(vcx, |app, _| {
        let session = app.demo_session.as_ref().expect("the demo session");
        assert!(session.settings.websites_note.line().is_none());
    });
}

/// Sends a change the way the driver does (the request, then its note waits
/// for it), leaves the screen, feeds TDLib's real error envelope for that
/// request, and returns whether the note shows a line.
fn refusal_after_leaving(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    send: impl FnOnce(&mut quill::state::Session) -> quill::ids::RequestId,
    leave: impl FnOnce(&mut QuillApp, &mut gpui_kit::Context<QuillApp>),
    line: impl Fn(&quill::state::Session) -> Option<String>,
    reopen: impl FnOnce(&mut QuillApp, &mut gpui_kit::Context<QuillApp>),
) -> Option<String> {
    use quill::diagnostics::{DiagnosticSink, MemorySink};
    use quill::telegram::client::copy_and_parse;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;
    let mut request = None;
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            request = Some(send(session));
        }
    });
    app.update(vcx, |app, cx| leave(app, cx));
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let extra = request.expect("the change was sent").0;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let seq = AtomicU64::new(session.last_seq);
            let json =
                format!(r#"{{"@type":"error","@extra":"{extra}","code":400,"message":"BROKEN"}}"#);
            let owned = copy_and_parse(&json, &seq, &sink).expect("the error parses");
            session.apply(owned);
        }
        cx.notify();
    });
    app.update(vcx, |app, cx| reopen(app, cx));
    app.read_with(vcx, |app, _| app.demo_session.as_deref().and_then(&line))
}

#[gpui_kit::test]
fn a_block_refused_after_leaving_privacy_is_not_on_the_next_open(cx: &mut TestAppContext) {
    use quill::settings_catalog::Page;
    use quill::state::{RequestPurpose, SettingsPurpose};
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let after = refusal_after_leaving(
        &app,
        vcx,
        |session| {
            let id = session.request(
                RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList {
                    user_id: 61,
                    block: true,
                }),
                None,
            );
            session.settings.block_note.begin(id);
            id
        },
        |app, _| app.leave_settings_page(Page::Privacy),
        |session| session.settings.block_note.line().map(str::to_owned),
        |app, cx| app.arrive_settings_page(Page::Privacy, cx),
    );
    assert_eq!(after, None);
}

#[gpui_kit::test]
fn a_terminate_refused_after_leaving_devices_is_not_on_the_next_open(cx: &mut TestAppContext) {
    use quill::settings_catalog::Page;
    use quill::state::{RequestPurpose, SettingsPurpose};
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let after = refusal_after_leaving(
        &app,
        vcx,
        |session| {
            let id = session.request(
                RequestPurpose::Settings(SettingsPurpose::TerminateSession { session_id: 4 }),
                None,
            );
            session.settings.sessions_op.begin(id);
            session.settings.sessions_note.begin(id);
            id
        },
        |app, _| app.leave_settings_page(Page::Devices),
        |session| session.settings.sessions_note.line().map(str::to_owned),
        |app, cx| app.arrive_settings_page(Page::Devices, cx),
    );
    assert_eq!(after, None);
}

#[gpui_kit::test]
fn a_disconnect_refused_after_closing_the_websites_box_is_not_on_the_next_open(
    cx: &mut TestAppContext,
) {
    use quill::state::{RequestPurpose, SettingsPurpose};
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let after = refusal_after_leaving(
        &app,
        vcx,
        |session| {
            let id = session.request(
                RequestPurpose::Settings(SettingsPurpose::DisconnectWebsite { website_id: 3 }),
                None,
            );
            session.settings.websites_op.begin(id);
            session.settings.websites_note.begin(id);
            id
        },
        |app, cx| app.close_websites(cx),
        |session| session.settings.websites_note.line().map(str::to_owned),
        |app, cx| app.open_websites(cx),
    );
    assert_eq!(after, None);
}

#[gpui_kit::test]
fn a_mask_change_refused_after_closing_the_manager_is_not_on_the_next_open(
    cx: &mut TestAppContext,
) {
    use quill::state::StickersPurpose;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    app.update(vcx, |app, cx| app.open_mask_sets(cx));
    let after = refusal_after_leaving(
        &app,
        vcx,
        |session| {
            let id = session.request(StickersPurpose::ReorderInstalledMaskSets.into(), None);
            session
                .stickers
                .masks
                .begin_change(id, quill::state::MaskChangeTarget::Reorder);
            id
        },
        |app, cx| app.close_mask_sets(cx),
        |session| session.stickers.masks.change_note.line().map(str::to_owned),
        |app, cx| app.open_mask_sets(cx),
    );
    assert_eq!(after, None);
}
