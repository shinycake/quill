//! The player bar, a chat window's GIF and the gift boxes act in the window
//! that owns them: the track's chat scrolls in its own window, a main
//! window chat change leaves a chat window's GIF alone, and a gift box
//! hosted in a chat window goes on without the main window's frame. The
//! sticker set box, with one viewed set and one editing state, is open in
//! one window at a time, and its pending pick lives only while it is.
#![cfg(feature = "demo-capture")]

use super::tests::{Harness, OPEN, OTHER, OTHER_LAST, new_app};
use crate::ui::app::QuillApp;
use crate::ui::playback_state::{ChatMessage, PlayingGif};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext, Context, TestAppContext, Window};
use quill::ids::{ChatId, MessageId};

fn in_window<R>(
    h: &Harness,
    handle: AnyWindowHandle,
    chat: Option<ChatId>,
    cx: &mut TestAppContext,
    f: impl FnOnce(&mut QuillApp, &mut Window, &mut Context<QuillApp>) -> R,
) -> R {
    let app = h.app.clone();
    let result = cx
        .update_window(handle, |_, window, cx| {
            app.update(cx, |app, cx| {
                app.in_window_scope(chat, cx, |app, cx| f(app, window, cx))
            })
        })
        .unwrap()
        .expect("the window's state");
    cx.run_until_parked();
    result
}

/// The bar's title, clicked in the main window for a track of a chat shown
/// in its own window, leaves the main window's chat where it is: the
/// track's window takes the jump.
#[gpui_kit::test]
fn the_bar_title_jumps_in_the_tracks_own_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.playback.playing_voice = Some(OTHER_LAST);
        app.playback.player.chat = Some(OTHER);
        app.jump_to_playing(window, cx);
    });
    assert_eq!(
        h.open_chat(cx),
        Some(OPEN),
        "the main window keeps its chat"
    );
    let jump = |app: &mut QuillApp| app.session().map(|session| session.search.chat_search.jump);
    let main_jump = h
        .app
        .update(cx, |app, cx| {
            app.in_window_scope(None, cx, |app, _| jump(app))
        })
        .flatten();
    assert_eq!(
        main_jump,
        Some(quill::state::ChatSearchJump::None),
        "the main window's history does not reload around another chat's id"
    );
    let window_jump = h
        .app
        .update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, _| jump(app))
        })
        .flatten();
    assert_eq!(
        window_jump,
        Some(quill::state::ChatSearchJump::Ready {
            message_id: OTHER_LAST
        }),
        "the track's window scrolls to it"
    );
}

/// A GIF file looping in a chat window plays on when the main window
/// picks another chat; the main window's own GIF stops.
#[gpui_kit::test]
fn a_main_window_chat_change_leaves_a_chat_windows_gif(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    let gif = PlayingGif {
        window: Some(OTHER),
        message: ChatMessage::new(OTHER, MessageId(7)),
    };
    in_window(&h, chat_window, Some(OTHER), cx, |app, _, _| {
        app.playback.playing_animation = Some(gif);
        app.playback
            .autoplayed_gifs
            .insert(ChatMessage::new(OTHER, MessageId(7)));
    });
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.playback
            .autoplayed_gifs
            .insert(ChatMessage::new(OPEN, MessageId(3)));
        app.select_listed_chat(ChatId(13), window, cx);
    });
    h.app.read_with(cx, |app, _| {
        assert_eq!(app.playback.playing_animation, Some(gif), "it plays on");
        let marks = &app.playback.autoplayed_gifs;
        assert!(marks.contains(&ChatMessage::new(OTHER, MessageId(7))));
        assert!(
            !marks.contains(&ChatMessage::new(OPEN, MessageId(3))),
            "the left chat's GIFs may autoplay again"
        );
    });
    // The main window's own GIF stops with its chat.
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.playback.playing_animation = Some(PlayingGif {
            window: None,
            message: ChatMessage::new(ChatId(13), MessageId(5)),
        });
        app.select_listed_chat(OPEN, window, cx);
        assert_eq!(app.playback.playing_animation, None);
    });
}

/// A post's Statistics from a chat window: the main window, which draws
/// the panel, shows the post's page, not the channel overview its open
/// would put back after the page was set.
#[gpui_kit::test]
fn post_statistics_from_a_chat_window_shows_the_post(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, chat_window, Some(OTHER), cx, |app, window, cx| {
        app.open_message_statistics_panel(OTHER, MessageId(40), window, cx);
    });
    cx.run_until_parked();
    let page = h.app.read_with(cx, |app, _| {
        app.session()
            .map(|session| session.groups.stats.page(OTHER.0))
    });
    assert_eq!(page, Some(quill::state::StatsPage::Message(40)));
}

/// A gift box hosted in a chat window: its follow-ups run in that
/// window's frame, with the main window never drawn.
#[gpui_kit::test]
fn a_chat_windows_gift_box_goes_on_without_the_main_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        // The box was opened from the chat window, its last input.
        app.windows.last_press = Some(OTHER);
        app.windows
            .dialog_opened_in(crate::ui::shell::DialogKind::GiftMarket, Some(OTHER));
        app.gift_market.pending_auction = true;
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.gift_market.auction_ready = Some(5);
        }
    });
    cx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    h.app.read_with(cx, |app, _| {
        assert!(
            !app.gift_market.pending_auction,
            "the auction answer was taken"
        );
        let ready = app
            .demo_session
            .as_ref()
            .and_then(|s| s.payments.gift_market.auction_ready);
        assert_eq!(ready, None);
    });
}

/// A chat window shows a set; View Sticker Set in the main window closes
/// that box rather than switch it to the other set under its edit.
#[gpui_kit::test]
fn viewing_a_set_closes_the_box_in_the_other_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, chat_window, Some(OTHER), cx, |app, _, cx| {
        app.view_message_sticker_set(1, cx);
    });
    in_window(&h, h.main, None, cx, |app, _, cx| {
        app.view_message_sticker_set(2, cx);
    });
    let open_in = |chat: Option<ChatId>, cx: &mut TestAppContext| {
        h.app
            .update(cx, |app, cx| {
                app.in_window_scope(chat, cx, |app, _| app.message_ui.menu_ui.sticker_set_open)
            })
            .unwrap_or(false)
    };
    assert!(open_in(None, cx), "the main window shows the new set");
    assert!(!open_in(Some(OTHER), cx), "the chat window's box closed");
}

/// A video pick whose box closed (archived, removed) opens no editor
/// when its download lands.
#[gpui_kit::test]
fn a_closed_boxs_video_pick_opens_no_editor(cx: &mut TestAppContext) {
    use crate::ui::sticker_set_box::state::{PendingVideo, PickExisting};
    use quill::sticker_set_box::SetKind;
    let h = new_app(cx);
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.message_ui.menu_ui.sticker_set_open = false;
        app.sticker_box.pending_video = Some(PendingVideo {
            pick: PickExisting {
                set_id: 78,
                kind: SetKind::Stickers,
                adapt: false,
            },
            file_id: quill::ids::FileId(5),
            emoji: None,
            window: None,
        });
        app.sticker_box_frame_upkeep(window, cx);
        assert!(
            app.sticker_box.pending_video.is_none(),
            "the pick is dropped"
        );
        assert!(app.composer_ui.send_box.video_editor.is_none());
    });
}
