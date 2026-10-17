//! Moving across posters in a viewer opened from the story tray, over the
//! `ready-story-viewer` demo: Leo's unread story opens, so the walk holds
//! the posters with unread stories in tray order, Leo, Quill News, then
//! Maya (Sam's stories are read). Nothing reaches Telegram.
use super::open_tests::{frames, open_app};
use crate::ui::app::QuillApp;
use crate::ui::media_panel::PanelTab;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};
use quill::ids::ChatId;
use quill::story_sibling_layout::sibling_layout;
use quill::story_viewer::{StoryOpenScope, StoryViewerItem};
use quill::telegram::envelope::StoryListView;

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
        app.open_story_viewer(ChatId(MAYA), 6, StoryOpenScope::Tray, cx);
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

#[gpui_kit::test]
fn a_story_opened_from_a_card_shows_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.close_story_viewer(cx);
        app.open_story_viewer(ChatId(LEO), 3, StoryOpenScope::Single, cx);
    });
    frames(vcx);
    assert_eq!(shown(&app, vcx), Some((LEO, 3)));
    app.read_with(vcx, |app, _| {
        assert!(app.stories.viewer.tray_walk().is_none(), "no walk");
        assert_eq!(app.stories.viewer.position(), Some((1, 1)));
        assert!(!app.story_step_available(1));
    });
    assert!(vcx.debug_bounds("story-sibling-left").is_none());
    assert!(vcx.debug_bounds("story-sibling-right").is_none());
    step(&app, vcx, 1);
    assert!(
        app.read_with(vcx, |app, _| !app.stories.viewer.is_open()),
        "Next past the story closes the viewer"
    );
}

#[gpui_kit::test]
fn a_neighbour_whose_story_is_gone_is_passed_over(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stories.stories.remove(&(NEWS, 1));
            session.stories.missing.insert((NEWS, 1));
        }
        cx.notify();
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.story_step_available(1)));
    step(&app, vcx, 1);
    assert_eq!(shown(&app, vcx), Some((MAYA, 5)), "Quill News is skipped");
}

#[gpui_kit::test]
fn a_neighbour_that_cannot_show_yet_neither_enables_next_nor_blocks_the_end(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stories.stories.remove(&(NEWS, 1));
        }
        cx.notify();
    });
    frames(vcx);
    assert!(
        !app.read_with(vcx, |app, _| app.story_step_available(1)),
        "Quill News is still loading"
    );
    assert!(vcx.debug_bounds("story-sibling-right").is_none());
    step(&app, vcx, 1);
    assert!(app.read_with(vcx, |app, _| !app.stories.viewer.is_open()));
}

#[gpui_kit::test]
fn an_archived_poster_walks_the_archive(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.close_story_viewer(cx);
        if let Some(session) = app.demo_session.as_mut() {
            for chat in [LEO, MAYA] {
                if let Some(entry) = session.stories.tray.get_mut(&chat) {
                    entry.list = Some(StoryListView::Archive);
                }
            }
        }
        app.open_story_viewer(ChatId(MAYA), 5, StoryOpenScope::Tray, cx);
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let walk = app.stories.viewer.tray_walk().expect("a walk");
        assert_eq!(walk.list(), StoryListView::Archive);
        let mut posters: Vec<i64> = [-1, 1]
            .into_iter()
            .flat_map(|delta| walk.neighbours(delta))
            .map(|(chat, _)| chat)
            .collect();
        posters.sort_unstable();
        assert_eq!(posters, vec![LEO], "only the other archived poster");
    });
}

#[gpui_kit::test]
fn the_neighbour_sits_beside_the_panel_centred_on_the_story(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    let frame = vcx.debug_bounds("story-viewer-frame").expect("the story");
    let panel = vcx.debug_bounds("story-viewer-panel").expect("the panel");
    let right = vcx.debug_bounds("story-sibling-right").expect("Quill News");
    let window = f32::from(vcx.update(|window, _| window.viewport_size().width));
    let layout = sibling_layout(window).expect("room beside the story");
    assert!((f32::from(right.size.width) - layout.width).abs() < 0.5);
    assert!((f32::from(right.origin.x) - layout.right_x).abs() < 0.5);
    assert!(
        right.origin.x >= panel.origin.x + panel.size.width,
        "clear of the panel"
    );
    let centre = |b: gpui_kit::Bounds<gpui_kit::Pixels>| f32::from(b.origin.y + b.size.height / 2.);
    assert!(
        (centre(right) - centre(frame)).abs() <= 1.,
        "centred on the story: {right:?} {frame:?}"
    );
}

#[gpui_kit::test]
fn the_reply_media_panel_hides_the_neighbours(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx, "ready-story-viewer");
    let vcx = &mut vcx;
    assert!(vcx.debug_bounds("story-sibling-right").is_some());
    let toggle = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update(vcx, |app, cx| {
            app.stories.reply_open = true;
            app.toggle_story_reply_panel(PanelTab::Emoji, cx);
        });
        frames(vcx);
    };
    toggle(&app, vcx);
    assert!(app.read_with(vcx, |app, _| app.story_reply_panel_active()));
    assert!(
        vcx.debug_bounds("story-sibling-right").is_none(),
        "the panel sits where Quill News was"
    );
    toggle(&app, vcx);
    assert!(
        vcx.debug_bounds("story-sibling-right").is_some(),
        "back once it closes"
    );
}
