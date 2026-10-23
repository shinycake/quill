//! The story album picker and the cover picker through the real app: the
//! ticks and the button's numbers follow the taps.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::story_page::StoryPage;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::story_album_picker::AlbumPickerTarget;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the window made the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

#[gpui_kit::test]
fn ticking_stories_tracks_the_album_edit(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.stories.page = Some(StoryPage::new(ChatId(1), window, cx));
        app.open_album_picker(AlbumPickerTarget::Album(3), cx);
        app.toggle_picker_story(11, cx);
        app.toggle_picker_story(12, cx);
        app.toggle_picker_story(11, cx);
    });
    let (count, added) = app.read_with(vcx, |app, _| {
        let picker = app
            .stories
            .page
            .as_ref()
            .and_then(|page| page.picker.as_ref())
            .expect("the picker is open");
        (picker.count(), picker.added())
    });
    assert_eq!((count, added), (1, vec![12]));
    app.update_in(vcx, |app, _, cx| app.close_album_picker(cx));
    assert!(app.read_with(vcx, |app, _| {
        app.stories
            .page
            .as_ref()
            .is_some_and(|page| page.picker.is_none())
    }));
}

#[gpui_kit::test]
fn the_cover_picker_opens_and_closes(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| app.toggle_story_cover_edit(1, 2, cx));
    assert!(app.read_with(vcx, |app, _| app.stories.cover.is_some()));
    app.update_in(vcx, |app, _, cx| app.toggle_story_cover_edit(1, 2, cx));
    assert!(app.read_with(vcx, |app, _| app.stories.cover.is_none()));
}
