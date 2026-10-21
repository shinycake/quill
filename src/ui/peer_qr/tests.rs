//! The QR code box in the UI (needs the `demo-capture` feature for
//! gpui-kit's test support): it opens from each entry point, the colors
//! and emoji panels change the box, Esc closes a panel before the box,
//! only eligible peers get a box, and the clock only runs for a
//! countdown.
use super::demo::{BACKDROPS, FRIEND, GROUP_CHAT, ME};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, ClipboardItem, Entity, Modifiers, TestAppContext, VisualTestContext, px, size,
};
use quill::deep_link_types::{DeepLinkUi, SettingsTarget};
use quill::peer_qr::appearance::SwatchKey;
use quill::peer_qr::color::Rgb;
use quill::peer_qr::free::{FreeCode, FreeMode};
use quill::peer_qr::layout::{BOX_WIDTH, PANEL_MARGIN, SWATCH_SKIP, swatch_size};
use quill::peer_qr::link::{UserLinkData, UserLinkFetch};
use quill::peer_qr::state::{QrPanel, QrPeer, QrSubject, QrToast};
use quill::privacy::{NewChatPrivacy, NewChatPrivacyState, PrivacyKeyState, PrivacyRuleDetail};
use quill::settings_catalog::Page;
use quill::state::InfoPanelTarget;
use quill::telegram::envelope::ChannelMemberStatus;
use quill::telegram::requests::PrivacyWho;
use quill::telegram::requests_privacy::PrivacySettingKey;

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
    // With reduced motion the dialog is at rest; the test clock lets
    // anything waiting on a timer run.
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(250));
    vcx.run_until_parked();
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

