//! The QR code box in the UI (needs the `demo-capture` feature for
//! gpui-kit's test support): it opens from each entry point, the colors
//! and emoji panels change the box, Esc closes a panel before the box,
//! and only eligible peers get a box.
use super::demo::{BACKDROPS, FRIEND, GROUP_CHAT, ME};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::deep_link_types::{DeepLinkUi, SettingsTarget};
use quill::peer_qr::appearance::SwatchKey;
use quill::peer_qr::color::Rgb;
use quill::peer_qr::state::{QrPanel, QrPeer, QrSubject};
use quill::settings_catalog::Page;
use quill::state::InfoPanelTarget;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-peer-qr")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    // Dialog entrances run on the wall clock; at rest the layout is the
    // same on every run.
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(250));
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn shown(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.debug_bounds(selector).is_some()
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

fn subject(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<QrSubject> {
    app.read_with(vcx, |app, _| {
        app.peer_qr
            .open
            .as_ref()
            .map(|open| open.state.subject.clone())
    })
}

fn close_box(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| app.close_qr_box(cx));
    frames(vcx);
    assert!(!shown(vcx, "peer-qr-box"));
}

#[gpui_kit::test]
fn the_demo_opens_your_own_box_with_colors_and_emoji(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    assert_eq!(
        shell.read_with(vcx, |shell, _| shell.open_dialog),
        Some(DialogKind::PeerQr)
    );
    assert_eq!(subject(&app, vcx), Some(QrSubject::Peer(QrPeer::User(ME))));
    for selector in [
        "peer-qr-close",
        "peer-qr-copy",
        "peer-qr-share",
        "peer-qr-colors",
        "peer-qr-emoji",
    ] {
        assert!(shown(vcx, selector), "{selector}");
    }
    let label = app.read_with(vcx, |app, _| app.qr_view().and_then(|view| view.label));
    assert_eq!(label.as_deref(), Some("@AVERYSTONE"));
}

#[gpui_kit::test]
fn a_color_circle_recolors_the_box_and_esc_closes_only_the_panel(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    click(vcx, "peer-qr-colors");
    let panel = app.read_with(vcx, |app, _| {
        app.peer_qr.open.as_ref().and_then(|open| open.state.panel)
    });
    assert_eq!(panel, Some(QrPanel::Colors));
    let (id, center, edge) = BACKDROPS[1];
    assert_eq!(id, 2);
    click(vcx, "peer-qr-swatch-2");
    app.read_with(vcx, |app, _| {
        let open = app.peer_qr.open.as_ref().expect("open");
        assert_eq!(open.state.color, SwatchKey::Backdrop(id));
        let view = app.qr_view().expect("view");
        assert_eq!(view.appearance.center, Rgb::hex(center));
        assert_eq!(view.appearance.edge, Rgb::hex(edge));
    });
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let open = app.peer_qr.open.as_ref().expect("the box stays");
        assert_eq!(open.state.panel, None);
        assert_eq!(open.state.color, SwatchKey::Backdrop(id));
    });
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert_eq!(subject(&app, vcx), None);
}

#[gpui_kit::test]
fn an_emoji_toggles_in_and_out_of_the_pattern(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    click(vcx, "peer-qr-emoji");
    click(vcx, "peer-qr-emoji-0");
    let first = quill::emoji_catalog::catalog()
        .next()
        .map(|entry| entry.emoji.to_string());
    let picks = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.peer_qr
                .open
                .as_ref()
                .map(|open| open.state.emoji.clone())
                .unwrap_or_default()
        })
    };
    assert_eq!(picks(&app, vcx), first.into_iter().collect::<Vec<_>>());
    click(vcx, "peer-qr-emoji-0");
    assert!(picks(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn the_settings_row_and_the_deep_link_open_your_own_box(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    close_box(&app, vcx);
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Account));
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("settings-row-qr-code", cx));
    frames(vcx);
    assert_eq!(subject(&app, vcx), Some(QrSubject::Peer(QrPeer::User(ME))));
    assert!(shown(vcx, "peer-qr-box"));
    close_box(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.run_deep_link_ui(DeepLinkUi::Settings(SettingsTarget::QrCode), window, cx);
    });
    frames(vcx);
    assert_eq!(subject(&app, vcx), Some(QrSubject::Peer(QrPeer::User(ME))));
}

