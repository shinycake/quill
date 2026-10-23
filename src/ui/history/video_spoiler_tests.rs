//! Spoiler videos in the history (tdesktop `MediaSpoiler` over `Gif`): a
//! click lifts the cover for good, and from then on the video is plain
//! media that opens the viewer, alone or as an album tile.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, Pixels, Point, TestAppContext, VisualTestContext, px, size,
};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-albums");
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
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

/// A spoiler video message `id` in chat 11, in `album` when not 0.
fn spoiler_video(id: i32, album: i64) -> String {
    let file = crate::ui::demo::demo_file_json(990 + id, "", false);
    let album = if album == 0 {
        String::new()
    } else {
        format!(r#","media_album_id":"{album}""#)
    };
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":false,"date":1790640000{album},"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":7,"width":240,"height":160,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":null,"video":{file}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":true,"is_secret":false}}}}}}"#
    )
}

fn receive(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, updates: &[String]) {
    app.update(vcx, |app, cx| {
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = app.demo_ui.sink.clone();
        for json in updates {
            let parsed = quill::telegram::client::copy_and_parse(json, &app.demo_ui.seq, &sink);
            if let (Some(session), Some(parsed)) = (app.demo_session.as_mut(), parsed) {
                session.apply(parsed);
            }
        }
        cx.notify();
    });
    frames(vcx);
    app.update(vcx, |app, cx| app.jump_to_latest_messages(cx));
    frames(vcx);
}

fn centre(vcx: &mut VisualTestContext, id: (&'static str, u64)) -> Option<Point<Pixels>> {
    vcx.update(|window, _| window.try_find(id).map(|el| el.bounds().center()))
}

fn click(vcx: &mut VisualTestContext, at: Point<Pixels>) {
    vcx.simulate_click(at, Modifiers::default());
    frames(vcx);
}

fn viewer_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.viewer.state.is_open())
}

#[gpui_kit::test]
fn a_revealed_spoiler_video_shows_and_opens_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    receive(&app, vcx, &[spoiler_video(901, 0)]);
    assert!(
        vcx.debug_bounds("video-901").is_none(),
        "the video hides under its spoiler"
    );
    let cover = centre(vcx, ("spoiler-cover", 901)).expect("the spoiler covers the video");
    click(vcx, cover);
    assert!(!viewer_open(&app, vcx), "the first click only reveals");
    let video = vcx
        .debug_bounds("video-901")
        .expect("the revealed video renders as a plain video");
    click(vcx, video.center());
    assert!(
        viewer_open(&app, vcx),
        "a click on the revealed video opens the viewer"
    );
}

#[gpui_kit::test]
fn an_album_spoiler_tile_reveals_then_opens_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    receive(
        &app,
        vcx,
        &[spoiler_video(921, 99001), spoiler_video(922, 99001)],
    );
    let row = 921u64 * 64;
    let cover = centre(vcx, ("spoiler-cover", row)).expect("the album tile is covered");
    assert!(
        centre(vcx, ("album-disc", row)).is_none(),
        "no plain tile shows through the cover"
    );
    click(vcx, cover);
    assert!(!viewer_open(&app, vcx), "the first click only reveals");
    let tile = centre(vcx, ("album-tile", row)).expect("the tile stays");
    assert!(
        centre(vcx, ("album-disc", row)).is_some(),
        "the revealed tile shows its play disc"
    );
    click(vcx, tile);
    assert!(
        viewer_open(&app, vcx),
        "a click on the revealed tile opens the viewer"
    );
}
