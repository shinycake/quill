use super::super::{
    WindowDraw, begin_chat_frame, begin_frame, begin_patch_frame, forget_window, frame,
    next_image_id, slice_rendered, slice_shown,
};
use super::{LatestFrames, retire_all, take_unshowable};
use gpui_kit::{EntityId, RenderImage, WindowId};
use quill::ids::ChatId;
use smallvec::SmallVec;
use std::sync::Arc;

fn picture() -> Arc<RenderImage> {
    let frame = image::Frame::new(image::RgbaImage::new(2, 2));
    Arc::new(RenderImage::new(SmallVec::from_buf([frame])))
}

/// A sweep with only the main window open.
fn main_sweep() -> Vec<Arc<RenderImage>> {
    take_unshowable(true, &[])
}

/// Retiring an image twice (two caches let go of it, or one cache did
/// twice) still lets it leave: two entries held each other, and animated
/// sticker frames piled up at 20 MB a second.
#[test]
fn an_image_retired_twice_still_leaves() {
    let image = picture();
    let (id, held) = (image.id, Arc::downgrade(&image));
    retire_all([image.clone()]);
    begin_frame();
    retire_all([image]);
    assert!(
        main_sweep().is_empty(),
        "the frame it was retired in may show it"
    );
    begin_frame();
    let left: Vec<_> = main_sweep().iter().map(|image| image.id).collect();
    assert_eq!(left, [id], "it leaves the atlas once");
    assert_eq!(held.strong_count(), 0, "and nothing holds it");
}

/// The F1 leak: an inline video retires a picture every frame while only
/// its animation layer redraws and the chat list replays a render from
/// long ago. Pictures made after that render must leave after the next
/// frame; one the chat list may have painted must stay.
#[test]
fn frames_retired_while_a_slice_replays_leave_after_the_next_frame() {
    let sidebar = EntityId::from((1_u64 << 32) | 13);
    begin_frame();
    let painted_by_sidebar = next_image_id();
    slice_rendered(sidebar, None);
    // Layer-only frames: `QuillApp` renders, the sidebar replays.
    begin_frame();
    slice_shown(sidebar, None);
    let mut pictures = Vec::new();
    for _ in 0..30 {
        let picture = next_image_id();
        let retired = begin_frame();
        slice_shown(sidebar, None);
        pictures.push((picture, retired));
    }
    let main = LatestFrames::now(true);
    assert_eq!(main.slices.len(), 1);
    // The latest frame may still show the newest picture.
    let (newest, newest_at) = pictures.pop().unwrap_or_default();
    assert_eq!(newest_at, frame());
    assert!(main.may_show(newest, newest_at));
    for (picture, retired) in &pictures {
        assert!(
            !main.may_show(*picture, *retired),
            "picture {picture} retired in frame {retired} is held by a replaying slice"
        );
    }
    // Something the sidebar's render may have painted stays until the
    // sidebar renders again.
    assert!(main.may_show(painted_by_sidebar, frame() - 1));
    begin_frame();
    slice_rendered(sidebar, None);
    assert!(!LatestFrames::now(true).may_show(painted_by_sidebar, frame() - 2));
}

/// Frames that redraw only an animation layer: what the layer drew leaves
/// frame by frame, while what the whole frame drew still shows, however
/// many such frames pass.
#[test]
fn a_patch_frame_lets_go_of_the_layer_alone() {
    let slice = EntityId::from((1_u64 << 32) | 19);
    let layer = EntityId::from((1_u64 << 32) | 21);
    let in_slice = next_image_id();
    let rendered = begin_frame();
    slice_rendered(slice, None);
    let in_app = next_image_id();
    let whole = begin_frame();
    slice_shown(slice, None);
    slice_rendered(layer, None);
    LatestFrames::now(true);
    let mut drawn = Vec::new();
    for _ in 0..30 {
        let picture = next_image_id();
        begin_patch_frame();
        slice_rendered(layer, None);
        LatestFrames::now(true);
        drawn.push((picture, frame()));
    }
    let main = LatestFrames::now(true);
    assert!(main.may_show(in_slice, rendered), "the slice it replays");
    assert!(main.may_show(in_app, whole), "the whole frame");
    let (latest, retired) = drawn.pop().unwrap_or_default();
    assert!(main.may_show(latest, retired), "the layer's latest frame");
    for (picture, replaced) in &drawn {
        assert!(
            !main.may_show(*picture, *replaced),
            "picture {picture}, replaced in frame {replaced}"
        );
    }
}

