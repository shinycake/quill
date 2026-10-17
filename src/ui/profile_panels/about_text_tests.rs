//! The channel and group descriptions are linkified blocks in the info panel.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn open(cx: &mut TestAppContext, kind: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(kind);
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    (app, vcx)
}

fn stored_links(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    supergroup: i64,
) -> Vec<String> {
    app.read_with(vcx, |app, _| {
        let Some(info) = app
            .session()
            .and_then(|s| s.groups.supergroup_full_infos.get(&supergroup))
        else {
            return Vec::new();
        };
        info.description_entities
            .iter()
            .filter_map(|e| info.description.get(e.utf8_start..e.utf8_end))
            .map(str::to_string)
            .collect()
    })
}

#[gpui_kit::test]
fn the_channel_description_keeps_every_link(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-info-channel");
    assert!(vcx.update(|window, _| window.try_find("info-about").is_some()));
    assert_eq!(
        stored_links(&app, &mut vcx, 71),
        ["https://example.com/changelog", "news@example.com"]
    );
}

#[gpui_kit::test]
fn a_group_description_shows_only_telegram_links(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-info-group");
    assert!(vcx.update(|window, _| window.try_find("info-about").is_some()));
    // The stored list is whatever TDLib found; the panel strips it.
    let stored = stored_links(&app, &mut vcx, 72);
    assert!(stored.contains(&"https://example.com/rules".to_string()));
    let shown = app.read_with(&vcx, |app, _| {
        let info = app
            .session()
            .and_then(|s| s.groups.supergroup_full_infos.get(&72))?;
        let kept =
            quill::text::about::strip_external_links(&info.description, &info.description_entities);
        Some(
            kept.iter()
                .filter_map(|e| info.description.get(e.utf8_start..e.utf8_end))
                .map(str::to_string)
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(
        shown,
        Some(vec![
            "https://t.me/quillapp".to_string(),
            "@quill_app".to_string(),
            "#quill".to_string()
        ])
    );
}
