//! The repeat period of the schedule picker, the shortcut commands and the
//! automatic emoji replacement, on a demo `QuillApp` (gpui-kit's test
//! support needs the `demo-capture` feature). Nothing is sent: the demo
//! session has no sender.
#![cfg(feature = "demo-capture")]

use crate::ui::actions::{
    ArchiveOpenChat, JustSend, ScheduleSend, SendSilently, ShowAdminLog, ShowScheduledMessages,
    ToggleLinkPreview,
};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::{apply_custom_bindings, bind_keys};
use crate::ui::scheduled::{ScheduleTarget, picker_value};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::wall_clock::WallClock;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::composer::ComposerScheduling;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::settings::CustomKeybinding;
use quill::telegram::envelope::ChannelMemberStatus;

const OPEN: ChatId = ChatId(11);
const CHANNEL: ChatId = ChatId(13);
const DAY: i32 = 86_400;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| {
        for _ in 0..2 {
            window.render_frame(cx);
        }
    });
    vcx.run_until_parked();
    (app, vcx)
}

fn set_premium(app: &mut QuillApp, premium: bool) {
    app.demo_session
        .as_mut()
        .expect("demo session")
        .payments
        .premium_option = Some(premium);
}

/// Open the composer's picker at a valid time and confirm it with `repeat`
/// chosen, as the menu does.
fn schedule_with_repeat(
    app: &mut QuillApp,
    repeat: i32,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<QuillApp>,
) {
    cx.set_global(WallClock::chicago_repeated_hour());
    app.open_schedule_picker(ScheduleTarget::Composer, window, cx);
    // 01:20, which went by once already as CDT.
    let value = picker_value(WallClock::of(cx).now() + 900, cx).expect("a picker value");
    let picker = app.composer_ui.schedule_picker.as_mut().expect("picker");
    picker.repeat = repeat;
    let date = picker.date.state.clone();
    date.update(cx, |state, cx| state.set_date_time(value, window, cx));
    app.confirm_schedule_picker(window, cx);
}

#[gpui_kit::test]
fn the_repeat_choice_rides_the_schedule_for_premium(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        set_premium(app, true);
        schedule_with_repeat(app, 7 * DAY, window, cx);
        assert!(matches!(
            app.pending_send().scheduling,
            ComposerScheduling::SendAtDate(_)
        ));
        assert_eq!(app.pending_send().schedule_repeat, 7 * DAY);
        let options = app.composer_send_options();
        assert_eq!(options.schedule_repeat_period, 7 * DAY);
        // The choice is as one-shot as the schedule: clearing the schedule
        // drops it from the options.
        app.update_pending_send(|send| send.scheduling = ComposerScheduling::None);
        assert_eq!(app.composer_send_options().schedule_repeat_period, 0);
    });
}

#[gpui_kit::test]
fn without_premium_the_repeat_choice_is_dropped_and_the_button_is_locked(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        set_premium(app, false);
        app.open_schedule_picker(ScheduleTarget::Composer, window, cx);
        // The row is there, locked.
        assert!(app.schedule_repeat_row(0, cx).is_some());
        assert!(!app.repeat_allowed(ScheduleTarget::Composer));
        app.close_schedule_picker(cx);
        schedule_with_repeat(app, DAY, window, cx);
        assert_eq!(app.pending_send().schedule_repeat, 0);
    });
}

#[gpui_kit::test]
fn the_repeat_row_is_not_offered_when_rescheduling(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        set_premium(app, true);
        assert!(app.repeat_offered(ScheduleTarget::Composer));
        assert!(!app.repeat_offered(ScheduleTarget::RescheduleSelected));
        assert!(!app.repeat_offered(ScheduleTarget::Reschedule(quill::ids::MessageId(5))));
        // A forward of several messages cannot repeat; no forward at all
        // means there is nothing to repeat.
        assert!(!app.repeat_offered(ScheduleTarget::Share));
        app.open_schedule_picker(ScheduleTarget::RescheduleSelected, window, cx);
        assert!(app.schedule_repeat_row(0, cx).is_none());
    });
}

