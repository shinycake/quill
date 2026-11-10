//! Recent actions on a demo `QuillApp` with the `ready-event-log` fixture:
//! the rows run oldest first with one day label per day, and the Filter box
//! keeps its choices until Save and drops them on Cancel.
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::dialogs::group_admin_settings::GroupSettingsView;
use crate::ui::event_log_rows::LogBody;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::event_log_filter::FilterCategory;
use quill::ids::ChatId;
use quill::state::ChatEventLogFetch;

const CHANNEL: ChatId = ChatId(13);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-event-log");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| {
        for _ in 0..2 {
            window.render_frame(cx);
        }
    });
    vcx.run_until_parked();
    (app, vcx)
}

#[gpui_kit::test]
fn rows_run_oldest_first_with_the_message_each_action_concerns(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, _| {
        let page = match app
            .session()
            .and_then(|session| session.groups.event_logs.get(&CHANNEL.0))
        {
            Some(ChatEventLogFetch::Loaded(page)) => page.clone(),
            other => panic!("log not loaded: {other:?}"),
        };
        let chronological: Vec<_> = page.events.iter().rev().cloned().collect();
        let entries = app.event_log_entries(&chronological);
        assert_eq!(entries.len(), chronological.len());
        assert!(entries.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert!(entries[0].day.is_some(), "the first row opens its day");
        let labels = entries.iter().filter(|entry| entry.day.is_some()).count();
        assert!(labels >= 1 && labels < entries.len());
        assert!(entries.iter().any(|entry| matches!(
            &entry.body,
            LogBody::Edited { original, new } if original == "Draft" && !new.is_empty()
        )));
        assert!(
            entries
                .iter()
                .all(|entry| entry.line.starts_with(&entry.actor))
        );
    });
}

#[gpui_kit::test]
fn the_filter_box_applies_on_save_and_drops_on_cancel(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_group_settings_view(CHANNEL, GroupSettingsView::RecentActions, cx);
        app.open_event_log_filter(CHANNEL, cx);
        let view = app.admin.group_settings_dialog.as_ref().map(|d| &d.view);
        assert!(matches!(
            view,
            Some(GroupSettingsView::RecentActionsFilter(_))
        ));
        let open = app.event_log_filter_box().expect("filter box");
        assert!(open.draft().all_actions() && open.draft().can_save());

        // Nothing ticked cannot be saved; one category can.
        app.edit_event_log_filter(cx, |draft, _| draft.set_all_actions(false));
        let open = app.event_log_filter_box().expect("filter box");
        assert!(!open.draft().can_save());
        app.edit_event_log_filter(cx, |draft, _| {
            draft.toggle_category(FilterCategory::Edited, true);
        });
        let open = app.event_log_filter_box().expect("filter box");
        assert!(open.draft().can_save());

        // Cancel: back on the list, the choices dropped.
        app.close_event_log_filter(cx);
        assert!(app.event_log_filter_box().is_none());
        let view = app.admin.group_settings_dialog.as_ref().map(|d| &d.view);
        assert_eq!(view, Some(&GroupSettingsView::RecentActions));
        app.open_event_log_filter(CHANNEL, cx);
        let open = app.event_log_filter_box().expect("filter box");
        assert!(open.draft().all_actions());
    });
}

/// Another admin's invite links, opened from the Group Settings box's
/// Invite Links, close with the box: it reopens on the group's own links.
#[gpui_kit::test]
fn another_admins_links_close_with_the_group_settings_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_group_settings_view(CHANNEL, GroupSettingsView::InviteLinks, cx);
        let Some(session) = app.session_mut() else {
            panic!("the demo session");
        };
        session.groups.invite_pages.set_admin_links(
            CHANNEL.0,
            quill::state::AdminLinksState {
                creator_user_id: 77,
                active: quill::state::InviteLinkFetch::Loading,
                revoked: quill::state::InviteLinkFetch::Loading,
                active_request: None,
                revoked_request: None,
            },
        );
        app.close_group_settings_dialog(cx);
        let open = app
            .session()
            .is_some_and(|s| s.groups.invite_pages.admin_links(CHANNEL.0).is_some());
        assert!(!open, "the admin's page went with the box");
    });
}

