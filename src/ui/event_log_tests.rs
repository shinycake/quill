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
        let view = app.admin.group_settings_dialog.as_ref().map(|d| d.view);
        assert_eq!(view, Some(GroupSettingsView::RecentActionsFilter));
        let open = app.admin.event_log_filter.as_ref().expect("filter box");
        assert!(open.draft().all_actions() && open.draft().can_save());

        // Nothing ticked cannot be saved; one category can.
        app.edit_event_log_filter(cx, |draft, _| draft.set_all_actions(false));
        let open = app.admin.event_log_filter.as_ref().expect("filter box");
        assert!(!open.draft().can_save());
        app.edit_event_log_filter(cx, |draft, _| {
            draft.toggle_category(FilterCategory::Edited, true);
        });
        let open = app.admin.event_log_filter.as_ref().expect("filter box");
        assert!(open.draft().can_save());

        // Cancel: back on the list, the choices dropped.
        app.close_event_log_filter(cx);
        assert!(app.admin.event_log_filter.is_none());
        let view = app.admin.group_settings_dialog.as_ref().map(|d| d.view);
        assert_eq!(view, Some(GroupSettingsView::RecentActions));
        app.open_event_log_filter(CHANNEL, cx);
        let open = app.admin.event_log_filter.as_ref().expect("filter box");
        assert!(open.draft().all_actions());
    });
}
