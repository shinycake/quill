//! Keys, actions and layers each window answers for itself (round 11,
//! area B): the app's commands work from a chat window, the main window's
//! menus never take a chat window's keys, and a window's GIF panel is its
//! own.

use super::tests::{Harness, OTHER, new_app};
use crate::ui::actions::LockApp;
use crate::ui::app::QuillApp;
use crate::ui::media_panel::PanelTab;
use gpui_kit::{AnyWindowHandle, AppContext, Context, TestAppContext, point, px};
use quill::ids::ChatId;

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

fn render(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, _| window.refresh())
        .expect("the window");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn lock_from_a_chat_window_locks_quill(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.settle(cx);
    h.app
        .update(cx, |app, _| app.account.passcode.enabled = true);
    render(chat_window, cx);
    cx.update_window(chat_window, |_, window, cx| {
        window.dispatch_action(Box::new(LockApp), cx);
    })
    .expect("the chat window");
    cx.run_until_parked();
    let locked = h
        .app
        .read_with(cx, |app, _| app.account.passcode.is_locked());
    assert!(locked, "Cmd/Ctrl+L in a chat window locks the app");
}

#[gpui_kit::test]
fn a_main_window_menu_leaves_a_chat_windows_keys_alone(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.settle(cx);
    h.app.update(cx, |app, _| {
        app.chat_list.archive_menu = Some(point(px(40.), px(80.)));
    });
    assert!(scoped(&h, None, cx, |app, _| app.context_menu_open()));
    assert!(
        !scoped(&h, Some(OTHER), cx, |app, _| app.context_menu_open()),
        "the chat window draws no chat-list menu"
    );
    // Escape in the chat window spends nothing on the main window's menu.
    let spent = scoped(&h, Some(OTHER), cx, |app, cx| app.dismiss_context_menus(cx));
    assert!(!spent);
    let still = h
        .app
        .read_with(cx, |app, _| app.chat_list.archive_menu.is_some());
    assert!(still, "the main window's menu stays open");
    assert!(scoped(&h, None, cx, |app, cx| app.dismiss_context_menus(cx)));
}

#[gpui_kit::test]
fn each_window_has_its_own_gif_panel(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.settle(cx);
    let gate = |h: &Harness, cx: &mut TestAppContext| {
        h.app.read_with(cx, |app, _| {
            app.session().is_some_and(|s| s.stickers.gifs.open)
        })
    };
    scoped(&h, Some(OTHER), cx, |app, cx| {
        app.open_media_panel(PanelTab::Gifs, cx)
    });
    assert!(gate(&h, cx), "the window's GIFs load");
    assert!(
        !scoped(&h, None, cx, |app, _| app.gif_panel_open()),
        "the main window shows no GIF panel"
    );
    // Escape in the main window has no GIF panel to close.
    assert!(!scoped(&h, None, cx, |app, cx| app.close_media_panel(cx)));
    scoped(&h, None, cx, |app, cx| {
        app.open_media_panel(PanelTab::Gifs, cx)
    });
    scoped(&h, None, cx, |app, cx| app.close_media_panel(cx));
    assert!(gate(&h, cx), "the chat window's panel still shows GIFs");
    scoped(&h, Some(OTHER), cx, |app, cx| app.close_media_panel(cx));
    assert!(!gate(&h, cx), "no window shows GIFs");
}

#[gpui_kit::test]
fn each_window_scrolls_its_own_subsection_tabs(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    h.settle(cx);
    let moved = point(px(-120.), px(0.));
    scoped(&h, Some(OTHER), cx, |app, _| {
        app.history.subsection_tabs.strip.set_offset(moved);
        app.history
            .subsection_tabs
            .column
            .set_offset(point(px(0.), px(-40.)));
    });
    let main = scoped(&h, None, cx, |app, _| {
        (
            app.history.subsection_tabs.strip.offset(),
            app.history.subsection_tabs.column.offset(),
        )
    });
    assert_eq!(main, (point(px(0.), px(0.)), point(px(0.), px(0.))));
    let window = scoped(&h, Some(OTHER), cx, |app, _| {
        app.history.subsection_tabs.strip.offset()
    });
    assert_eq!(window, moved, "the chat window kept its own strip offset");
}

#[gpui_kit::test]
fn each_saved_view_has_its_own_history_key(cx: &mut TestAppContext) {
    use quill::state::{SavedSublistView, SavedTagSearch, TopicHistory};
    use quill::telegram::envelope::ReactionType;
    let h = new_app(cx);
    let scopes = h.app.update(cx, |app, _| {
        let mut scopes = Vec::new();
        let Some(session) = app.demo_session.as_mut() else {
            return scopes;
        };
        // The open chat becomes Saved Messages.
        session.my_user_id = session.open_chat.map(|chat| chat.0);
        scopes.push(app.saved_view_scope());
        for topic_id in [5, 6] {
            if let Some(session) = app.demo_session.as_mut() {
                session.threads.saved.sublist = Some(SavedSublistView {
                    topic_id,
                    history: TopicHistory::default(),
                    page: quill::state::SavedPage::Idle,
                });
            }
            scopes.push(app.saved_view_scope());
        }
        for emoji in ["\u{1F44D}", "\u{2764}"] {
            if let Some(session) = app.demo_session.as_mut() {
                session.threads.saved.tag_search = Some(SavedTagSearch {
                    topic_id: 6,
                    tag: ReactionType::Emoji {
                        emoji: emoji.to_string(),
                    },
                    history: TopicHistory::default(),
                    loaded: true,
                    page: quill::state::SavedPage::Idle,
                });
            }
            scopes.push(app.saved_view_scope());
        }
        scopes
    });
    assert_eq!(scopes.len(), 5);
    assert_eq!(
        scopes.first(),
        Some(&None),
        "the plain Saved history keeps key 0"
    );
    let views: Vec<i64> = scopes.iter().skip(1).flatten().copied().collect();
    assert_eq!(views.len(), 4, "every sublist and tag view has a scope");
    let mut distinct = views.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 4, "no two views share a history key");
    assert!(
        views.iter().all(|scope| *scope < 0),
        "never a reply thread's id"
    );
}