#[gpui_kit::test]
fn the_schedule_shortcut_asks_for_a_time_and_then_sends(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        cx.set_global(WallClock::chicago_repeated_hour());
        // Nothing to send: the command does not apply.
        assert!(!app.schedule_send_by_key(window, cx));
        app.composer
            .update(cx, |input, cx| input.set_value("later", window, cx));
        assert!(app.schedule_send_by_key(window, cx));
        let picker = app
            .composer_ui
            .schedule_picker
            .as_ref()
            .expect("the picker");
        assert!(picker.send_after);
        // Confirming the time sends, as tdesktop's schedule box does.
        let date = picker.date.state.clone();
        // 01:20, which went by once already as CDT.
        let value = picker_value(WallClock::of(cx).now() + 900, cx).expect("a picker value");
        date.update(cx, |state, cx| state.set_date_time(value, window, cx));
        app.confirm_schedule_picker(window, cx);
        assert!(app.composer.read(cx).value().is_empty(), "the message went");
    });
}

#[gpui_kit::test]
fn silent_send_and_just_send_need_something_to_send(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        assert!(!app.send_silently_by_key(window, cx));
        assert!(!app.just_send_by_key(window, cx));
        app.composer
            .update(cx, |input, cx| input.set_value("quiet", window, cx));
        assert!(app.send_silently_by_key(window, cx));
        assert!(app.composer.read(cx).value().is_empty());
        // The silence was for that one send.
        assert!(!app.composer_ui.send_silent_once);
        assert!(!app.composer_effective_silent());
        app.composer
            .update(cx, |input, cx| input.set_value("loud", window, cx));
        assert!(app.just_send_by_key(window, cx));
        assert!(app.composer.read(cx).value().is_empty());
        // The option itself reaches the wire options.
        app.composer_ui.send_silent_once = true;
        assert!(app.composer_send_options().disable_notification);
    });
}

#[gpui_kit::test]
fn the_scheduled_list_and_the_link_preview_toggle_follow_the_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        assert!(app.show_scheduled_by_key(window, cx));
        assert!(app.composer_ui.scheduled_dialog_open);
        app.composer_ui.scheduled_dialog_open = false;

        // No link in the text: nothing to toggle.
        assert!(!app.toggle_link_preview_by_key(window, cx));
        app.composer.update(cx, |input, cx| {
            input.set_value("see https://example.com", window, cx)
        });
        assert!(app.toggle_link_preview_by_key(window, cx));
        assert!(app.pending_send().preview.disabled);
        // Turning it back on, even though the text has changed.
        app.composer
            .update(cx, |input, cx| input.set_value("plain", window, cx));
        assert!(app.toggle_link_preview_by_key(window, cx));
        assert!(!app.pending_send().preview.disabled);
    });
}

#[gpui_kit::test]
fn archive_and_the_recent_actions_apply_to_the_open_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        assert!(app.archive_chat_by_key(window, cx));
        assert_eq!(app.connection.status_note(), tr(keys::ARCHIVED_ADDED));

        // A private chat has no recent actions; a channel I own does.
        assert!(!app.admin_log_by_key(window, cx));
        let session = app.demo_session.as_mut().expect("demo session");
        session.open_chat(CHANNEL);
        session.chats.get_mut(&CHANNEL.0).unwrap().my_member_status =
            Some(ChannelMemberStatus::Creator);
        assert!(app.admin_log_by_key(window, cx));
        assert!(
            app.session()
                .is_some_and(|s| s.users_state.open_info_panel.is_some())
        );
    });
}

#[gpui_kit::test]
fn the_new_commands_are_bound_to_chords_the_user_picks(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // They ship without chords, as in tdesktop.
    for id in [
        "archive-chat",
        "show-scheduled",
        "just-send",
        "send-silently",
        "schedule-send",
        "toggle-link-preview",
        "record-round",
        "show-admin-log",
    ] {
        let action = crate::ui::keybindings::REBINDABLE_ACTIONS
            .iter()
            .find(|a| a.id == id)
            .expect(id);
        assert!(action.defaults.is_empty(), "{id}");
    }
    vcx.update(|_, cx| {
        apply_custom_bindings(
            cx,
            &[CustomKeybinding {
                id: "archive-chat".into(),
                keystroke: "ctrl-alt-a".into(),
            }],
        );
    });
    vcx.simulate_keystrokes("ctrl-alt-a");
    let note = app.read_with(vcx, |app, _| app.connection.status_note().to_string());
    assert_eq!(
        note,
        tr(keys::ARCHIVED_ADDED),
        "the chosen chord archived the open chat"
    );
    // The dispatchers exist for every command (a missing handler would
    // leave the chord dead).
    vcx.update(|window, cx| {
        for action in [
            Box::new(ArchiveOpenChat) as Box<dyn gpui_kit::Action>,
            Box::new(JustSend),
            Box::new(ScheduleSend),
            Box::new(SendSilently),
            Box::new(ShowAdminLog),
            Box::new(ShowScheduledMessages),
            Box::new(ToggleLinkPreview),
        ] {
            window.dispatch_action(action, cx);
        }
    });
}

