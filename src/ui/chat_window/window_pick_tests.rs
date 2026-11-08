//! A chat window's media panel is its own: the main window's story reply,
//! group title emoji and Business sticker step neither hide it nor take
//! its picks (round 12, area B, findings 4 and 5). Its `#` suggestions
//! are the account's, shared with every window (finding 7).

use super::tests::{CHANNEL, Harness, OTHER, new_app};
use crate::ui::app::QuillApp;
use crate::ui::business::View;
use crate::ui::media_panel::PanelTab;
use gpui_kit::{AnyWindowHandle, AppContext, Context, TestAppContext};
use quill::ids::{ChatId, FileId};

fn scoped<R>(
    h: &Harness,
    chat: Option<ChatId>,
    cx: &mut TestAppContext,
    f: impl FnOnce(&mut QuillApp, &mut Context<QuillApp>) -> R,
) -> R {
    h.app
        .update(cx, |app, cx| app.in_window_scope(chat, cx, f))
        .expect("the window's state")
}

fn in_main(
    h: &Harness,
    cx: &mut TestAppContext,
    f: impl FnOnce(&mut QuillApp, &mut gpui_kit::Window, &mut Context<QuillApp>),
) {
    let app = h.app.clone();
    cx.update_window(h.main, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(None, cx, |app, cx| f(app, window, cx));
        });
    })
    .expect("the main window");
}

fn open_chat_window(h: &Harness, cx: &mut TestAppContext) -> AnyWindowHandle {
    let window = h.open_in_new_window(OTHER, cx);
    h.settle(cx);
    window
}

#[gpui_kit::test]
fn the_main_windows_title_emoji_leaves_a_chat_windows_panel_alone(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = open_chat_window(&h, cx);
    in_main(&h, cx, |app, window, cx| {
        app.open_group_edit_box(CHANNEL, window, cx);
        if let Some(edit) = app.group_edit_box_mut() {
            edit.title_emoji = true;
        }
        app.open_media_panel(PanelTab::Emoji, cx);
    });
    assert!(
        scoped(&h, None, cx, |app, _| app.group_title_emoji_active()),
        "the main window's panel serves the title"
    );
    let over = scoped(&h, Some(OTHER), cx, |app, cx| {
        app.open_media_panel(PanelTab::Emoji, cx);
        app.media_panel_over_composer()
    });
    assert!(over, "the chat window's panel shows over its composer");
}

#[gpui_kit::test]
fn a_sticker_picked_in_a_chat_window_skips_the_business_start_page(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = open_chat_window(&h, cx);
    in_main(&h, cx, |app, window, cx| {
        app.open_business(window, cx);
        app.business_go(View::StartPage, window, cx);
        app.business_go(View::StickerPick, window, cx);
    });
    let taken = scoped(&h, Some(OTHER), cx, |app, cx| {
        app.route_business_start_page_sticker(FileId(1), cx)
    });
    assert!(!taken, "the chat window's pick is not the Start Page's");
    let view = scoped(&h, None, cx, |app, _| app.business_view());
    assert_eq!(view, Some(View::StickerPick), "the step still waits");
    let taken = scoped(&h, None, cx, |app, cx| {
        app.route_business_start_page_sticker(FileId(1), cx)
    });
    assert!(taken, "a pick in the main window still goes to the page");
}

/// A sticker sent from a chat window flies there only: the main window's
/// conversation draws no copy of it in the other window's coordinates.
#[gpui_kit::test]
fn a_sticker_flies_only_in_the_window_it_was_sent_from(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = open_chat_window(&h, cx);
    crate::ui::send_fly::reset();
    let image = std::sync::Arc::new(gpui_kit::RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(image::RgbaImage::new(2, 2)),
    ])));
    crate::ui::send_fly::arm_for_test(Some(OTHER), OTHER, gpui_kit::ImageSource::Render(image));
    let main = scoped(&h, None, cx, |app, cx| app.send_fly_overlay(cx).is_some());
    let window = scoped(&h, Some(OTHER), cx, |app, cx| {
        app.send_fly_overlay(cx).is_some()
    });
    crate::ui::send_fly::reset();
    assert!(!main, "the main window draws no flight");
    assert!(window, "the sending window does");
}

fn hashtags(h: &Harness, chat: Option<ChatId>, cx: &mut TestAppContext) -> Vec<String> {
    scoped(h, chat, cx, |app, _| {
        app.recent_hashtags()
            .map(|list| list.tags.iter().map(|(tag, _)| tag.clone()).collect())
            .unwrap_or_default()
    })
}

/// Every window offers the hashtags sent from any of them, and another
/// account keeps a list of its own (round 12, area B, finding 7).
#[gpui_kit::test]
fn recent_hashtags_are_the_accounts_and_every_windows(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = open_chat_window(&h, cx);
    let tag = "r12bwindowtag";
    scoped(&h, Some(OTHER), cx, |app, _| {
        app.remember_sent_hashtags(&format!("sent from the window #{tag}"));
    });
    scoped(&h, None, cx, |app, _| {
        app.remember_sent_hashtags("and from the main window #r12bmaintag");
    });
    for window in [None, Some(OTHER)] {
        let shown = hashtags(&h, window, cx);
        assert!(
            shown.iter().any(|t| t == tag) && shown.iter().any(|t| t == "r12bmaintag"),
            "{window:?} offers both: {shown:?}"
        );
    }
    let saved = h
        .app
        .read_with(cx, |app, _| app.session().map(|s| s.account.clone()));
    let saved = saved.expect("an account on screen");
    let on_disk = quill::settings::safe_app_root()
        .map(|root| quill::settings::AccountPaths::for_root(&root, &saved))
        .map(|paths| quill::settings::load_recent_hashtags(&paths))
        .unwrap_or_default();
    assert!(
        on_disk.tags.iter().any(|(t, _)| t == tag),
        "saved in the account's own folder"
    );
    h.app.update(cx, |app, _| {
        if let Some(session) = app.session_mut() {
            session.account = quill::ids::AccountKey("r12b-other-account".into());
        }
    });
    assert!(
        !hashtags(&h, None, cx).iter().any(|t| t == tag),
        "another account does not see them"
    );
}
