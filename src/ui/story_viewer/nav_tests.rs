//! Moving across posters in a viewer opened from the story tray, over the
//! `ready-story-viewer` demo: Leo's unread story opens, so the walk holds
//! the posters with unread stories in tray order, Leo, Quill News, then
//! Maya (Sam's stories are read). Nothing reaches Telegram.
use super::open_tests::{frames, open_app};
use crate::ui::app::QuillApp;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};
use quill::ids::ChatId;
use quill::story_viewer::StoryViewerItem;

const LEO: i64 = 105;
const NEWS: i64 = 54;
const MAYA: i64 = 52;

fn shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<(i64, i32)> {
    app.read_with(vcx, |app, _| {
        app.stories
            .viewer
            .current()
            .map(StoryViewerItem::key)
            .map(|(chat, story)| (chat.0, story))
    })
}

fn step(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, delta: i32) {
    app.update(vcx, |app, cx| app.step_story_viewer(delta, cx));
    frames(vcx);
}

#[gpui_kit::test]
fn next_and_prev_cross_into_the_neighbour_posters(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    assert_eq!(shown(&app, vcx), Some((LEO, 3)));
    let prev = app.read_with(vcx, |app, _| app.story_step_available(-1));
    assert!(!prev, "no unread poster before Leo");
    step(&app, vcx, 1);
    assert_eq!(shown(&app, vcx), Some((NEWS, 1)), "the next poster");
    step(&app, vcx, 1);
    assert_eq!(shown(&app, vcx), Some((MAYA, 5)), "its first unread story");
    step(&app, vcx, 1);
    assert_eq!(shown(&app, vcx), Some((MAYA, 6)));
    step(&app, vcx, -1);
    step(&app, vcx, -1);
    assert_eq!(
        shown(&app, vcx),
        Some((NEWS, 1)),
        "back past the first story"
    );
    step(&app, vcx, -1);
    assert_eq!(shown(&app, vcx), Some((LEO, 3)));
    step(&app, vcx, -1);
    assert_eq!(shown(&app, vcx), Some((LEO, 3)), "the first story restarts");
}

#[gpui_kit::test]
fn a_poster_reopens_on_the_story_last_shown_for_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    step(&app, vcx, 1);
    step(&app, vcx, 1);
    step(&app, vcx, 1);
    assert_eq!(shown(&app, vcx), Some((MAYA, 6)));
    app.update(vcx, |app, cx| app.jump_story_poster(-1, cx));
    frames(vcx);
    assert_eq!(shown(&app, vcx), Some((NEWS, 1)));
    app.update(vcx, |app, cx| app.jump_story_poster(1, cx));
    frames(vcx);
    assert_eq!(
        shown(&app, vcx),
        Some((MAYA, 6)),
        "where the user left Maya"
    );
}

#[gpui_kit::test]
fn the_last_story_of_the_last_poster_closes_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.close_story_viewer(cx);
        app.open_story_viewer(ChatId(MAYA), 6, cx);
    });
    frames(vcx);
    assert_eq!(shown(&app, vcx), Some((MAYA, 6)));
    let next = app.read_with(vcx, |app, _| app.story_step_available(1));
    assert!(!next, "Maya is the last poster");
    step(&app, vcx, 1);
    assert!(app.read_with(vcx, |app, _| !app.stories.viewer.is_open()));
}

#[gpui_kit::test]
fn a_click_on_the_right_preview_opens_that_poster(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    assert!(vcx.debug_bounds("story-sibling-left").is_none());
    let right = vcx
        .debug_bounds("story-sibling-right")
        .expect("Quill News beside the story");
    vcx.simulate_click(right.center(), Modifiers::default());
    frames(vcx);
    assert_eq!(shown(&app, vcx), Some((NEWS, 1)));
    assert!(
        vcx.debug_bounds("story-sibling-left").is_some(),
        "Leo is now left"
    );
}