#[gpui_kit::test]
fn typed_emoticons_become_emoji_and_backspace_takes_them_back(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let focus_composer = |vcx: &mut VisualTestContext| {
        vcx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.composer.update(cx, |input, cx| input.focus(window, cx));
            });
        });
    };
    focus_composer(vcx);
    vcx.simulate_input(":-)");
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, "\u{1f642}", "the emoticon became an emoji");
    // Backspace right after puts the typed text back.
    vcx.simulate_keystrokes("backspace");
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, ":-)");
    vcx.simulate_keystrokes("backspace");
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, ":-", "a second Backspace deletes as usual");

    // With the setting off nothing is replaced, dashes included.
    app.update_in(vcx, |app, window, cx| {
        app.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
        app.chat_prefs.replace_emoji = false;
    });
    focus_composer(vcx);
    vcx.simulate_input(":-) --");
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, ":-) --");
}

#[gpui_kit::test]
fn nothing_is_replaced_inside_code(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.composer.update(cx, |input, cx| input.focus(window, cx));
        // The next typed text is monospace.
        app.toggle_composer_tag(quill::composer_doc::ComposerTag::Code, window, cx);
    });
    vcx.simulate_input("<3");
    let (text, spans) = app.read_with(vcx, |app, cx| {
        let state = app.composer.read(cx);
        (
            state.value().to_string(),
            crate::ui::composer_field::field_doc(state).spans,
        )
    });
    assert_eq!(text, "<3", "{spans:?}");
}

/// A photo still being sent as the newest message of the open chat.
fn pending_photo(app: &mut QuillApp, caption: &str) -> quill::ids::MessageId {
    use quill::telegram::envelope::{MessageContent, PhotoContent};
    let id = quill::ids::MessageId(90_000_000_001);
    let history = app
        .demo_session
        .as_mut()
        .expect("demo session")
        .histories
        .get_mut(&OPEN.0)
        .expect("the open chat's history");
    let mut message = history
        .messages
        .values()
        .next_back()
        .expect("a message")
        .clone();
    message.id = id;
    message.is_outgoing = true;
    message.pending = true;
    message.media_album_id = 0;
    message.forward_info = None;
    message.content = MessageContent::Photo(PhotoContent {
        caption: caption.into(),
        caption_entities: Vec::new(),
        show_caption_above_media: false,
        sizes: Vec::new(),
        is_secret: false,
        has_spoiler: false,
        has_stickers: false,
        minithumbnail: None,
    });
    history.messages.insert(id.0, message);
    id
}

#[gpui_kit::test]
fn up_opens_the_caption_of_a_media_message_still_being_sent(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let id = app.update_in(vcx, |app, window, cx| {
        let id = pending_photo(app, "on its way");
        app.composer.update(cx, |input, cx| input.focus(window, cx));
        id
    });
    vcx.simulate_keystrokes("up");
    let (edit, text) = app.read_with(vcx, |app, cx| {
        (
            app.composer_ui.pending_edit.clone(),
            app.composer.read(cx).value().to_string(),
        )
    });
    let edit = edit.expect("Up opened an edit");
    assert_eq!(edit.message_id, id);
    assert_eq!(text, "on its way");
}

#[gpui_kit::test]
fn up_keeps_scrolling_while_a_reply_is_pending(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        pending_photo(app, "on its way");
        app.composer_ui
            .set_reply(Some(quill::composer::ComposerReplyTo::new(
                OPEN,
                quill::ids::MessageId(1),
            )));
        app.composer.update(cx, |input, cx| input.focus(window, cx));
    });
    vcx.simulate_keystrokes("up");
    assert!(app.read_with(vcx, |app, _| app.composer_ui.pending_edit.is_none()));
}
