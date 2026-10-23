#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! Media dragged out of a message onto the chat list (tdesktop
//! `dragItemByHandler` and album parts): pressing a photo, an album tile or a
//! sticker and moving picks that one message up like its time label does,
//! while a press that stays a click still opens the media.

use crate::ui::HistoryRow;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, MouseButton, Point, TestAppContext, VisualTestContext, point,
    px, size,
};
use quill::ids::{ChatId, MessageId};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext, demo: &str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn centre_of(vcx: &mut VisualTestContext, id: (&'static str, u64)) -> Point<gpui_kit::Pixels> {
    vcx.update(|window, _| window.find(id).bounds().center())
}

fn lit(vcx: &mut VisualTestContext, chat: u64) -> bool {
    vcx.update(|window, _| window.try_find(("drop-row-hot", chat)).is_some())
}

fn dragging(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|_, cx| cx.has_active_drag())
}

/// Receives one more message (id 901, incoming, chat 11) with `content` (the
/// JSON of a TDLib `MessageContent`) and returns a point on its media (low in the
/// row, clear of the hover toolbar).
fn inject(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    content: &str,
) -> Point<gpui_kit::Pixels> {
    app.update(vcx, |app, cx| {
        let present = app
            .session()
            .and_then(|s| s.histories.get(&11))
            .is_some_and(|h| h.messages.contains_key(&901));
        if present {
            return;
        }
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = app.demo_ui.sink.clone();
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":901,"chat_id":11,"is_outgoing":false,"date":1790640000,"content":{content}}}}}"#
        );
        let parsed = quill::telegram::client::copy_and_parse(&json, &app.demo_ui.seq, &sink);
        if let (Some(session), Some(parsed)) = (app.demo_session.as_mut(), parsed) {
            session.apply(parsed);
        }
        cx.notify();
    });
    frames(vcx);
    // A taller history (other fonts) leaves the new row below the fold:
    // scroll to it so its media is where the press lands.
    app.update(vcx, |app, cx| app.jump_to_latest_messages(cx));
    frames(vcx);
    // The incoming bubble's media sits at the row's left edge.
    let (row, at) = app.read_with(vcx, |app, _| {
        let hits = app.history.hit_rows.borrow();
        hits.iter()
            .find_map(|(ix, bounds)| match app.history.rows.get(*ix) {
                Some(HistoryRow::Single(inputs)) if inputs.message.id == MessageId(901) => Some((
                    *bounds,
                    point(bounds.left() + px(90.), bounds.bottom() - px(35.)),
                )),
                _ => None,
            })
            .expect("the injected row is painted")
    });
    // The composer takes the bottom of the window.
    let window = vcx.update(|window, _| window.viewport_size());
    assert!(
        at.y > px(60.) && at.y < window.height - px(120.),
        "the press point {at:?} (row {row:?}) is off the history of a {window:?} window"
    );
    at
}

fn file() -> String {
    crate::ui::demo::demo_file_json(990, "", false)
}

fn photo_content() -> String {
    let file = file();
    format!(
        r#"{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}"#
    )
}

fn sticker_content() -> String {
    let file = file();
    format!(
        r#"{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"990","set_id":"77","width":128,"height":128,"emoji":"x","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{file}}}}}"#
    )
}

fn video_note_content() -> String {
    let file = file();
    format!(
        r#"{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":5,"waveform":"","length":200,"minithumbnail":null,"thumbnail":null,"speech_recognition_result":null,"video":{file}}},"is_viewed":true,"is_secret":false}}"#
    )
}

fn document_content() -> String {
    let file = file();
    format!(
        r#"{{"@type":"messageDocument","document":{{"@type":"document","file_name":"report.pdf","mime_type":"application/pdf","minithumbnail":null,"thumbnail":null,"document":{file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#
    )
}

fn video_content() -> String {
    let file = file();
    format!(
        r#"{{"@type":"messageVideo","video":{{"@type":"video","duration":7,"width":240,"height":160,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":null,"video":{file}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#
    )
}

fn paid_content() -> String {
    r#"{"@type":"messagePaidMedia","star_count":5,"media":[{"@type":"paidMediaPreview","width":240,"height":160,"duration":0,"minithumbnail":null}],"caption":{"@type":"formattedText","text":"","entities":[]},"show_caption_above_media":false}"#.to_string()
}