fn toast(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<QrToast> {
    app.read_with(vcx, |app, _| {
        app.peer_qr
            .open
            .as_ref()
            .and_then(|open| open.toast.clone())
            .map(|(toast, _)| toast)
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
                .map(|open| super::plain_picks(&open.state.emoji))
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
    vcx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("before".into())));
    click(vcx, "info-qr");
    assert_eq!(
        subject(&app, vcx),
        Some(QrSubject::Peer(QrPeer::User(FRIEND)))
    );
    let clipboard = vcx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(
        clipboard.as_deref(),
        Some("before"),
        "the row under the button did not copy"
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
    assert!(shown(vcx, "peer-qr-colors"), "the creator styles the box");
}

#[gpui_kit::test]
fn a_member_who_cannot_change_info_gets_no_colors(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    close_box(&app, vcx);
    app.update(vcx, |app, cx| {
        if let Some(chat) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.chats.get_mut(&GROUP_CHAT))
        {
            chat.my_member_status = Some(ChannelMemberStatus::Member);
        }
        app.open_peer_qr_box(QrPeer::Chat(GROUP_CHAT), cx);
    });
    frames(vcx);
    assert!(shown(vcx, "peer-qr-box"));
    assert!(!shown(vcx, "peer-qr-colors"));
    assert!(!shown(vcx, "peer-qr-emoji"));
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
fn copy_puts_the_card_on_the_clipboard(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    click(vcx, "peer-qr-copy");
    frames(vcx);
    let item = vcx
        .read_from_clipboard()
        .expect("something on the clipboard");
    let image = item.entries().iter().find_map(|entry| match entry {
        gpui_kit::ClipboardEntry::Image(image) => Some(image.clone()),
        _ => None,
    });
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        let image = image.expect("the card image");
        assert_eq!(image.format, gpui_kit::ImageFormat::Png);
        assert!(image.bytes.starts_with(b"\x89PNG"));
        assert_eq!(toast(&app, vcx), Some(QrToast::Copied));
    } else {
        assert_eq!(item.text().as_deref(), Some("https://t.me/averystone"));
        assert_eq!(toast(&app, vcx), Some(QrToast::LinkCopied));
    }
    assert!(shown(vcx, "peer-qr-toast"));
}

#[gpui_kit::test]
fn color_rows_are_one_circle_and_gap_apart(cx: &mut TestAppContext) {
    let (_, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    click(vcx, "peer-qr-colors");
    let first = vcx
        .debug_bounds("peer-qr-swatch--1")
        .expect("the default circle");
    let wrapped = vcx
        .debug_bounds("peer-qr-swatch-6")
        .expect("the seventh circle, on the second row");
    let circle = swatch_size(BOX_WIDTH - 2. * PANEL_MARGIN) as f32;
    assert!((47. ..=48.5).contains(&circle), "{circle}");
    assert!((f32::from(first.size.width) - circle).abs() < 1.);
    let pitch = f32::from(wrapped.origin.y - first.origin.y);
    assert!(
        (pitch - (circle + SWATCH_SKIP as f32)).abs() < 1.,
        "rows {pitch} apart"
    );
    assert!(
        f32::from(wrapped.origin.x - first.origin.x).abs() < 1.,
        "rows start at the left"
    );
}

#[gpui_kit::test]
fn a_failed_contact_link_is_shown_and_asked_again_on_reopen(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    close_box(&app, vcx);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            if let Some(me) = session.users.get_mut(&ME) {
                me.username.clear();
                me.active_usernames.clear();
            }
            let rule = |who| {
                PrivacyKeyState::Ready(PrivacyRuleDetail {
                    who: Some(who),
                    ..Default::default()
                })
            };
            let privacy = &mut session.settings.privacy;
            privacy.insert(PrivacySettingKey::ShowPhoneNumber, rule(PrivacyWho::Nobody));
            privacy.insert(
                PrivacySettingKey::AllowFindingByPhoneNumber,
                rule(PrivacyWho::Contacts),
            );
        }
        app.open_own_qr_box(cx);
    });
    frames(vcx);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session
                .users_state
                .peer_qr
                .fail_link(None, "Too many requests".into());
        }
        app.drain_peer_qr(cx);
    });
    frames(vcx);
    assert_eq!(
        toast(&app, vcx),
        Some(QrToast::Failed("Too many requests".into()))
    );
    assert!(shown(vcx, "peer-qr-toast"));
    close_box(&app, vcx);
    app.update(vcx, |app, cx| app.open_own_qr_box(cx));
    frames(vcx);
    let contact = app.read_with(vcx, |app, _| {
        app.session()
            .map(|session| session.users_state.peer_qr.contact_link.clone())
    });
    assert_eq!(contact, Some(UserLinkFetch::Idle), "reopening asks again");
}

#[gpui_kit::test]
fn the_clock_runs_only_while_a_countdown_shows(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let running = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.peer_qr
                .open
                .as_ref()
                .is_some_and(|open| open.clock_running)
        })
    };
    assert!(!running(&app, vcx), "a username link never runs out");
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.privacy_data.new_chat =
                Some(NewChatPrivacyState::Ready(NewChatPrivacy {
                    allow_from_unknown: true,
                    incoming_paid_message_star_count: 50,
                }));
        }
        if let Some(open) = app.peer_qr.open.as_mut() {
            open.state.free.mode = FreeMode::Temporary {
                code: FreeCode {
                    link: UserLinkData {
                        url: "https://t.me/+free".into(),
                        expires_at: quill::local_time::now_unix() - 1,
                    },
                    can_permanent: true,
                },
            };
        }
        app.sync_qr_clock(cx);
    });
    assert!(running(&app, vcx), "a free code counts down");
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(1_100));
    vcx.run_until_parked();
    assert!(!running(&app, vcx), "the clock stops once the code ran out");
    assert_eq!(toast(&app, vcx), Some(QrToast::CodeExpired));
}