/// The box moved to another chat (a chat window's Edit, then Manage) takes
/// the first chat's pages with it, so reopening the first chat's Invite
/// Links shows its own links, and "Delete all revoked" has no admin.
#[gpui_kit::test]
fn the_box_for_another_chat_replaces_the_invite_pages(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let other = ChatId(CHANNEL.0 + 1);
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_group_settings_view(CHANNEL, GroupSettingsView::InviteLinks, cx);
        if let Some(dialog) = app.admin.group_settings_dialog.as_mut() {
            dialog.revoked_open = true;
            dialog.details_link = Some("https://t.me/+a".into());
        }
        let Some(session) = app.session_mut() else {
            panic!("the demo session");
        };
        session.groups.invite_pages.set_admin_links(
            CHANNEL.0,
            quill::state::AdminLinksState {
                creator_user_id: 77,
                active: quill::state::InviteLinkFetch::Loading,
                revoked: quill::state::InviteLinkFetch::Loading,
                active_request: None,
                revoked_request: None,
            },
        );
        // The same chat again keeps its pages.
        app.open_group_settings_view(CHANNEL, GroupSettingsView::InviteLinks, cx);
        assert!(
            app.session()
                .is_some_and(|s| s.groups.invite_pages.admin_links(CHANNEL.0).is_some())
        );
        app.open_group_settings_view(other, GroupSettingsView::Boosts, cx);
        app.close_group_settings_dialog(cx);
        app.open_group_settings_view(CHANNEL, GroupSettingsView::InviteLinks, cx);
        let pages = app.session().map(|s| s.groups.invite_pages.chat());
        assert_eq!(pages, Some(None), "no page of either chat is left");
        let dialog = app.admin.group_settings_dialog.as_ref().expect("the box");
        assert!(!dialog.revoked_open && dialog.details_link.is_none());
    });
}

/// Recent actions of another group shows that group's own query, not the
/// text typed in the previous one's box, however the page was reached.
#[gpui_kit::test]
fn the_search_box_follows_the_chat_whose_log_is_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let other = ChatId(CHANNEL.0 + 1);
    app.update_in(&mut vcx, |app, window, cx| {
        app.ensure_event_log_search(window, cx);
        if let Some(session) = app.session_mut() {
            session
                .groups
                .event_log_queries
                .insert(CHANNEL.0, "foo".to_string());
        }
        app.open_group_settings_view(CHANNEL, GroupSettingsView::RecentActions, cx);
        app.event_log_search_upkeep(window, cx);
        let shown = app
            .admin
            .event_log_search
            .as_ref()
            .map(|search| search.input.read(cx).value().to_string());
        assert_eq!(shown.as_deref(), Some("foo"));
        // Another group's box (Manage > Recent actions): its own, empty query.
        app.close_group_settings_dialog(cx);
        app.open_group_settings_view(other, GroupSettingsView::RecentActions, cx);
        app.event_log_search_upkeep(window, cx);
        let shown = app
            .admin
            .event_log_search
            .as_ref()
            .map(|search| search.input.read(cx).value().to_string());
        assert_eq!(shown.as_deref(), Some(""));
    });
}

/// The "load older events" mark is per chat and goes when the list starts
/// over, so a page of the same length asks again.
#[gpui_kit::test]
fn the_older_events_mark_is_cleared_when_the_list_starts_over(cx: &mut TestAppContext) {
    let (app, vcx) = new_app(cx);
    app.read_with(&vcx, |app, _| {
        app.admin.event_log_autoload.set(Some((CHANNEL, 3)));
        app.event_log_reset_list();
        assert_eq!(app.admin.event_log_autoload.get(), None);
    });
}