fn photo(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Point<gpui_kit::Pixels> {
    let fallback = inject(app, vcx, &photo_content());
    vcx.debug_bounds("photo-901").map_or(fallback, |frame| {
        point(frame.center().x, frame.bottom() - px(30.))
    })
}

fn viewer_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.viewer.state.is_open())
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat))
}

/// A point on the video tile of the own album (message 812 of chat 11):
/// the right half of the mosaic at the top of that row.
fn tile(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Point<gpui_kit::Pixels> {
    app.read_with(vcx, |app, _| {
        let hits = app.history.hit_rows.borrow();
        hits.iter()
            .find_map(|(ix, bounds)| match app.history.rows.get(*ix) {
                Some(HistoryRow::Album {
                    album_id: 77002, ..
                }) => Some(point(bounds.right() - px(100.), bounds.top() + px(60.))),
                _ => None,
            })
            .expect("the own album is painted")
    })
}

/// Presses at `from` and moves to `to` in steps, button down.
fn press_and_move(
    vcx: &mut VisualTestContext,
    from: Point<gpui_kit::Pixels>,
    to: Point<gpui_kit::Pixels>,
) {
    let none = Modifiers::default();
    vcx.simulate_mouse_move(from, None, none);
    vcx.simulate_mouse_down(from, MouseButton::Left, none);
    for step in 1..=4 {
        let t = step as f32 / 4.;
        let at = point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        vcx.simulate_mouse_move(at, Some(MouseButton::Left), none);
        frames(vcx);
    }
}

fn forward_bar(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<MessageId> {
    app.read_with(vcx, |app, _| {
        app.share
            .pending_forward
            .as_ref()
            .map(|draft| draft.message_ids.clone())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn an_album_tile_pressed_and_moved_lifts_that_one_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = tile(&app, vcx);
    let to = centre_of(vcx, ("chat-row", 12));
    press_and_move(vcx, from, to);
    assert!(dragging(vcx), "the press on the tile lifted a drag");
    assert!(lit(vcx, 12), "the row under the pointer lights up");
}

#[gpui_kit::test]
fn dropping_an_album_tile_forwards_the_tile_not_the_album(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = tile(&app, vcx);
    let to = centre_of(vcx, ("chat-row", 12));
    press_and_move(vcx, from, to);
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    frames(vcx);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(12)));
    assert_eq!(forward_bar(&app, vcx), vec![MessageId(812)]);
    assert!(!viewer_open(&app, vcx), "a drop is not a click on the tile");
}

#[gpui_kit::test]
fn a_tile_clicked_with_a_small_wiggle_still_opens_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = tile(&app, vcx);
    let near = point(from.x + px(4.), from.y);
    press_and_move(vcx, from, near);
    vcx.simulate_mouse_up(near, MouseButton::Left, Modifiers::default());
    frames(vcx);
    assert!(viewer_open(&app, vcx), "under 10 px is still a click");
    assert!(!dragging(vcx));
}

#[gpui_kit::test]
fn a_plain_click_on_a_tile_opens_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = tile(&app, vcx);
    vcx.simulate_click(from, Modifiers::default());
    frames(vcx);
    assert!(viewer_open(&app, vcx));
}

#[gpui_kit::test]
fn a_drag_released_back_on_its_tile_is_not_a_click(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = tile(&app, vcx);
    let away = point(from.x + px(60.), from.y);
    press_and_move(vcx, from, away);
    let none = Modifiers::default();
    vcx.simulate_mouse_move(from, Some(MouseButton::Left), none);
    vcx.simulate_mouse_up(from, MouseButton::Left, none);
    frames(vcx);
    assert!(!viewer_open(&app, vcx));
}

#[gpui_kit::test]
fn a_protected_chat_never_lifts_its_media(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.chats_state.protected_chats.insert(11);
        }
    });
    frames(vcx);
    let from = tile(&app, vcx);
    let to = centre_of(vcx, ("chat-row", 12));
    press_and_move(vcx, from, to);
    assert!(!dragging(vcx));
    assert!(!lit(vcx, 12));
}

#[gpui_kit::test]
fn a_photo_in_a_bubble_lifts_its_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = photo(&app, vcx);
    let to = centre_of(vcx, ("chat-row", 12));
    press_and_move(vcx, from, to);
    assert!(dragging(vcx));
    assert!(lit(vcx, 12));
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    frames(vcx);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(12)));
    assert_eq!(forward_bar(&app, vcx), vec![MessageId(901)]);
}

#[gpui_kit::test]
fn a_click_on_a_photo_still_opens_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let at = photo(&app, vcx);
    vcx.simulate_click(at, Modifiers::default());
    frames(vcx);
    assert!(viewer_open(&app, vcx));
}