/// A message price and a temporary free code, then the pill's menu.
fn open_free_menu(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.privacy_data.new_chat =
                Some(NewChatPrivacyState::Ready(NewChatPrivacy {
                    allow_from_unknown: true,
                    incoming_paid_message_star_count: 50,
                }));
        }
        if let Some(open) = app.peer_qr.open.as_mut() {
            open.state.free.mode = FreeMode::Temporary {
                code: FreeCode {
                    link: UserLinkData {
                        url: "https://t.me/+free".into(),
                        expires_at: quill::local_time::now_unix() + 3_600,
                    },
                    can_permanent: true,
                },
            };
        }
        cx.notify();
    });
    frames(vcx);
    click(vcx, "peer-qr-pill");
}

#[gpui_kit::test]
fn the_pill_menu_drops_from_the_pill_top(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    open_free_menu(&app, vcx);
    let pill = vcx.debug_bounds("peer-qr-pill").expect("the pill");
    let menu = vcx.debug_bounds("peer-qr-menu").expect("the open menu");
    assert!(
        (f32::from(menu.origin.y) - f32::from(pill.origin.y)).abs() < 1.,
        "menu at {:?}, pill at {:?}",
        menu.origin,
        pill.origin
    );
    let centers = |bounds: gpui_kit::Bounds<gpui_kit::Pixels>| {
        f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.
    };
    assert!(
        (centers(menu) - centers(pill)).abs() < 1.,
        "centered on the pill"
    );
    // The menu reaches over the Colors button; a press there picks the
    // menu's timer entry, not Colors under it.
    let colors = vcx
        .debug_bounds("peer-qr-colors")
        .expect("the colors button");
    assert!(menu.contains(&colors.center()));
    vcx.simulate_click(colors.center(), Modifiers::default());
    frames(vcx);
    assert!(!shown(vcx, "peer-qr-colors-grid"), "Colors stayed shut");
    // The demo has no client, so the timer request comes back unavailable;
    // a press that missed the menu would only have dismissed it.
    assert_eq!(toast(&app, vcx), Some(QrToast::Unavailable));
}

#[gpui_kit::test]
fn a_press_off_the_menu_closes_only_the_menu(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    open_free_menu(&app, vcx);
    assert!(shown(vcx, "peer-qr-menu"));
    let qr = vcx.debug_bounds("peer-qr-box").expect("the box");
    // Left of the box, over the dialog's backdrop.
    let beside = gpui_kit::point(qr.origin.x - px(40.), qr.center().y);
    vcx.simulate_click(beside, Modifiers::default());
    frames(vcx);
    assert!(!shown(vcx, "peer-qr-menu"), "the menu closed");
    assert!(shown(vcx, "peer-qr-box"), "the box stayed");
}

#[gpui_kit::test]
fn a_share_finishing_in_a_reopened_box_is_dropped(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    // The card renders off the UI thread; before it is back, the same
    // box closes and opens again.
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.qr_share(window, cx);
            app.close_qr_box(cx);
            app.open_own_qr_box(cx);
        })
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.share.link_text, None, "the new box shared nothing");
        let open = app.peer_qr.open.as_ref().expect("the new box");
        assert_eq!(open.share_image, None);
    });
}

#[gpui_kit::test]
fn a_card_whose_chat_did_not_open_is_not_attached_later(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    frames(vcx);
    let card = std::env::temp_dir().join(format!("quill-qr-stale-{}.png", std::process::id()));
    std::fs::write(&card, b"card").expect("a card file");
    // The shared card waits for chat 424242, but the next chat a link
    // opens is another one.
    app.update(vcx, |app, cx| {
        app.peer_qr.pending_attach = Some((quill::ids::ChatId(424_242), card.clone()));
        app.links.pending_deep_link_open = Some((
            quill::ids::ChatId(GROUP_CHAT),
            quill::state::DeepLinkAction::ShareDraft { text: "hi".into() },
        ));
        cx.notify();
    });
    frames(vcx);
    let pending = app.read_with(vcx, |app, _| app.peer_qr.pending_attach.clone());
    assert_eq!(pending, None);
    assert!(!card.exists(), "the card file is gone");
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

mod theme;
