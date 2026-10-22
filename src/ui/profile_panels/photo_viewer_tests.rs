//! A group's info panel avatar opens the photo viewer on the current
//! photo, with the earlier photos after it.

use crate::ui::app::QuillApp;
use crate::ui::demos::group_photos::PHOTO_GROUP;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{AppContext, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::state::{PhotoDelete, PhotoOwner};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

#[gpui_kit::test]
fn the_group_avatar_opens_its_photos_newest_first(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-info-group-photos");
    let handle = cx.open_window(size(px(1200.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    let Some(app) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    let avatar = vcx.update(|window, _| window.find_by_label("View photos").bounds());
    vcx.simulate_click(avatar.center(), Modifiers::default());
    frames(&mut vcx);

    let opened = app.read_with(&vcx, |app, _| {
        (
            app.viewer.extra.profile_owner,
            app.viewer.state.current().map(|item| item.message_id.0),
            app.profile_photo_facts()
                .map(|facts| (facts.header, facts.actions)),
        )
    });
    let (owner, photo, facts) = opened;
    assert_eq!(owner, Some(PhotoOwner::Chat(ChatId(PHOTO_GROUP))));
    assert_eq!(photo, Some(9303));
    let Some((header, actions)) = facts else {
        panic!("a gallery photo is on screen");
    };
    assert_eq!(header, "Photo 1 of 3");
    // The creator may delete the current photo; it is already the main one.
    assert_eq!(
        actions.delete,
        Some(PhotoDelete::ChatPhoto {
            chat_id: ChatId(PHOTO_GROUP)
        })
    );
    assert!(!actions.set_main && !actions.report);

    app.update(&mut vcx, |app, cx| app.step_media_viewer(1, cx));
    frames(&mut vcx);
    let next = app.read_with(&vcx, |app, _| {
        (
            app.viewer.state.current().map(|item| item.message_id.0),
            app.profile_photo_facts()
                .map(|facts| (facts.header, facts.actions.set_main)),
        )
    });
    assert_eq!(next, (Some(9302), Some(("Photo 2 of 3".to_string(), true))));
}
