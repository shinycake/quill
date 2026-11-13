//! The story page's waits through the real app: the picker, the typed name
//! and the album view change on the answer to the call, not on the press.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::story_page::{AlbumDelete, AlbumView, PageWait, StoryPage};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, RequestId};
use quill::story_album_picker::AlbumPickerTarget;
use quill::story_page::{StoryPageOp, StoryPageOpState};

const CHAT: ChatId = ChatId(1);

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
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn set_op(app: &mut QuillApp, state: StoryPageOpState) {
    if let Some(session) = app.demo_session.as_deref_mut() {
        session.stories.page_ops.begin(StoryPageOp {
            kind: quill::story_page::StoryPageOpKind::CreateAlbum,
            state,
            chat_id: Some(CHAT),
        });
    }
}

/// Create pressed with stories ticked and a name typed: while the call is
/// on its way, and after it fails, both stay.
#[gpui_kit::test]
fn a_failed_create_keeps_the_picker_and_the_name(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let request = RequestId(700);
    app.update_in(&mut vcx, |app, window, cx| {
        let page = StoryPage::new(CHAT, window, cx);
        page.new_album_name
            .update(cx, |input, cx| input.set_value("Trips", window, cx));
        app.stories.page = Some(page);
        app.open_album_picker(AlbumPickerTarget::NewAlbum, cx);
        app.toggle_picker_story(11, cx);
        if let Some(page) = app.stories.page.as_mut() {
            page.wait = PageWait::Creating(request);
        }
        set_op(app, StoryPageOpState::Sending { request });
        app.story_page_upkeep(window, cx);
        let page = app.stories.page.as_ref().expect("the page");
        assert_eq!(page.wait, PageWait::Creating(request), "still waiting");
        assert!(page.picker.is_some());
        set_op(
            app,
            StoryPageOpState::Failed {
                request,
                reason: "FLOOD".into(),
            },
        );
        app.story_page_upkeep(window, cx);
        let page = app.stories.page.as_ref().expect("the page");
        assert_eq!(page.wait, PageWait::Idle, "a failure ends the wait");
        assert_eq!(
            page.picker.as_ref().map(|picker| picker.selected_ids()),
            Some(vec![11]),
            "the ticked story is kept"
        );
        assert_eq!(page.new_album_name.read(cx).value().as_ref(), "Trips");
    });
}

#[gpui_kit::test]
fn a_created_album_closes_the_picker_and_clears_the_name(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let request = RequestId(701);
    app.update_in(&mut vcx, |app, window, cx| {
        let page = StoryPage::new(CHAT, window, cx);
        page.new_album_name
            .update(cx, |input, cx| input.set_value("Trips", window, cx));
        app.stories.page = Some(page);
        app.open_album_picker(AlbumPickerTarget::NewAlbum, cx);
        if let Some(page) = app.stories.page.as_mut() {
            page.wait = PageWait::Creating(request);
        }
        set_op(app, StoryPageOpState::Succeeded { request });
        app.story_page_upkeep(window, cx);
        let page = app.stories.page.as_ref().expect("the page");
        assert_eq!(page.wait, PageWait::Idle);
        assert!(page.picker.is_none());
        assert_eq!(page.new_album_name.read(cx).value().as_ref(), "");
    });
}

/// The album view goes when the delete is answered ok, not when it is
/// sent; a failed or lost delete leaves the album in view.
#[gpui_kit::test]
fn the_album_view_leaves_only_on_a_deleted_answer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let mut page = StoryPage::new(CHAT, window, cx);
        page.view = AlbumView::album(3);
        app.stories.page = Some(page);
        for (request, answered, leaves) in
            [(RequestId(710), false, false), (RequestId(711), true, true)]
        {
            if let Some(page) = app.stories.page.as_mut() {
                page.view = AlbumView::album(3);
                page.view.delete_sent(request);
            }
            set_op(app, StoryPageOpState::Sending { request });
            app.story_page_upkeep(window, cx);
            let open = app
                .stories
                .page
                .as_ref()
                .and_then(|page| page.view.deleting());
            assert_eq!(open, Some(request), "in flight keeps the view and its call");
            set_op(
                app,
                if answered {
                    StoryPageOpState::Succeeded { request }
                } else {
                    StoryPageOpState::Failed {
                        request,
                        reason: "NO".into(),
                    }
                },
            );
            app.story_page_upkeep(window, cx);
            let open = app
                .stories
                .page
                .as_ref()
                .and_then(|page| page.view.open_album());
            assert_eq!(open.is_none(), leaves);
        }
        // A delete whose op is gone (the page was left) ends the wait too.
        if let Some(page) = app.stories.page.as_mut() {
            page.view = AlbumView::album(3);
            page.view.delete_sent(RequestId(799));
        }
        app.story_page_upkeep(window, cx);
        let page = app.stories.page.as_ref().expect("the page");
        assert_eq!(page.view, AlbumView::album(3), "the wait ended with it");
    });
}

