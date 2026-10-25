//! The story viewer's ⋯ menu holds what the old row of text buttons held,
//! behind the same gates: someone else's story offers Report and Hide, your
//! own offers Edit, Privacy, Close friends and Delete instead.
use super::icon_chrome::MenuAct;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-story-photo")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

fn acts(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<MenuAct> {
    app.read_with(vcx, |app, _| {
        app.story_viewer_menu_entries()
            .into_iter()
            .map(|entry| entry.act)
            .collect()
    })
}

#[gpui_kit::test]
fn the_menu_follows_whose_story_it_is(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let theirs = acts(&app, vcx);
    for act in [
        MenuAct::Stories,
        MenuAct::CopyLink,
        MenuAct::Hide,
        MenuAct::Report,
    ] {
        assert!(theirs.contains(&act), "{act:?} for someone else's story");
    }
    for act in [MenuAct::Delete, MenuAct::Edit, MenuAct::CloseFriends] {
        assert!(!theirs.contains(&act), "{act:?} is only for your own");
    }
    assert!(
        !theirs.contains(&MenuAct::Stealth),
        "Hide My View is only for a user's stories, not a channel's"
    );
    app.update(vcx, |app, _| {
        let key = app
            .stories
            .viewer
            .current()
            .map(|item| (item.chat_id.0, item.story_id));
        if let (Some(key), Some(session)) = (key, app.demo_session.as_mut())
            && let Some(story) = session.stories.stories.get_mut(&key)
        {
            story.can_be_deleted = true;
            story.can_be_edited = true;
            story.can_set_privacy_settings = true;
        }
    });
    let mine = acts(&app, vcx);
    for act in [
        MenuAct::Delete,
        MenuAct::Edit,
        MenuAct::Privacy,
        MenuAct::CloseFriends,
    ] {
        assert!(mine.contains(&act), "{act:?} for your own story");
    }
    for act in [MenuAct::Report, MenuAct::Hide] {
        assert!(!mine.contains(&act), "{act:?} is not for your own");
    }
}
