//! The Data & Storage step sliders against a demo window (needs the
//! `demo-capture` feature for gpui-kit's test support): the network
//! editor's size caps and the local storage limits.
use crate::ui::app::QuillApp;
use crate::ui::index_sliders::StepSlider;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::data_settings::{CapKind, NetworkKind};
use quill::limit_steps::{cap_at, cap_index};

const MIB: i64 = 1024 * 1024;
const GIB: i64 = 1024 * MIB;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-index-sliders");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(crate::ui::bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(2400.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("demo window did not build");
    };
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn shown(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(selector).is_some())
}

fn move_slider(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    key: StepSlider,
    index: usize,
    release: bool,
) {
    app.update_in(vcx, |app, window, cx| {
        let Some(slider) = app.settings.index_sliders.get(key).cloned() else {
            panic!("slider {key:?} missing");
        };
        let value = SliderValue::Single(index as f32);
        slider.update(cx, |state, cx| {
            state.set_value(value, window, cx);
            cx.emit(if release {
                SliderEvent::Release(value)
            } else {
                SliderEvent::Change(value)
            });
        });
    });
    frames(vcx);
}

fn draft_cap(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, kind: CapKind) -> Option<i64> {
    app.read_with(vcx, |app, _| {
        app.settings
            .data_storage_editor
            .as_ref()
            .map(|(_, draft)| draft.cap(kind))
    })
}

#[gpui_kit::test]
fn network_editor_size_caps_follow_their_sliders(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.demo_ready_storage_usage(window, cx)
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("data-storage-net-Wi-Fi", cx));
    frames(vcx);
    let wifi = app.read_with(vcx, |app, _| {
        app.settings.data_storage_editor.as_ref().map(|(n, _)| *n)
    });
    assert_eq!(wifi, Some(NetworkKind::WiFi), "the row opens the editor");
    for id in [
        "data-storage-limit-photo",
        "data-storage-limit-video",
        "data-storage-limit-other",
    ] {
        assert!(shown(vcx, id), "{id} is shown while the type is on");
    }

    // Dragging sets the cap to the step: index 29 is 50 MB.
    move_slider(&app, vcx, StepSlider::DataCap(CapKind::Video), 29, false);
    assert_eq!(draft_cap(&app, vcx, CapKind::Video), Some(50 * MIB));
    move_slider(&app, vcx, StepSlider::DataCap(CapKind::Video), 39, true);
    assert_eq!(draft_cap(&app, vcx, CapKind::Video), Some(100 * MIB));
    // The other types did not move.
    assert_eq!(draft_cap(&app, vcx, CapKind::Photo), Some(10 * MIB));

    // The switch turns the type off (no slider) and on again where the
    // slider is.
    vcx.update(|window, cx| window.click("data-storage-cap-photo", cx));
    frames(vcx);
    assert_eq!(draft_cap(&app, vcx, CapKind::Photo), Some(0));
    assert!(!shown(vcx, "data-storage-limit-photo"));
    vcx.update(|window, cx| window.click("data-storage-cap-photo", cx));
    frames(vcx);
    assert_eq!(
        draft_cap(&app, vcx, CapKind::Photo),
        Some(10 * MIB),
        "back on at the slider's step"
    );
    assert!(shown(vcx, "data-storage-limit-photo"));

    // The last step is "no limit" for a photo (an int32 on the wire).
    move_slider(&app, vcx, StepSlider::DataCap(CapKind::Photo), 99, true);
    assert_eq!(
        draft_cap(&app, vcx, CapKind::Photo),
        Some(quill::data_settings::NO_LIMIT_SIZE_CAP)
    );
    assert_eq!(cap_index(cap_at(10)), 10);
}

#[gpui_kit::test]
fn storage_limit_sliders_apply_when_let_go(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.demo_ready_storage_usage(window, cx)
    });
    frames(vcx);
    assert!(shown(vcx, "local-storage-size-limit"));
    assert!(shown(vcx, "local-storage-time-limit"));
    let limits = |app: &QuillApp| {
        let Some(session) = app.session() else {
            panic!("demo session missing");
        };
        (
            session.settings.storage_limits.size_limit(),
            session.settings.storage_limits.keep_for(),
        )
    };
    // Dragging only moves the label.
    move_slider(&app, vcx, StepSlider::StorageSize, 9, false);
    assert_eq!(app.read_with(vcx, |app, _| limits(app)), (None, None));
    // Letting go applies the step: index 9 is 2 GB.
    move_slider(&app, vcx, StepSlider::StorageSize, 9, true);
    assert_eq!(
        app.read_with(vcx, |app, _| limits(app)),
        (Some(2 * GIB), None)
    );
    // The time slider keeps the size: index 3 is one month.
    move_slider(&app, vcx, StepSlider::StorageKeep, 3, true);
    assert_eq!(
        app.read_with(vcx, |app, _| limits(app)),
        (Some(2 * GIB), Some(31 * 86_400))
    );
    // The last steps read "No limit" and "Never".
    move_slider(&app, vcx, StepSlider::StorageSize, 18, true);
    move_slider(&app, vcx, StepSlider::StorageKeep, 15, true);
    assert_eq!(app.read_with(vcx, |app, _| limits(app)), (None, None));
}