/// A slice render may paint whatever exists until its frame is done.
#[test]
fn a_slice_render_covers_what_was_made_until_its_frame_was_done() {
    let slice = EntityId::from((1_u64 << 32) | 15);
    begin_frame();
    slice_rendered(slice, None);
    let during = next_image_id();
    // Between frames: the frame of the render is done.
    LatestFrames::now(true);
    let after = next_image_id();
    begin_frame();
    slice_shown(slice, None);
    let retired = frame();
    begin_frame();
    slice_shown(slice, None);
    let main = LatestFrames::now(true);
    assert!(main.may_show(during, retired), "the render may show it");
    assert!(!main.may_show(after, retired), "made after the render");
}

/// What a chat window's slice painted stays while the slice replays,
/// with the main window closed and through the window's patch frames; a
/// closed chat window keeps nothing.
#[test]
fn a_replaying_chat_slice_keeps_what_it_painted_until_its_window_closes() {
    let chat = ChatId(5);
    let slice = EntityId::from(0x61_u64);
    begin_chat_frame(chat, WindowDraw::Whole);
    slice_rendered(slice, Some(chat));
    let painted = picture();
    begin_chat_frame(chat, WindowDraw::Patch);
    let retired = frame();
    let id = painted.id.0;
    for _ in 0..3 {
        begin_chat_frame(chat, WindowDraw::Whole);
        slice_shown(slice, Some(chat));
    }
    assert!(LatestFrames::now(false).may_show(id, retired));
    forget_window(chat);
    assert!(!LatestFrames::now(false).may_show(id, retired));
}

/// Another window's latest frame shows what existed when it was done,
/// for as long as the window draws nothing new; with no window open,
/// nothing shows.
#[test]
fn a_window_that_stops_drawing_keeps_only_what_its_last_frame_may_show() {
    let chat = WindowId::from((1_u64 << 32) | 3);
    let shown = picture();
    // The sweep after the window's first frame.
    take_unshowable(false, &[(chat, 1)]);
    let later = picture();
    let (shown_id, later_id) = (shown.id, later.id);
    retire_all([shown, later]);
    let left: Vec<_> = take_unshowable(false, &[(chat, 1)])
        .iter()
        .map(|image| image.id)
        .collect();
    assert_eq!(left, [later_id], "made after the frame on screen");
    for _ in 0..3 {
        assert!(take_unshowable(false, &[(chat, 1)]).is_empty());
    }
    let left: Vec<_> = take_unshowable(false, &[(chat, 2)])
        .iter()
        .map(|image| image.id)
        .collect();
    assert_eq!(left, [shown_id], "the window drew again");
    let image = picture();
    retire_all([image.clone()]);
    drop(image);
    assert_eq!(take_unshowable(false, &[]).len(), 1, "no window open");
}

