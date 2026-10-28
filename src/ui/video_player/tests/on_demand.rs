//! Autoplay off: the chat's clips rest on their stills until clicked. A
//! clicked round video (message 611) plays once with sound, downloaded
//! first when it is not local, and rests again when it ends; a clicked MP4
//! GIF loops muted on the inline player; a video labelled `image/gif` is
//! handed from the GIF decoder to the player too.

use super::{Script, calls, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::inline_players::Demand;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use gpui_kit::{Modifiers, ScrollDelta, ScrollWheelEvent, point};
use quill::ids::{ChatId, FileId, MessageId};
use quill::telegram::envelope::MessageContent;
use std::cell::RefCell;
use std::rc::Rc;

const ROUND: (i64, i64) = (11, 611);
/// The round video's clip (`apply_ready_video_note`).
const ROUND_FILE: i32 = 92;
const MP4_GIF: (i64, i64) = (11, 650);
const MP4_GIF_FILE: i32 = 66;
const LABELLED_GIF: (i64, i64) = (11, 651);
const LABELLED_GIF_FILE: i32 = 67;

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
                Some(ScreenshotDemo::named("ready-video-inline")),
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

fn settle(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    for _ in 0..4 {
        app.update_in(vcx, |app, _, cx| app.notify_conversation(cx));
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

/// Leave the chat and come back, so its history list picks up the
/// messages injected meanwhile, and scroll to the newest of them.
fn reopen_at_bottom(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let other = app
        .read_with(vcx, |app, _| {
            app.session()
                .and_then(|session| session.chats.keys().copied().find(|id| *id != 11))
        })
        .expect("a second demo chat");
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(ChatId(other), window, cx)
    });
    settle(app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(ChatId(11), window, cx)
    });
    settle(app, vcx);
    vcx.simulate_event(ScrollWheelEvent {
        position: point(px(800.), px(300.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-5000.))),
        modifiers: Modifiers::default(),
        ..Default::default()
    });
    settle(app, vcx);
}

fn queue_scripts(count: usize) -> Vec<Rc<RefCell<Script>>> {
    (0..count)
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

/// The demo chat with autoplay off for videos and GIFs, every inline
/// player gone.
fn autoplay_off(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.update(|window, _| window.activate_window());
    app.update_in(&mut vcx, |app, _, _| {
        let session = app.demo_session.as_mut().expect("the demo session");
        session.settings.media_prefs.autoplay_videos = false;
        session.settings.media_prefs.autoplay_gifs = false;
    });
    settle(&app, &mut vcx);
    (app, vcx)
}

fn set_local(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, file: i32, local: bool) {
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_mut().expect("the demo session");
        assert!(session.media.files.contains_key(&file), "the clip's file");
        session.media.files.update(file, |file| {
            file.local.is_downloading_completed = local;
            file.local.is_downloading_active = false;
        });
    });
}

/// What the poll loop does after a download lands.
fn land(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| app.resume_play_intent(cx));
    settle(app, vcx);
}

fn round_note(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> MessageContent {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.histories.get(&ROUND.0))
            .and_then(|history| history.messages.get(&ROUND.1))
            .map(|message| message.content.clone())
            .expect("the round video message")
    })
}

#[gpui_kit::test]
fn a_clicked_round_video_plays_with_sound_once_downloaded(cx: &mut TestAppContext) {
    let scripts = queue_scripts(16);
    let (app, mut vcx) = autoplay_off(cx);
    let vcx = &mut vcx;
    assert!(
        app.read_with(vcx, |app, _| app.playback.inline_videos.player(ROUND))
            .is_none(),
        "with autoplay off the round video rests on its still"
    );
    // Not downloaded yet: the click asks for the file and waits.
    set_local(&app, vcx, ROUND_FILE, false);
    let MessageContent::VideoNote(note) = round_note(&app, vcx) else {
        panic!("message 611 is a round video");
    };
    // What the circle's click and Space on the focused row both call.
    app.update_in(vcx, |app, _, cx| {
        app.play_video_note(ChatId(ROUND.0), MessageId(ROUND.1), &note, cx);
    });
    settle(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.playback.intent.as_ref().map(|intent| intent.file),
            Some(FileId(ROUND_FILE)),
            "the press waits for the clip"
        );
        assert!(app.playback.inline_videos.player(ROUND).is_none());
    });
    // The download lands: it plays from the start with sound.
    set_local(&app, vcx, ROUND_FILE, true);
    land(&app, vcx);
    let player = app
        .read_with(vcx, |app, _| {
            let videos = &app.playback.inline_videos;
            assert!(videos.is_sounding(ROUND), "plays with sound");
            videos.player(ROUND)
        })
        .expect("the round video's player");
    assert!(player.wants_play());
    let script = scripts
        .iter()
        .find(|script| {
            let calls = &script.borrow().calls;
            calls.iter().any(|call| call == "volume 1")
        })
        .expect("a backend turned up to full volume")
        .clone();
    let heard = calls(&script);
    assert!(heard.iter().any(|call| call == "seek 0"), "{heard:?}");
    assert!(
        heard.iter().any(|call| call.starts_with("play")),
        "{heard:?}"
    );
    // Played to its end: the demand is done and the still shows again.
    script.borrow_mut().at_end = true;
    drop(player);
    settle(&app, vcx);
    settle(&app, vcx);
    app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        assert_eq!(videos.demand(ROUND), None);
        assert!(!videos.is_sounding(ROUND));
        assert!(videos.player(ROUND).is_none(), "rests on its still");
    });
}

