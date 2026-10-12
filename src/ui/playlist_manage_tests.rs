//! The Playlist page of your own saved music: reorder, the track menu and
//! the remove confirmation (needs the `demo-capture` feature for
//! gpui-kit's test support).
use super::{PlaylistMenu, REMOVED_TOAST};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, point, px, size};
use quill::ids::FileId;
use quill::telegram::envelope::ProfileAudio;

const ME: i64 = 7;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn track(id: i32) -> ProfileAudio {
    ProfileAudio {
        file_id: FileId(id),
        title: format!("Song {id}"),
        performer: "Artist".into(),
        file_name: String::new(),
        duration: 60,
    }
}

/// Your own playlist with tracks 1, 2 and 3, open on the page.
fn open_playlist(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.my_user_id = Some(ME);
        let playlist = session.media.playlists.entry(ME).or_default();
        playlist.tracks = vec![track(1), track(2), track(3)];
        playlist.total_count = 3;
        playlist.loaded = true;
        session.media.shared_media.playlist_user = Some(ME);
        cx.notify();
    });
    vcx.run_until_parked();
}

fn order(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<i32> {
    app.read_with(vcx, |app, _| {
        app.session().expect("session").media.playlists[&ME]
            .tracks
            .iter()
            .map(|t| t.file_id.0)
            .collect()
    })
}

#[gpui_kit::test]
fn dropping_a_track_on_another_row_reorders_the_list(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_playlist(&app, vcx);
    app.update_in(vcx, |app, _, cx| app.reorder_playlist_track(0, 2, cx));
    vcx.run_until_parked();
    assert_eq!(order(&app, vcx), [2, 3, 1]);
    // The same row and rows past the end leave the list alone.
    app.update_in(vcx, |app, _, cx| {
        app.reorder_playlist_track(1, 1, cx);
        app.reorder_playlist_track(0, 9, cx);
    });
    assert_eq!(order(&app, vcx), [2, 3, 1]);
}

#[gpui_kit::test]
fn the_track_menu_opens_and_delete_asks_before_removing(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_playlist(&app, vcx);
    app.update_in(vcx, |app, _, cx| {
        app.open_playlist_menu(FileId(2), point(px(300.), px(200.)), cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        let menu: Option<PlaylistMenu> = app.playback.playlist_menu;
        assert_eq!(menu.map(|m| m.file_id), Some(FileId(2)));
    });
    // Delete: the confirmation opens and nothing is removed yet.
    app.update_in(vcx, |app, _, cx| {
        app.playback.playlist_menu = None;
        app.ask_remove_playlist_track(FileId(2), cx);
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::RemovePlaylistTrack));
    });
    assert_eq!(order(&app, vcx), [1, 2, 3]);
    // Confirm: the track goes and the toast shows.
    app.update_in(vcx, |app, _, cx| app.confirm_remove_playlist_track(cx));
    vcx.run_until_parked();
    assert_eq!(order(&app, vcx), [1, 3]);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.connection.status_note, REMOVED_TOAST);
        assert_eq!(
            app.session().expect("session").media.playlists[&ME].total_count,
            2
        );
    });
}
