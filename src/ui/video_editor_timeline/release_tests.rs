//! The lost mouse-up of the video editor's picture and quality line, ended
//! by the window's blur and by the next pointer move with the button up.

use super::*;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::video_editor::{EditorTarget, StillCut, VideoEditorDialog};
use crate::ui::video_editor_stage::EditorStage;
use core::prelude::v1::test;
use gpui_kit::component::Root;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, px, size};
use quill::video_timeline::{CropArea, CropGrab};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(900.), px(600.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn hold(app: &mut QuillApp, cx: &mut Context<QuillApp>, drag: Drag) {
    let probe = cx.background_spawn(async {});
    let mut motion = EditorMotion::default();
    // The crop grid is shown while the picture is pressed.
    if matches!(drag, Drag::Pressed(_)) {
        motion.grid.start(1.0, 100, true);
    }
    motion.drag = drag;
    app.composer_ui.send_box.video_editor = Some(VideoEditorDialog {
        target: EditorTarget::MessageEdit,
        source: std::path::PathBuf::new(),
        stage: EditorStage::Reading { probe },
        cover: None,
        still_cut: StillCut::Idle,
        motion,
        set_emoji: None,
    });
}

fn state(app: &QuillApp) -> Option<(bool, f32)> {
    let editor = app.composer_ui.send_box.video_editor.as_ref()?;
    Some((
        matches!(editor.motion.drag, Drag::Idle),
        editor.motion.grid.target(),
    ))
}

fn pressed(grab: Option<CropGrab>) -> Drag {
    Drag::Pressed(StagePress {
        at: Point::default(),
        grab,
        start: CropArea::FULL,
    })
}

/// The window losing focus with the quality line or the picture held ends
/// the hold: the drag is idle and the crop grid goes.
#[gpui_kit::test]
fn the_blur_ends_a_held_quality_line_and_picture(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, cx| {
        hold(app, cx, Drag::Quality);
        app.release_lost_video_drag(cx);
        assert_eq!(state(app), Some((true, 0.0)), "the line let go");
        hold(app, cx, pressed(Some(CropGrab::default())));
        app.release_lost_video_drag(cx);
        assert_eq!(state(app), Some((true, 0.0)), "the grid goes");
    });
}

/// A pointer move with the button up (the mouse-up went outside the window)
/// ends the same holds through the strip's move hook.
#[gpui_kit::test]
fn a_move_with_the_button_up_ends_a_held_picture(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, cx| {
        hold(app, cx, pressed(Some(CropGrab::default())));
        app.video_timeline_moved(false, 0.0, cx);
        assert_eq!(state(app), Some((true, 0.0)), "the grid goes");
        hold(app, cx, Drag::Quality);
        app.video_timeline_moved(false, 0.0, cx);
        assert_eq!(state(app), Some((true, 0.0)));
    });
}