#[gpui_kit::test]
fn a_pending_photo_does_not_lift(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    photo(&app, vcx);
    app.update(vcx, |app, _| {
        if let Some(message) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.histories.get_mut(&11))
            .and_then(|h| h.messages.get_mut(&901))
        {
            message.pending = true;
        }
    });
    frames(vcx);
    let from = photo(&app, vcx);
    let to = centre_of(vcx, ("chat-row", 12));
    press_and_move(vcx, from, to);
    assert!(!dragging(vcx));
}

/// Lifting `content` from its row and dropping it on chat 12 forwards message 901.
fn lifts_and_forwards(cx: &mut TestAppContext, content: String) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = inject(&app, vcx, &content);
    let to = centre_of(vcx, ("chat-row", 12));
    press_and_move(vcx, from, to);
    assert!(dragging(vcx), "the press lifted a drag");
    assert!(lit(vcx, 12));
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    frames(vcx);
    assert_eq!(forward_bar(&app, vcx), vec![MessageId(901)]);
}

#[gpui_kit::test]
fn a_sticker_lifts_its_message(cx: &mut TestAppContext) {
    lifts_and_forwards(cx, sticker_content());
}

#[gpui_kit::test]
fn a_document_lifts_its_message(cx: &mut TestAppContext) {
    lifts_and_forwards(cx, document_content());
}

#[gpui_kit::test]
fn a_round_video_lifts_its_message(cx: &mut TestAppContext) {
    lifts_and_forwards(cx, video_note_content());
}

#[gpui_kit::test]
fn a_single_video_lifts_its_message(cx: &mut TestAppContext) {
    lifts_and_forwards(cx, video_content());
}

#[gpui_kit::test]
fn a_paid_media_card_does_not_lift(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = inject(&app, vcx, &paid_content());
    let to = centre_of(vcx, ("chat-row", 12));
    press_and_move(vcx, from, to);
    assert!(!dragging(vcx), "a locked card is not a drag source");
}

/// A point on the play disc of the injected video (message 901): a press
/// the disc swallowed would not lift the video.
fn video_disc(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Point<gpui_kit::Pixels> {
    let fallback = inject(app, vcx, &video_content());
    vcx.debug_bounds("video-901")
        .map_or(fallback, |frame| frame.center())
}

#[gpui_kit::test]
fn a_video_lifts_from_its_centre_disc(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = video_disc(&app, vcx);
    let to = centre_of(vcx, ("chat-row", 12));
    press_and_move(vcx, from, to);
    assert!(dragging(vcx), "the press on the disc lifted a drag");
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    frames(vcx);
    assert_eq!(forward_bar(&app, vcx), vec![MessageId(901)]);
    assert!(!viewer_open(&app, vcx));
}

#[gpui_kit::test]
fn a_click_on_the_video_disc_is_not_a_drag(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let at = video_disc(&app, vcx);
    vcx.simulate_click(at, Modifiers::default());
    frames(vcx);
    assert!(!dragging(vcx));
    assert!(forward_bar(&app, vcx).is_empty());
}

#[gpui_kit::test]
fn a_lifted_photo_released_over_the_history_does_not_reply(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let from = photo(&app, vcx);
    // Far to the right of the press: a swipe to reply goes left, so go
    // left over the history instead.
    let to = point(from.x - px(60.), from.y + px(2.));
    press_and_move(vcx, from, to);
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    frames(vcx);
    let replying = app.read_with(vcx, |app, _| app.composer_ui.reply().is_some());
    assert!(!replying, "the release ended a drag, not a swipe to reply");
}

#[gpui_kit::test]
fn an_album_video_tile_centres_its_play_disc_inside_the_tile(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx, "ready-albums");
    let vcx = &mut vcx;
    let row = 812u64 * 64;
    let (tile, play) = vcx.update(|window, _| {
        (
            window.find(("album-tile", row)).bounds(),
            window.find(("album-disc", row)).bounds(),
        )
    });
    assert!(
        tile.contains(&play.origin) && tile.contains(&play.bottom_right()),
        "the play disc {play:?} sits inside the tile {tile:?}"
    );
    let (t, p) = (tile.center(), play.center());
    assert!(
        (t.x - p.x).abs() <= px(1.) && (t.y - p.y).abs() <= px(1.),
        "the play disc is centred: {p:?} vs {t:?}"
    );
}
