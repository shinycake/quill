//! Video userpics in a group of Premium members on scripted players: the
//! userpic beside each sender's messages and the private chat's row play
//! muted and looped, wait while the window is in the background or the
//! emoji panel is open, stop when the chat closes, sleep while no
//! longer drawn and come back, and keep the still with "Autoplay GIFs"
//! off.

use super::{Script, calls, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::take_video_targets;
use crate::ui::inline_players::RELEASE_AFTER;
use crate::ui::keybindings::bind_keys;
use crate::ui::media_panel::PanelTab;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::userpic_videos::Surface;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use std::cell::RefCell;
use std::rc::Rc;

/// The demo's first member (`demos/video_userpics.rs`), who also has a
/// private chat in the list.
const FIRST_MEMBER: i64 = 9601;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-group-video-userpics")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..4 {
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

/// One backend for every player the test may open, in any order.
fn scripts() -> Vec<Rc<RefCell<Script>>> {
    (0..40)
        .map(|_| {
            let script = Rc::new(RefCell::new(Script {
                ready: true,
                picture: true,
                ..Script::default()
            }));
            queue_backend(&script);
            script
        })
        .collect()
}

fn keys(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, surface: Surface) -> Vec<i64> {
    app.read_with(vcx, |app, _| {
        let mut users: Vec<i64> = app
            .playback
            .userpic_videos
            .keys()
            .into_iter()
            .filter(|(on, _)| *on == surface)
            .map(|(_, user)| user)
            .collect();
        users.sort_unstable();
        users
    })
}

fn playing(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, surface: Surface) -> Vec<i64> {
    let users = keys(app, vcx, surface);
    app.read_with(vcx, |app, _| {
        users
            .into_iter()
            .filter(|user| app.playback.userpic_videos.is_playing((surface, *user)))
            .collect()
    })
}

#[gpui_kit::test]
fn userpics_play_muted_beside_messages_and_in_the_chat_list(cx: &mut TestAppContext) {
    let scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);

    let senders = keys(&app, vcx, Surface::History);
    assert_eq!(senders.len(), 10, "one player per sender: {senders:?}");
    assert_eq!(playing(&app, vcx, Surface::History), senders);
    assert_eq!(keys(&app, vcx, Surface::ChatList), [FIRST_MEMBER]);
    assert_eq!(playing(&app, vcx, Surface::ChatList), [FIRST_MEMBER]);
    let opened: Vec<Vec<String>> = scripts
        .iter()
        .map(calls)
        .filter(|calls| !calls.is_empty())
        .collect();
    assert_eq!(opened.len(), 11, "no decoder beyond the userpics shown");
    for calls in &opened {
        assert!(calls.contains(&"volume 0".to_string()), "muted: {calls:?}");
        assert!(calls.iter().any(|c| c.starts_with("play")), "{calls:?}");
    }

    let (slice, layer) = app
        .read_with(vcx, |app, _| app.slices.conversation_ids())
        .expect("the conversation slice");
    take_video_targets();
    for script in &scripts {
        script.borrow_mut().picture = true;
    }
    settle(vcx);
    let targets = take_video_targets();
    assert!(targets.contains(&layer), "{targets:?} (layer {layer:?})");
    assert!(!targets.contains(&slice), "{targets:?} (slice {slice:?})");

    vcx.deactivate_window();
    settle(vcx);
    assert!(playing(&app, vcx, Surface::History).is_empty());
    assert!(playing(&app, vcx, Surface::ChatList).is_empty());
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    assert_eq!(playing(&app, vcx, Surface::History), senders);

    app.update_in(vcx, |app, _, cx| app.open_media_panel(PanelTab::Emoji, cx));
    settle(vcx);
    assert!(app.read_with(vcx, |app, _| app.media_panel_open()));
    assert!(playing(&app, vcx, Surface::History).is_empty());
    assert!(playing(&app, vcx, Surface::ChatList).is_empty());
    app.update_in(vcx, |app, _, cx| {
        app.close_media_panel(cx);
    });
    settle(vcx);
    assert_eq!(playing(&app, vcx, Surface::History), senders);

    let before: usize = scripts.iter().filter(|s| s.borrow().dropped).count();
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(ChatId(FIRST_MEMBER), window, cx)
    });
    settle(vcx);
    for _ in 0..2 {
        app.update_in(vcx, |app, _, cx| app.notify_conversation(cx));
        vcx.run_until_parked();
    }
    assert!(keys(&app, vcx, Surface::History).is_empty());
    let after: usize = scripts.iter().filter(|s| s.borrow().dropped).count();
    assert!(
        after >= before + 10,
        "the group's decoders went: {before} -> {after}"
    );
    assert_eq!(playing(&app, vcx, Surface::ChatList), [FIRST_MEMBER]);
}

#[gpui_kit::test]
fn with_autoplay_off_userpics_stay_still(cx: &mut TestAppContext) {
    let scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    assert_eq!(keys(&app, vcx, Surface::History).len(), 10);
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.media_prefs.autoplay_gifs = false;
        }
        cx.notify();
    });
    settle(vcx);
    assert!(keys(&app, vcx, Surface::History).is_empty());
    assert!(keys(&app, vcx, Surface::ChatList).is_empty());
    assert!(
        scripts
            .iter()
            .all(|s| s.borrow().dropped || s.borrow().calls.is_empty()),
        "every opened decoder went"
    );
}

#[gpui_kit::test]
fn a_userpic_no_longer_drawn_sleeps_and_comes_back(cx: &mut TestAppContext) {
    let scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    settle(vcx);
    let row = (Surface::ChatList, FIRST_MEMBER);
    assert_eq!(playing(&app, vcx, Surface::ChatList), [FIRST_MEMBER]);
    let opened = |scripts: &[Rc<RefCell<Script>>]| {
        scripts
            .iter()
            .filter(|s| !s.borrow().calls.is_empty() || s.borrow().dropped)
            .count()
    };
    let before = opened(&scripts);

    // The Contacts tab takes the column, so the chat list stops drawing.
    // (Not a hidden sidebar: that is saved to the process's shared
    // window state and would start every later test's app without one.)
    app.update_in(vcx, |app, _, cx| app.open_contacts_tab(cx));
    settle(vcx);
    for _ in 0..2 {
        vcx.executor().advance_clock(RELEASE_AFTER);
        vcx.run_until_parked();
    }
    let asleep = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.playback.userpic_videos.asleep(row))
    };
    assert_eq!(asleep(&app, vcx), Some(true), "asleep while not drawn");
    assert!(playing(&app, vcx, Surface::ChatList).is_empty());

    app.update_in(vcx, |app, _, cx| app.open_chats_tab(cx));
    settle(vcx);
    settle(vcx);
    assert_eq!(asleep(&app, vcx), Some(false));
    assert_eq!(playing(&app, vcx, Surface::ChatList), [FIRST_MEMBER]);
    assert!(opened(&scripts) > before, "a new decoder for the row");
}
