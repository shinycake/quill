//! The Data & Storage step sliders against a demo window (needs the
//! `demo-capture` feature for gpui-kit's test support): the automatic media
//! download box and the local storage limits.
use crate::ui::app::QuillApp;
use crate::ui::index_sliders::StepSlider;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::settings::{AutoDownloadLimitGroup, AutoDownloadOverride, SourceLimits};

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
        let Some(slider) = app.index_sliders.get(key).cloned() else {
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

fn groups_limits(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<SourceLimits> {
    app.read_with(vcx, |app, _| {
        app.session()
            .map(|s| s.settings.media_prefs.auto_download.groups)
    })
}

fn click(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| window.click(id, cx));
    frames(vcx);
}

#[gpui_kit::test]
fn the_auto_download_box_saves_its_switches_and_sliders(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.demo_ready_storage_usage(window, cx)
    });
    frames(vcx);
    let before = groups_limits(&app, vcx);
    click(vcx, "auto-download-open-groups");
    for id in [
        "auto-download-limit-download",
        "auto-download-limit-autoplay",
    ] {
        assert!(shown(vcx, id), "{id} is shown in the box");
    }
    // Index 29 is 50 MB, index 4 is 5 MB.
    let download = StepSlider::AutoDownloadLimit(AutoDownloadLimitGroup::Download);
    let autoplay = StepSlider::AutoDownloadLimit(AutoDownloadLimitGroup::Autoplay);
    move_slider(&app, vcx, download, 29, false);
    move_slider(&app, vcx, autoplay, 4, true);
    click(vcx, "auto-download-kind-file");
    // Nothing applies before Save.
    assert_eq!(groups_limits(&app, vcx), before);
    click(vcx, "auto-download-save");
    assert!(
        !shown(vcx, "auto-download-limit-download"),
        "Save closes the box"
    );
    let groups = groups_limits(&app, vcx).unwrap_or(SourceLimits::DISABLED);
    assert_eq!(groups.photo, 50 * MIB);
    assert_eq!(groups.file, 0);
    assert_eq!(groups.voice, 50 * MIB);
    assert_eq!(groups.video, 5 * MIB);

    // Cancel drops the edits.
    click(vcx, "auto-download-open-groups");
    click(vcx, "auto-download-kind-photo");
    click(vcx, "auto-download-cancel");
    assert_eq!(groups_limits(&app, vcx), Some(groups));

    // The row switch turns the whole source off, and on with the defaults.
    click(vcx, "auto-download-switch-groups");
    assert_eq!(groups_limits(&app, vcx), Some(SourceLimits::DISABLED));
    click(vcx, "auto-download-switch-groups");
    assert_eq!(
        groups_limits(&app, vcx),
        Some(SourceLimits::defaults(
            quill::settings::AutoDownloadSource::Groups
        ))
    );

    // The master switch pauses every automatic download.
    click(vcx, "auto-download-master");
    let paused = app.read_with(vcx, |app, _| {
        app.session()
            .is_some_and(|s| s.settings.media_prefs.data_saver)
    });
    assert!(paused);
    click(vcx, "auto-download-master");
}

#[gpui_kit::test]
fn the_exception_picker_stages_its_list_until_the_box_saves(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.demo_ready_storage_usage(window, cx)
    });
    frames(vcx);
    let private_chat = app.read_with(vcx, |app, _| {
        app.session().and_then(|s| {
            s.ordered_chats()
                .into_iter()
                .find(|chat| {
                    quill::settings::AutoDownloadSource::of_chat(&chat.kind)
                        == quill::settings::AutoDownloadSource::Private
                        && !s.is_saved_messages(chat.id)
                })
                .map(|chat| chat.id.0)
        })
    });
    let Some(chat) = private_chat else {
        panic!("the demo has a private chat");
    };
    click(vcx, "auto-download-open-private");
    click(vcx, "auto-download-exceptions-never");
    let pick: &'static str = Box::leak(format!("auto-download-pick-{chat}").into_boxed_str());
    assert!(shown(vcx, pick), "the picker lists private chats");
    click(vcx, pick);
    click(vcx, "auto-download-pick-save");
    let exception = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session().and_then(|s| {
                s.settings
                    .media_prefs
                    .auto_download
                    .exception(quill::ids::ChatId(chat))
            })
        })
    };
    assert_eq!(exception(&app, vcx), None, "only the box's Save applies");
    click(vcx, "auto-download-save");
    assert_eq!(exception(&app, vcx), Some(AutoDownloadOverride::Never));
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

/// Round 8, 2c: the limit sliders show one account's TDLib limits. The
/// next arrival on the page (here after an account switch, which leaves
/// no page and brings a fresh Settings state) starts them from the
/// account shown, so a drag cannot write the last account's other limit.
#[gpui_kit::test]
fn storage_limit_sliders_restart_from_the_account_shown(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.demo_ready_storage_usage(window, cx)
    });
    frames(vcx);
    move_slider(&app, vcx, StepSlider::StorageSize, 9, true);
    let index = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, cx| {
            app.index_sliders.index(StepSlider::StorageSize, cx)
        })
    };
    assert_eq!(index(&app, vcx), Some(9));
    // Another account, with no limits set, is shown now.
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.settings.storage_limits = Default::default();
        }
        app.settings.nav = crate::ui::settings_window::SettingsNav::List;
        app.open_settings(Some(quill::settings_catalog::Page::DataStorage), cx);
        cx.notify();
    });
    frames(vcx);
    assert_eq!(
        index(&app, vcx),
        Some(quill::limit_steps::SIZE_STEPS - 1),
        "the slider reads the account shown: no limit"
    );
}

/// Sweep 10-133: a drag whose release never came (the pointer was lost) is
/// sent when the page is left, and a released one is not sent twice.
#[gpui_kit::test]
fn a_storage_drag_without_a_release_applies_when_the_page_is_left(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.demo_ready_storage_usage(window, cx)
    });
    frames(vcx);
    let size_limit = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session()
                .and_then(|session| session.settings.storage_limits.size_limit())
        })
    };
    move_slider(&app, vcx, StepSlider::StorageSize, 9, false);
    assert_eq!(size_limit(&app, vcx), None, "no release yet");
    app.update(vcx, |app, _| app.leave_data_storage());
    assert_eq!(size_limit(&app, vcx), Some(2 * GIB), "the leave applied it");
}