#[cfg(feature = "demo-capture")]
mod windows {
    use super::super::super::WindowDraw;
    use super::super::super::{BoundedImageCache, begin_frame, slice_rendered, slice_shown};
    use super::super::{begin_main_frame, retire_all, start_sweeper, sweep};
    use super::picture;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AnyWindowHandle, AppContext, Context, Entity, EntityId, IntoElement, ParentElement, Render,
        RenderImage, Styled, TestAppContext, Window, div, img, px, size,
    };
    use std::sync::{Arc, Weak};

    fn slice() -> EntityId {
        EntityId::from((1_u64 << 32) | 21)
    }

    /// A slice that renders every frame (the conversation, while it plays).
    fn busy_slice() -> EntityId {
        EntityId::from((1_u64 << 32) | 23)
    }

    /// Draws as `QuillApp` does: a picture of its own, one in a cached
    /// slice that rendered once and replays since, and a slice that renders
    /// every frame.
    struct MainWindow {
        own: Option<Arc<RenderImage>>,
        in_slice: Option<Arc<RenderImage>>,
        slice_renders: bool,
    }

    impl Render for MainWindow {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            begin_main_frame(window, WindowDraw::Whole, cx);
            if std::mem::take(&mut self.slice_renders) {
                slice_rendered(slice(), None);
            } else {
                slice_shown(slice(), None);
            }
            slice_rendered(busy_slice(), None);
            let pictures = [&self.own, &self.in_slice];
            div().children(
                pictures
                    .into_iter()
                    .flatten()
                    .map(|picture| img(picture.clone()).size(px(40.))),
            )
        }
    }

    /// Plays a sticker, as a chat window does: each frame paints a new
    /// picture and retires the one before.
    #[derive(Default)]
    struct ChatWindow {
        shown: Option<Arc<RenderImage>>,
        made: Vec<(Weak<RenderImage>, RenderImage)>,
    }

    impl Render for ChatWindow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let next = picture();
            self.made.push((Arc::downgrade(&next), probe(&next)));
            retire_all(self.shown.replace(next.clone()));
            img(next).size(px(40.))
        }
    }

    impl Drop for ChatWindow {
        fn drop(&mut self) {
            retire_all(self.shown.take());
        }
    }

    /// Shows one picture until told otherwise, as a chat window at rest
    /// does, starting each frame as a chat window does (its cache).
    struct StillWindow {
        cache: Entity<BoundedImageCache>,
        shown: Option<Arc<RenderImage>>,
    }

    impl Render for StillWindow {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.cache
                .update(cx, |cache, cx| cache.begin_window_frame(WindowDraw::Whole, window, cx));
            div().children(self.shown.clone().map(|picture| img(picture).size(px(40.))))
        }
    }

    /// An image with `image`'s id, to ask an atlas about it once it is
    /// gone.
    fn probe(image: &RenderImage) -> RenderImage {
        let frame = image::Frame::new(image::RgbaImage::new(1, 1));
        let mut probe = RenderImage::new(smallvec::SmallVec::from_buf([frame]));
        probe.id = image.id;
        probe
    }

    fn draw(cx: &mut TestAppContext, window: AnyWindowHandle) {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("the window is open");
        cx.run_until_parked();
    }

    fn in_atlas(cx: &mut TestAppContext, window: AnyWindowHandle, image: &RenderImage) -> bool {
        cx.update_window(window, |_, window, _| window.has_image_atlas_entry(image))
            .expect("the window is open")
    }

    /// The Mac leak: with the main window minimized, a chat window playing
    /// stickers grew by 25 MB a second, since only the main window's frames
    /// swept retired images. Its frames now leave after its next frame; what
    /// the main window's last frame may show (a picture of its own, one a
    /// cached slice replays) stays until it draws again.
    #[gpui_kit::test]
    fn a_chat_windows_frames_leave_while_the_main_window_does_not_draw(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            start_sweeper(cx);
        });
        let main = cx.open_window(size(px(200.), px(200.)), |_, _| MainWindow {
            own: Some(picture()),
            in_slice: Some(picture()),
            slice_renders: true,
        });
        let main_view = main.root(cx).expect("the main view");
        // The slice's picture is retired while the slice replays the
        // render that painted it: only that replay shows it now.
        let in_slice = main_view.update(cx, |main, _| main.in_slice.take().expect("shown"));
        let replayed = Arc::downgrade(&in_slice);
        retire_all([in_slice]);
        draw(cx, main.into());
        draw(cx, main.into());
        // Minimized: the main window draws no more, and lets go of its own
        // picture without drawing.
        let own = main_view.update(cx, |main, _| {
            main.slice_renders = true;
            main.own.take().expect("shown")
        });
        let kept = [Arc::downgrade(&own), replayed];
        retire_all([own]);
        cx.run_until_parked();

        let chat = cx.open_window(size(px(200.), px(200.)), |_, _| ChatWindow::default());
        let chat_view = chat.root(cx).expect("the chat view");
        for _ in 0..60 {
            draw(cx, chat.into());
        }
        let alive = chat_view.read_with(cx, |chat, _| {
            chat.made
                .iter()
                .filter(|(made, _)| made.strong_count() > 0)
                .count()
        });
        assert!(alive <= 2, "{alive} of 61 sticker frames still held");
        let probes: Vec<_> = chat_view.update(cx, |chat, _| {
            chat.made.drain(..).map(|(_, probe)| probe).collect()
        });
        let in_chat_atlas = probes
            .iter()
            .filter(|probe| in_atlas(cx, chat.into(), probe))
            .count();
        assert!(in_chat_atlas <= 2, "{in_chat_atlas} frames in the atlas");
        for picture in &kept {
            let picture = picture.upgrade().expect("the main window may show it");
            assert!(in_atlas(cx, main.into(), &picture));
        }

        // Back on screen, it draws without them.
        main_view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        draw(cx, main.into());
        for picture in kept {
            assert_eq!(picture.strong_count(), 0, "gone once no frame shows it");
        }
    }

    /// A chat window lets go of the picture its frame on screen shows, and
    /// draws later (or never: minimized). The picture stays in its atlas,
    /// however many frames the main window draws meanwhile, until the chat
    /// window draws without it.
    #[gpui_kit::test]
    fn a_picture_a_chat_window_shows_stays_until_that_window_draws_again(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            start_sweeper(cx);
        });
        let main = cx.open_window(size(px(200.), px(200.)), |_, _| MainWindow {
            own: None,
            in_slice: None,
            slice_renders: true,
        });
        // A sweep after the main window's frame bounds what that frame
        // shows: the chat window's picture, made after, is not in it.
        retire_all([picture()]);
        cx.run_until_parked();
        let chat = cx.open_window(size(px(200.), px(200.)), |_, cx| StillWindow {
            cache: cx.new(|_| BoundedImageCache::for_window(quill::ids::ChatId(1), Default::default())),
            shown: Some(picture()),
        });
        let chat_view = chat.root(cx).expect("the chat view");
        let shown = chat_view.update(cx, |chat, _| chat.shown.take().expect("shown"));
        let (kept, shown_probe) = (Arc::downgrade(&shown), probe(&shown));
        retire_all([shown]);
        cx.run_until_parked();
        assert_eq!(kept.strong_count(), 1, "its frame on screen shows it");
        assert!(in_atlas(cx, chat.into(), &shown_probe));
        for _ in 0..3 {
            draw(cx, main.into());
        }
        assert_eq!(
            kept.strong_count(),
            1,
            "the main window's frames say nothing"
        );
        assert!(in_atlas(cx, chat.into(), &shown_probe));

        draw(cx, chat.into());
        assert_eq!(kept.strong_count(), 0, "drawn without it: gone");
        assert!(!in_atlas(cx, chat.into(), &shown_probe));
    }

    #[gpui_kit::test]
    fn a_sweep_frees_pixels_off_the_ui_thread(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let main = cx.open_window(size(px(200.), px(200.)), |_, _| MainWindow {
            own: Some(picture()),
            in_slice: None,
            slice_renders: true,
        });
        let view = main.root(cx).expect("the main view");
        let own = view.update(cx, |main, _| main.own.take().expect("shown"));
        let (pixels, own_probe) = (Arc::downgrade(&own), probe(&own));
        assert!(in_atlas(cx, main.into(), &own_probe));
        retire_all([own]);
        begin_frame();
        begin_frame();

        cx.update(|cx| sweep(None, cx));
        assert!(!in_atlas(cx, main.into(), &own_probe), "out of the atlas");
        assert_eq!(pixels.strong_count(), 1, "not freed on the UI thread");
        cx.run_until_parked();
        assert_eq!(pixels.strong_count(), 0, "freed in the background");
    }

    /// The main window closed while a chat window played stickers grew as
    /// fast on a Mac, and with no window open Quill kept 657 MB of retired
    /// pictures. A closed window shows nothing; with none open, everything
    /// retired goes, and so does what is retired later.
    #[gpui_kit::test]
    fn retired_pictures_leave_with_the_windows_that_showed_them(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            start_sweeper(cx);
        });
        let main = cx.open_window(size(px(200.), px(200.)), |_, _| MainWindow {
            own: Some(picture()),
            in_slice: None,
            slice_renders: true,
        });
        let main_view = main.root(cx).expect("the main view");
        let chat = cx.open_window(size(px(200.), px(200.)), |_, _| ChatWindow::default());
        let chat_view = chat.root(cx).expect("the chat view");
        draw(cx, chat.into());
        let own = main_view.update(cx, |main, _| main.own.take().expect("shown"));
        let kept = Arc::downgrade(&own);
        retire_all([own]);
        cx.run_until_parked();
        assert_eq!(kept.strong_count(), 1, "the main window may show it");

        cx.update_window(main.into(), |_, window, _| window.remove_window())
            .expect("the main window is open");
        cx.run_until_parked();
        for _ in 0..30 {
            draw(cx, chat.into());
        }
        assert_eq!(kept.strong_count(), 0, "no window shows it");
        let made: Vec<_> = chat_view.read_with(cx, |chat, _| {
            chat.made.iter().map(|(made, _)| made.clone()).collect()
        });
        drop(chat_view);
        let alive = made.iter().filter(|made| made.strong_count() > 0).count();
        assert!(
            alive <= 2,
            "{alive} of {} sticker frames still held",
            made.len()
        );

        // No window open: what was retired goes, and so does what the
        // app lets go of afterwards.
        cx.update_window(chat.into(), |_, window, _| window.remove_window())
            .expect("the chat window is open");
        cx.run_until_parked();
        assert!(cx.windows().is_empty());
        let alive = made.iter().filter(|made| made.strong_count() > 0).count();
        assert_eq!(alive, 0, "sticker frames held with no window open");
        let late = picture();
        let gone = Arc::downgrade(&late);
        retire_all([late]);
        cx.run_until_parked();
        assert_eq!(gone.strong_count(), 0, "retired with no window open");
    }
}