#[gpui_kit::test]
fn profile_rows_open_a_user_or_group_box(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    close_box(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.open_info_panel_target(InfoPanelTarget::User(FRIEND), window, cx);
    });
    frames(vcx);
    click(vcx, "info-qr");
    assert_eq!(
        subject(&app, vcx),
        Some(QrSubject::Peer(QrPeer::User(FRIEND)))
    );
    // Another user's box has no colors or emoji.
    assert!(!shown(vcx, "peer-qr-colors"));
    let link = app.read_with(vcx, |app, _| {
        app.qr_view()
            .and_then(|view| view.shown)
            .map(|shown| shown.link)
    });
    assert_eq!(link.as_deref(), Some("https://t.me/mayachen"));
    close_box(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.open_info_panel_target(InfoPanelTarget::Supergroup(777), window, cx);
    });
    frames(vcx);
    click(vcx, "group-username-qr");
    assert_eq!(
        subject(&app, vcx),
        Some(QrSubject::Peer(QrPeer::Chat(GROUP_CHAT)))
    );
}

#[gpui_kit::test]
fn an_invite_link_opens_a_titled_square_box(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    close_box(&app, vcx);
    app.update(vcx, |app, cx| {
        app.open_invite_link_qr("https://t.me/+quillreaders", cx)
    });
    frames(vcx);
    assert!(shown(vcx, "peer-qr-box"));
    assert!(shown(vcx, "peer-qr-copy"));
    assert!(!shown(vcx, "peer-qr-colors"));
    app.read_with(vcx, |app, _| {
        let view = app.qr_view().expect("view");
        assert_eq!(view.title, Some(quill::lang::keys::GROUP_INVITE_QR_TITLE));
        assert_eq!(view.label, None);
        assert_eq!(
            view.shown.map(|shown| shown.link).as_deref(),
            Some("https://t.me/+quillreaders")
        );
    });
}

#[gpui_kit::test]
fn only_yourself_and_public_peers_get_a_box(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    close_box(&app, vcx);
    let nameless = 4_343;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.users.insert(
                nameless,
                quill::telegram::envelope::ParsedUser {
                    id: nameless,
                    first_name: "Dana".into(),
                    ..Default::default()
                },
            );
        }
        assert!(app.qr_eligible(QrPeer::User(ME)));
        assert!(app.qr_eligible(QrPeer::User(FRIEND)));
        assert!(app.qr_eligible(QrPeer::Chat(GROUP_CHAT)));
        assert!(!app.qr_eligible(QrPeer::User(nameless)));
        app.open_peer_qr_box(QrPeer::User(nameless), cx);
    });
    frames(vcx);
    assert_eq!(subject(&app, vcx), None);
}

#[gpui_kit::test]
fn copy_puts_the_card_image_on_the_clipboard(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    click(vcx, "peer-qr-copy");
    frames(vcx);
    let image = vcx.read_from_clipboard().and_then(|item| {
        item.into_entries().find_map(|entry| match entry {
            gpui_kit::ClipboardEntry::Image(image) => Some(image),
            _ => None,
        })
    });
    let image = image.expect("an image on the clipboard");
    assert_eq!(image.format, gpui_kit::ImageFormat::Png);
    assert!(image.bytes.starts_with(b"\x89PNG"));
    let toast = app.read_with(vcx, |app, _| {
        app.peer_qr.open.as_ref().and_then(|open| open.toast)
    });
    assert_eq!(
        toast.map(|(toast, _)| toast),
        Some(quill::peer_qr::state::QrToast::Copied)
    );
    assert!(shown(vcx, "peer-qr-toast"));
}

#[gpui_kit::test]
fn share_hides_the_box_under_the_chat_chooser_until_cancelled(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    click(vcx, "peer-qr-share");
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.share.link_text.as_deref(),
            Some("https://t.me/averystone")
        );
        let card = app
            .peer_qr
            .open
            .as_ref()
            .and_then(|open| open.share_image.clone());
        assert!(
            card.is_some_and(|path| path.exists()),
            "the card waits as a file"
        );
    });
    assert_eq!(
        shell.read_with(vcx, |shell, _| shell.open_dialog),
        Some(DialogKind::DeepLinkShare)
    );
    vcx.update(|window, cx| window.click("share-link-cancel", cx));
    frames(vcx);
    assert_eq!(
        shell.read_with(vcx, |shell, _| shell.open_dialog),
        Some(DialogKind::PeerQr)
    );
    app.read_with(vcx, |app, _| {
        let open = app.peer_qr.open.as_ref().expect("the box is back");
        assert_eq!(open.share_image, None);
    });
}