#[gpui_kit::test]
fn a_clicked_mp4_gif_loops_on_the_inline_player(cx: &mut TestAppContext) {
    let scripts = queue_scripts(16);
    let (app, mut vcx) = autoplay_off(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_mut().expect("the demo session");
        crate::ui::demos::video_fixtures::apply_clip_animation(
            session,
            &app.demo_ui.sink,
            &app.demo_ui.seq,
            (MP4_GIF.1, MP4_GIF_FILE),
            "video/mp4",
        );
    });
    reopen_at_bottom(&app, vcx);
    assert!(
        app.read_with(vcx, |app, _| app.playback.inline_videos.player(MP4_GIF))
            .is_none()
    );
    // The play disc.
    app.update_in(vcx, |app, _, cx| {
        app.toggle_animation_playback(
            MessageId(MP4_GIF.1),
            FileId(MP4_GIF_FILE),
            "video/mp4".into(),
            cx,
        );
    });
    settle(&app, vcx);
    let player = app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        assert_eq!(videos.demand(MP4_GIF), Some(Demand::Loop));
        assert!(!videos.is_sounding(MP4_GIF));
        // The MP4 never reaches the GIF decoder.
        assert!(app.playback.playing_animation.is_none());
        assert!(app.playback.gif_decodes.is_empty());
        videos.player(MP4_GIF)
    });
    let player = player.expect("an inline player for the MP4 GIF");
    assert!(player.wants_play(), "it loops in view");
    let looping = scripts
        .iter()
        .any(|script| script.borrow().calls.iter().any(|call| call == "volume 0"));
    assert!(looping, "muted");
}

#[gpui_kit::test]
fn a_video_labelled_image_gif_plays_on_the_inline_player(cx: &mut TestAppContext) {
    let _scripts = queue_scripts(16);
    let (app, mut vcx) = autoplay_off(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_mut().expect("the demo session");
        crate::ui::demos::video_fixtures::apply_clip_animation(
            session,
            &app.demo_ui.sink,
            &app.demo_ui.seq,
            (LABELLED_GIF.1, LABELLED_GIF_FILE),
            "image/gif",
        );
    });
    reopen_at_bottom(&app, vcx);
    app.update_in(vcx, |app, _, cx| {
        app.toggle_animation_playback(
            MessageId(LABELLED_GIF.1),
            FileId(LABELLED_GIF_FILE),
            "image/gif".into(),
            cx,
        );
    });
    // The decoder reads the header off the UI thread, finds no GIF and
    // hands the file over.
    settle(&app, vcx);
    app.read_with(vcx, |app, _| {
        let videos = &app.playback.inline_videos;
        assert!(app.playback.playing_animation.is_none());
        assert!(app.playback.gif.is_none());
        assert!(app.playback.gif_decodes.is_empty());
        assert_eq!(videos.demand(LABELLED_GIF), Some(Demand::Loop));
        assert!(videos.player(LABELLED_GIF).is_some());
    });
}

#[gpui_kit::test]
fn a_second_press_while_the_round_video_downloads_cancels_it(cx: &mut TestAppContext) {
    let _scripts = queue_scripts(16);
    let (app, mut vcx) = autoplay_off(cx);
    let vcx = &mut vcx;
    set_local(&app, vcx, ROUND_FILE, false);
    let MessageContent::VideoNote(note) = round_note(&app, vcx) else {
        panic!("message 611 is a round video");
    };
    let press = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update_in(vcx, |app, _, cx| {
            app.play_video_note(ChatId(ROUND.0), MessageId(ROUND.1), &note, cx);
        });
        settle(app, vcx);
    };
    press(&app, vcx);
    assert!(app.read_with(vcx, |app, _| app.playback.intent.is_some()));
    press(&app, vcx);
    assert!(
        app.read_with(vcx, |app, _| app.playback.intent.is_none()),
        "the second press cancels the download and the play"
    );
    // Landing anyway (it finished before the cancel): nothing starts.
    set_local(&app, vcx, ROUND_FILE, true);
    land(&app, vcx);
    assert!(
        app.read_with(vcx, |app, _| app.playback.inline_videos.player(ROUND))
            .is_none()
    );
}

#[gpui_kit::test]
fn one_press_on_a_voice_note_plays_it_once_downloaded(cx: &mut TestAppContext) {
    const VOICE: i64 = 91;
    const VOICE_FILE: i32 = 82;
    let (app, mut vcx) = autoplay_off(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_mut().expect("the demo session");
        crate::ui::audio_playback::apply_ready_voice(session, &app.demo_ui.sink, &app.demo_ui.seq);
    });
    set_local(&app, vcx, VOICE_FILE, false);
    let press = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update_in(vcx, |app, _, cx| {
            app.press_media(FileId(VOICE_FILE), cx, |app, cx| {
                app.toggle_voice_playback(
                    ChatId(11),
                    MessageId(VOICE),
                    FileId(VOICE_FILE),
                    true,
                    3.,
                    cx,
                );
            });
        });
        settle(app, vcx);
    };
    // Pressed twice: the download stops and nothing plays when it lands.
    press(&app, vcx);
    assert!(app.read_with(vcx, |app, _| app.playback.intent.is_some()));
    press(&app, vcx);
    set_local(&app, vcx, VOICE_FILE, true);
    land(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.playback.intent.is_none());
        assert_eq!(app.playback.playing_voice, None, "cancelled");
    });
    // Pressed once: it plays by itself once downloaded.
    set_local(&app, vcx, VOICE_FILE, false);
    press(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.playback.intent.is_some(), "parked for the download");
        assert_eq!(app.playback.playing_voice, None);
    });
    set_local(&app, vcx, VOICE_FILE, true);
    land(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.playback.intent.is_none());
        assert_eq!(
            app.playback.playing_voice,
            Some(MessageId(VOICE)),
            "plays by itself"
        );
    });
}