/// Delete armed, then the add-stories picker opened and closed: the next
/// click on Delete asks again instead of deleting.
#[gpui_kit::test]
fn opening_the_picker_disarms_the_delete(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let mut page = StoryPage::new(CHAT, window, cx);
        page.view = AlbumView::Album {
            id: 3,
            delete: AlbumDelete::Armed,
        };
        app.stories.page = Some(page);
        app.open_album_picker(AlbumPickerTarget::Album(3), cx);
        app.close_album_picker(cx);
        let armed = app
            .stories
            .page
            .as_ref()
            .is_some_and(|page| page.view.delete_armed(3));
        assert!(!armed, "the confirm did not outlive the picker");
    });
}

/// A second mutation while one is in flight is not sent.
#[gpui_kit::test]
fn a_second_mutation_waits_for_the_first(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.stories.page = Some(StoryPage::new(CHAT, window, cx));
        set_op(
            app,
            StoryPageOpState::Sending {
                request: RequestId(720),
            },
        );
        let mut called = false;
        let sent = app.story_page_mutate(
            "pin",
            |_, _| {
                called = true;
                Ok(())
            },
            cx,
        );
        assert!(sent.is_none());
        assert!(!called, "nothing is sent while the first call is in flight");
    });
}

/// An answer acts on what asked: a delete of album 3 that is answered while
/// album 4 is in view leaves album 4 in view.
#[gpui_kit::test]
fn a_deleted_answer_leaves_another_album_in_view(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let mut page = StoryPage::new(CHAT, window, cx);
        page.view = AlbumView::album(3);
        page.view.delete_sent(RequestId(730));
        // The user opens another album while the delete is on its way: the
        // delete's stage goes with the view it was part of.
        page.view = AlbumView::album(4);
        app.stories.page = Some(page);
        set_op(
            app,
            StoryPageOpState::Succeeded {
                request: RequestId(730),
            },
        );
        app.story_page_upkeep(window, cx);
        let page = app.stories.page.as_ref().expect("the page");
        assert_eq!(page.wait, PageWait::Idle);
        assert_eq!(page.view, AlbumView::album(4));
    });
}

/// A create answered after the picker moved on to an album keeps that
/// album's picker open.
#[gpui_kit::test]
fn a_created_answer_keeps_another_picker_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.stories.page = Some(StoryPage::new(CHAT, window, cx));
        app.open_album_picker(AlbumPickerTarget::Album(3), cx);
        if let Some(page) = app.stories.page.as_mut() {
            page.wait = PageWait::Creating(RequestId(740));
        }
        set_op(
            app,
            StoryPageOpState::Succeeded {
                request: RequestId(740),
            },
        );
        app.story_page_upkeep(window, cx);
        let page = app.stories.page.as_ref().expect("the page");
        assert!(page.picker.is_some());
    });
}

/// The adds of a save whose removes never left are answered: the picker
/// stays and the adds are no longer changes, so a retry does not send them
/// again.
#[gpui_kit::test]
fn a_partly_sent_save_keeps_the_picker_and_drops_the_sent_adds(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.stories.page = Some(StoryPage::new(CHAT, window, cx));
        app.open_album_picker(AlbumPickerTarget::Album(3), cx);
        app.toggle_picker_story(11, cx);
        if let Some(page) = app.stories.page.as_mut() {
            page.wait = PageWait::SavingPickerPartly {
                album_id: 3,
                requests: vec![RequestId(750)],
                added: vec![11],
            };
        }
        set_op(
            app,
            StoryPageOpState::Succeeded {
                request: RequestId(750),
            },
        );
        app.story_page_upkeep(window, cx);
        let page = app.stories.page.as_ref().expect("the page");
        assert_eq!(page.wait, PageWait::Idle);
        let picker = page.picker.as_ref().expect("the picker stays");
        assert!(picker.added().is_empty());
        assert!(picker.is_selected(11));
    });
}

/// A frame with a call still out asks for no redraw: the answer's own wake
/// does, so the upkeep must not keep the frame loop running meanwhile.
#[gpui_kit::test]
fn a_frame_with_the_call_still_out_does_not_ask_for_another(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let request = RequestId(730);
    app.update_in(&mut vcx, |app, window, cx| {
        let mut page = StoryPage::new(CHAT, window, cx);
        page.view = AlbumView::album(3);
        page.view.delete_sent(request);
        page.wait = PageWait::Creating(RequestId(731));
        app.stories.page = Some(page);
    });
    app.update(&mut vcx, |app, _| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            for request in [request, RequestId(731)] {
                session.stories.page_ops.begin(StoryPageOp {
                    kind: quill::story_page::StoryPageOpKind::CreateAlbum,
                    state: StoryPageOpState::Sending { request },
                    chat_id: Some(CHAT),
                });
            }
        }
    });
    vcx.run_until_parked();
    let notified = std::rc::Rc::new(std::cell::Cell::new(0_u32));
    let counted = notified.clone();
    let _watch = vcx.update(|_, cx| cx.observe(&app, move |_, _| counted.set(counted.get() + 1)));
    app.update_in(&mut vcx, |app, window, cx| {
        app.story_page_upkeep(window, cx)
    });
    assert_eq!(notified.get(), 0, "no redraw while both calls are out");
    app.update_in(&mut vcx, |app, window, cx| {
        set_op(app, StoryPageOpState::Succeeded { request });
        app.story_page_upkeep(window, cx);
    });
    assert_eq!(notified.get(), 1, "the answer redraws once");
}
