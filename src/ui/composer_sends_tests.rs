//! The next send's attachments and options belong to the chat they were
//! chosen in: every way of opening another chat shows that chat's own
//! (docs/decisions/sweep-per-chat-composer.md). On a demo `QuillApp`
//! (gpui-kit's test support needs the `demo-capture` feature).
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::composer_sends::SendSlot;
use crate::ui::keybindings::bind_keys;
use crate::ui::scheduled::ScheduleTarget;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::wall_clock::WallClock;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, Entity, Focusable, TestAppContext, VisualContext,
    VisualTestContext, Window, px, size,
};
use quill::composer::{AttachmentKind, ComposerAttachment, ComposerScheduling};
use quill::ids::{AccountKey, ChatId, MessageId};
use quill::paid_messages::CanSend;
use quill::telegram::requests::SelfDestructSend;
use std::path::PathBuf;

/// The demo's open chat (a private chat) and two more from its list.
const A: ChatId = ChatId(11);
const B: ChatId = ChatId(12);
const C: ChatId = ChatId(13);
/// The one message in chat B.
const B_LAST: MessageId = MessageId(40);

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
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    (app, vcx)
}

fn photo(name: &str) -> ComposerAttachment {
    ComposerAttachment {
        path: std::env::temp_dir().join(name),
        kind: AttachmentKind::Photo,
        file_name: name.into(),
        spoiler: false,
        extras: Default::default(),
    }
}

/// In the open chat: a photo in the tray, a timer, caption above, a
/// schedule, previews off and "send silently".
fn choose_everything(app: &mut QuillApp) {
    app.update_pending_send(|send| {
        send.attachments.push(photo("a.jpg"));
        send.self_destruct = Some(SelfDestructSend::Timer(5));
        send.caption_above = true;
        send.scheduling = ComposerScheduling::SendWhenOnline;
        send.preview.disabled = true;
    })
    .expect("an open chat");
    app.toggle_composer_silent();
}

/// Nothing chosen in the open chat leaks into what it shows or sends.
fn assert_nothing_chosen(app: &QuillApp, path: &str) {
    assert!(
        app.pending_attachments().is_empty(),
        "{path}: attachment leaked"
    );
    let send = app.pending_send();
    assert!(send.self_destruct.is_none(), "{path}: self-destruct leaked");
    assert!(!send.caption_above, "{path}: caption-above leaked");
    let options = app.composer_send_options();
    assert!(
        matches!(options.scheduling, ComposerScheduling::None),
        "{path}: schedule leaked"
    );
    assert!(
        !options.link_preview_disabled,
        "{path}: preview choice leaked"
    );
    assert!(!options.disable_notification, "{path}: silent leaked");
}

fn open_chat(app: &QuillApp) -> Option<ChatId> {
    app.session().and_then(|session| session.open_chat)
}

#[gpui_kit::test]
fn a_chat_switch_shows_the_other_chats_choices_and_back_restores_them(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        choose_everything(app);
        app.select_listed_chat(B, window, cx);
        assert_eq!(open_chat(app), Some(B));
        assert_nothing_chosen(app, "chat list");
        // Back in A, its tray and choices wait where they were left, like
        // its text draft (the tray sits in the compose area, which
        // tdesktop keeps per chat).
        app.select_listed_chat(A, window, cx);
        assert_eq!(app.pending_attachments().len(), 1);
        let options = app.composer_send_options();
        assert!(matches!(
            options.scheduling,
            ComposerScheduling::SendWhenOnline
        ));
        assert!(options.link_preview_disabled);
        assert!(options.disable_notification);
        assert_eq!(
            app.pending_send().self_destruct,
            Some(SelfDestructSend::Timer(5))
        );
    });
}

type OpenPath = fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>);

#[gpui_kit::test]
fn every_way_of_opening_a_chat_shows_its_own_choices(cx: &mut TestAppContext) {
    let paths: [(&str, OpenPath); 5] = [
        ("chat list", |app, window, cx| {
            app.select_listed_chat(B, window, cx)
        }),
        ("search result", |app, window, cx| {
            app.select_search_chat(B, window, cx)
        }),
        ("search message", |app, window, cx| {
            app.select_search_message(B, B_LAST, window, cx)
        }),
        ("story area", |app, _, cx| {
            app.story_area_open_message(B, B_LAST, cx)
        }),
        // Deep links, Settings > Ask a Question and new private chats are
        // opened by the driver once TDLib answers: nothing in the UI runs.
        ("driver", |app, _, _| {
            app.demo_session.as_mut().expect("demo").open_chat(B);
        }),
    ];
    let (app, mut vcx) = new_app(cx);
    for (path, open) in paths {
        app.update_in(&mut vcx, |app, window, cx| {
            app.select_listed_chat(A, window, cx);
            if app.pending_attachments().is_empty() {
                choose_everything(app);
            }
            open(app, window, cx);
            assert_eq!(open_chat(app), Some(B), "{path}");
            assert_nothing_chosen(app, path);
        });
    }
}

#[gpui_kit::test]
fn a_send_starts_its_chat_over_and_keeps_the_silent_choice(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        app.update_pending_send(|send| {
            send.scheduling = ComposerScheduling::SendWhenOnline;
            send.preview.disabled = true;
        })
        .expect("an open chat");
        app.toggle_composer_silent();
        app.composer.update(cx, |input, cx| {
            input.set_value("see https://example.com", window, cx)
        });
        assert!(app.just_send_by_key(window, cx));
        // tdesktop clears the draft and its preview choice after a send.
        let options = app.composer_send_options();
        assert!(matches!(options.scheduling, ComposerScheduling::None));
        assert!(!options.link_preview_disabled);
        // The silent choice is the chat's, not the message's.
        assert!(options.disable_notification);
    });
}

#[gpui_kit::test]
fn silent_follows_each_chats_default_and_its_own_override(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let session = app.demo_session.as_mut().expect("demo");
        session.sync.set_default_silent(C.0, true);
        app.select_listed_chat(C, window, cx);
        assert!(
            app.composer_effective_silent(),
            "C sends silently by default"
        );
        app.toggle_composer_silent();
        assert!(!app.composer_effective_silent(), "turned on sound for C");
        app.select_listed_chat(B, window, cx);
        assert!(!app.composer_effective_silent(), "B has its own default");
        app.toggle_composer_silent();
        assert!(app.composer_effective_silent(), "B silenced");
        app.select_listed_chat(C, window, cx);
        assert!(!app.composer_effective_silent(), "C kept its choice");
        app.select_listed_chat(B, window, cx);
        assert!(app.composer_effective_silent(), "B kept its choice");
        // Toggled back to the default, nothing is left over.
        app.toggle_composer_silent();
        let account = app.session().expect("demo").account.clone();
        assert_eq!(app.composer_sends.silent_override(&account, B), None);
    });
}

#[gpui_kit::test]
fn a_chat_window_keys_its_choices_by_chat_not_by_window(cx: &mut TestAppContext) {
    let (app, vcx) = new_app(cx);
    let main: AnyWindowHandle = vcx.window_handle();
    drop(vcx);
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.select_listed_chat(A, window, cx);
            app.update_pending_attachments(|tray| tray.push(photo("a.jpg")))
                .expect("an open chat");
            app.open_chat_in_new_window(B, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    let other = cx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main.window_id())
        .expect("B's window");
    cx.update_window(other, |_, window, cx| window.render_frame(cx))
        .unwrap();
    app.update(cx, |app, _| {
        // B's window: B's (empty) tray, then a file attached there.
        app.set_window_scope(Some(B));
        assert_eq!(open_chat(app), Some(B));
        assert!(app.pending_attachments().is_empty());
        app.update_pending_attachments(|tray| tray.push(photo("b.jpg")))
            .expect("B is open in its window");
        // The main window still shows A's tray only.
        app.set_window_scope(None);
        assert_eq!(open_chat(app), Some(A));
        let names: Vec<_> = app
            .pending_attachments()
            .iter()
            .map(|attachment| attachment.file_name.clone())
            .collect();
        assert_eq!(names, ["a.jpg"]);
        // B's choice is stored under B, whichever window is drawing.
        let b = SendSlot {
            account: app.session().expect("demo").account.clone(),
            draft: quill::composer::DraftTopic::Chat.key(B.0),
        };
        assert_eq!(app.composer_sends.get(&b).attachments.len(), 1);
    });
}

/// Make the demo session another account: same chats, same chat ids, as
/// when two accounts share a group or talk to the same user.
fn become_account(app: &mut QuillApp, key: &AccountKey) {
    app.demo_session.as_mut().expect("demo").account = key.clone();
}

fn second_account() -> AccountKey {
    AccountKey::new("account-1").expect("a valid key")
}

fn slot_of(account: &AccountKey, chat: ChatId) -> SendSlot {
    SendSlot {
        account: account.clone(),
        draft: quill::composer::DraftTopic::Chat.key(chat.0),
    }
}

/// A real picture file in a scratch directory (picked paths must exist).
fn picture(name: &str) -> PathBuf {
    let dir = quill::scratch::process_dir("quill-composer-sends");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    image::RgbaImage::from_pixel(8, 4, image::Rgba([1, 2, 3, 255]))
        .save(&path)
        .unwrap();
    std::fs::canonicalize(path).unwrap()
}

fn attachment_at(path: PathBuf, kind: AttachmentKind) -> ComposerAttachment {
    ComposerAttachment {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
            .into(),
        path,
        kind,
        spoiler: false,
        extras: Default::default(),
    }
}

fn history_len(app: &QuillApp, chat: ChatId) -> usize {
    app.demo_session
        .as_ref()
        .and_then(|session| session.histories.get(&chat.0))
        .map_or(0, |history| history.messages.len())
}

#[gpui_kit::test]
fn another_account_never_sees_this_accounts_choices_for_the_same_chat_id(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let first = app.session().expect("demo").account.clone();
        let second = second_account();
        app.select_listed_chat(A, window, cx);
        choose_everything(app);
        // Account B on screen with the same chat id open: nothing of A's.
        become_account(app, &second);
        assert_eq!(open_chat(app), Some(A));
        assert_nothing_chosen(app, "other account");
        app.update_pending_attachments(|tray| tray.push(photo("b.jpg")))
            .expect("an open chat");
        // Back to A: its tray and options, and not B's file.
        become_account(app, &first);
        let names: Vec<_> = app
            .pending_attachments()
            .iter()
            .map(|attachment| attachment.file_name.clone())
            .collect();
        assert_eq!(names, ["a.jpg"]);
        let options = app.composer_send_options();
        assert!(matches!(
            options.scheduling,
            ComposerScheduling::SendWhenOnline
        ));
        assert!(options.disable_notification);
        // B (now in the background) kept its own.
        assert_eq!(
            app.composer_sends
                .get(&slot_of(&second, A))
                .attachments
                .len(),
            1
        );
        // B logs out: its choices go, A's stay.
        app.forget_account_sends(&second);
        assert!(
            app.composer_sends
                .get(&slot_of(&second, A))
                .attachments
                .is_empty()
        );
        assert_eq!(app.composer_sends.silent_override(&second, A), None);
        assert_eq!(app.pending_attachments().len(), 1);
        assert!(app.composer_effective_silent());
    });
}

#[gpui_kit::test]
fn a_cover_picked_after_a_chat_switch_lands_in_the_chat_it_was_picked_for(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let cover = picture("cover.png");
        let clip_a = std::env::temp_dir().join("a.mp4");
        let clip_b = std::env::temp_dir().join("b.mp4");
        app.select_listed_chat(A, window, cx);
        app.update_pending_attachments(|tray| {
            tray.push(attachment_at(clip_a.clone(), AttachmentKind::Video))
        });
        // "Edit Cover" in A; the picker answers once the user is in B,
        // whose tray has a video at the same place.
        let picked_for = app.tray_item(0).expect("A's video");
        app.select_listed_chat(B, window, cx);
        app.update_pending_attachments(|tray| {
            tray.push(attachment_at(clip_b.clone(), AttachmentKind::Video))
        });
        app.set_attachment_cover(&picked_for, &cover, cx);
        assert_eq!(
            app.pending_attachments()[0].extras.cover,
            None,
            "B untouched"
        );
        app.select_listed_chat(A, window, cx);
        assert_eq!(
            app.pending_attachments()[0].extras.cover.as_deref(),
            Some(cover.as_path()),
            "A got the cover"
        );
        // A video that left its place meanwhile gets nothing.
        let stale = app.tray_item(0).expect("A's video");
        app.update_pending_attachments(|tray| {
            tray[0] = attachment_at(clip_b.clone(), AttachmentKind::Video)
        });
        app.set_attachment_cover(&stale, &cover, cx);
        assert_eq!(app.pending_attachments()[0].extras.cover, None);
    });
}

#[gpui_kit::test]
fn a_photo_edit_saved_after_a_chat_switch_replaces_the_photo_it_was_opened_on(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = new_app(cx);
    let photo_a = picture("edit-a.png");
    let photo_b = picture("edit-b.png");
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        app.update_pending_attachments(|tray| {
            tray.push(attachment_at(photo_a.clone(), AttachmentKind::Photo))
        });
        app.open_photo_editor(0, cx);
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, window, cx| {
        assert!(app.viewer.photo_editor.is_some());
        app.select_listed_chat(B, window, cx);
        app.update_pending_attachments(|tray| {
            tray.push(attachment_at(photo_b.clone(), AttachmentKind::Photo))
        });
        app.save_photo_editor(cx);
        assert_eq!(app.pending_attachments()[0].path, photo_b, "B untouched");
        app.select_listed_chat(A, window, cx);
        let edited = &app.pending_attachments()[0];
        assert_ne!(edited.path, photo_a, "A has the edited copy");
        assert_eq!(edited.file_name, "edit-a.png");
        let _ = std::fs::remove_file(&edited.path);
    });
}

#[gpui_kit::test]
fn a_schedule_confirmed_after_a_chat_switch_goes_to_its_own_chat_and_sends_nothing(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        // The picker opens at 01:15, which went by once already as CDT.
        cx.set_global(WallClock::chicago_repeated_hour());
        app.select_listed_chat(A, window, cx);
        app.open_schedule_picker(ScheduleTarget::Composer, window, cx);
        // The "Schedule" shortcut sends once a time is confirmed.
        if let Some(picker) = app.composer_ui.schedule_picker.as_mut() {
            picker.send_after = true;
        }
        app.select_listed_chat(B, window, cx);
        app.composer
            .update(cx, |input, cx| input.set_value("for B", window, cx));
        let before = history_len(app, B);
        app.confirm_schedule_picker(window, cx);
        assert!(matches!(
            app.pending_send().scheduling,
            ComposerScheduling::None
        ));
        assert_eq!(history_len(app, B), before, "B's text was not sent");
        app.select_listed_chat(A, window, cx);
        assert_eq!(
            app.pending_send().scheduling,
            ComposerScheduling::SendAtDate(WallClock::CHICAGO_FALL_BACK + 900)
        );
    });
}

#[gpui_kit::test]
fn a_price_saved_after_a_chat_switch_goes_to_the_chat_the_box_opened_in(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        app.open_price_dialog(window, cx);
        let input = app
            .composer_ui
            .send_box
            .price
            .as_ref()
            .expect("the box")
            .input
            .clone();
        app.select_listed_chat(B, window, cx);
        input.update(cx, |input, cx| input.set_value("10", window, cx));
        assert!(app.submit_price_dialog(cx));
        assert_eq!(app.pending_send().paid_stars, 0, "B untouched");
        app.select_listed_chat(A, window, cx);
        assert_eq!(app.pending_send().paid_stars, 10);
    });
}

#[gpui_kit::test]
fn pay_sends_only_in_the_chat_and_account_it_was_asked_in(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        let session = app.demo_session.as_mut().expect("demo");
        session.payments.paid_messages.record_answer(
            A.0,
            CanSend::HasPaidMessages { stars: 25 },
            1,
        );
        // Another chat by the time Pay is pressed: nothing is sent there.
        assert!(app.paid_send_waits(A, "hello", cx));
        app.select_listed_chat(B, window, cx);
        let before = history_len(app, B);
        app.confirm_paid_send(window, cx);
        assert!(app.paid_messages.confirm.is_none());
        assert_eq!(history_len(app, B), before);
        // Another account with the same chat id open: nothing either.
        app.select_listed_chat(A, window, cx);
        let first = app.session().expect("demo").account.clone();
        assert!(app.paid_send_waits(A, "hello", cx));
        become_account(app, &second_account());
        let before = history_len(app, A);
        app.confirm_paid_send(window, cx);
        assert_eq!(history_len(app, A), before);
        // In the chat it was asked in, Pay sends.
        become_account(app, &first);
        assert!(app.paid_send_waits(A, "hello", cx));
        app.confirm_paid_send(window, cx);
        assert_eq!(history_len(app, A), before + 1);
    });
}

#[gpui_kit::test]
fn an_entry_back_to_nothing_chosen_is_dropped(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        assert_eq!(app.composer_sends.slot_count(), 0);
        app.update_pending_send(|send| send.preview.disabled = true);
        assert_eq!(app.composer_sends.slot_count(), 1);
        app.update_pending_send(|send| send.preview.disabled = false);
        assert_eq!(app.composer_sends.slot_count(), 0, "preview back on");
        app.update_pending_attachments(|tray| tray.push(photo("a.jpg")));
        app.remove_attachment(0, cx);
        assert_eq!(app.composer_sends.slot_count(), 0, "last file removed");
        app.update_pending_send(|send| send.scheduling = ComposerScheduling::SendWhenOnline);
        app.update_pending_send(|send| send.scheduling = ComposerScheduling::None);
        assert_eq!(app.composer_sends.slot_count(), 0, "schedule cleared");
    });
}

#[gpui_kit::test]
fn a_file_another_tray_holds_keeps_its_checks_when_one_tray_drops_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let shared = photo("shared.mp4");
        app.select_listed_chat(A, window, cx);
        app.update_pending_attachments(|tray| tray.push(shared.clone()));
        app.select_listed_chat(B, window, cx);
        app.update_pending_attachments(|tray| tray.push(shared.clone()));
        app.composer_ui
            .send_box
            .sound_checked
            .insert(shared.path.clone());
        app.clear_attachment(cx);
        assert!(
            app.composer_ui
                .send_box
                .sound_checked
                .contains(&shared.path),
            "A's tray still holds it"
        );
        app.select_listed_chat(A, window, cx);
        app.clear_attachment(cx);
        assert!(
            !app.composer_ui
                .send_box
                .sound_checked
                .contains(&shared.path)
        );
    });
}

#[gpui_kit::test]
fn a_file_picker_answered_after_a_chat_switch_fills_the_chat_it_was_opened_in(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let file = picture("late.png");
        app.select_listed_chat(A, window, cx);
        let asked_in = app.send_slot().expect("an open chat");
        app.select_listed_chat(B, window, cx);
        app.attach_picked_in(&asked_in, vec![file.clone()], true, cx);
        assert!(app.pending_attachments().is_empty(), "B stays empty");
        let in_a = app.composer_sends.get(&asked_in);
        assert_eq!(in_a.attachments.len(), 1, "A got the file");
        assert!(in_a.attachments.iter().all(|a| a.path == file));
        app.select_listed_chat(A, window, cx);
        assert_eq!(app.pending_attachments().len(), 1);
    });
}

#[gpui_kit::test]
fn a_file_picker_answer_for_a_gone_account_or_chat_is_dropped(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let file = picture("gone.png");
        app.select_listed_chat(A, window, cx);
        let account = app.send_slot().expect("an open chat").account;
        // No such chat in the account on screen.
        let no_chat = slot_of(&account, ChatId(9_999));
        app.attach_picked_in(&no_chat, vec![file.clone()], false, cx);
        assert!(app.composer_sends.get(&no_chat).attachments.is_empty());
        // An account that is neither on screen nor in the background.
        let other = slot_of(&second_account(), B);
        app.attach_picked_in(&other, vec![file], false, cx);
        assert!(app.composer_sends.get(&other).attachments.is_empty());
        assert_eq!(app.composer_sends.slot_count(), 0);
    });
}

#[gpui_kit::test]
fn the_choices_go_when_the_session_leaves_ready_without_a_logout(cx: &mut TestAppContext) {
    use quill::telegram::envelope::AuthorizationState as Auth;
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        choose_everything(app);
        // Still signed in: Ready to Ready keeps them.
        app.forget_sends_after_auth(&Auth::Ready, false);
        assert_eq!(app.pending_attachments().len(), 1);
        // A phone-number prompt that was never a Ready session keeps them.
        app.demo_session.as_mut().expect("demo").auth = Auth::WaitPhoneNumber;
        app.forget_sends_after_auth(&Auth::WaitTdlibParameters, false);
        assert_eq!(app.pending_attachments().len(), 1);
        // Ready to WaitPhoneNumber (session terminated): gone.
        app.forget_sends_after_auth(&Auth::Ready, false);
        assert_eq!(app.composer_sends.slot_count(), 0);
        assert_nothing_chosen(app, "after the session ended");
        // Ready to Closed (bridge lost): gone too.
        choose_everything(app);
        app.demo_session.as_mut().expect("demo").auth = Auth::Closed;
        app.forget_sends_after_auth(&Auth::Ready, false);
        assert_eq!(app.composer_sends.slot_count(), 0);
    });
}

#[gpui_kit::test]
fn a_pending_check_for_another_chats_file_does_not_block_a_send_here(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        let in_a = photo("checking-a.mp4");
        app.update_pending_send(|send| send.attachments.push(in_a.clone()))
            .expect("an open chat");
        let send_box = &mut app.composer_ui.send_box;
        send_box.sound_checking.insert(in_a.path.clone());
        send_box.thumbnailing.insert(in_a.path.clone(), 1);
        assert!(app.tray_videos_checking(), "A waits on its own file");
        assert!(app.tray_thumbnails_pending());
        app.select_listed_chat(B, window, cx);
        assert!(!app.tray_videos_checking(), "B is not held by A's probe");
        assert!(!app.tray_thumbnails_pending());
        app.select_listed_chat(A, window, cx);
        assert!(app.tray_videos_checking());
    });
}

#[gpui_kit::test]
fn a_session_closing_over_two_polls_ends_like_one_that_closed_at_once(cx: &mut TestAppContext) {
    use quill::telegram::envelope::AuthorizationState as Auth;
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(A, window, cx);
        choose_everything(app);
        // Poll 1: Ready to Closing keeps them (still on its way out).
        app.demo_session.as_mut().expect("demo").auth = Auth::Closing;
        app.forget_sends_after_auth(&Auth::Ready, false);
        assert_eq!(app.pending_attachments().len(), 1);
        // Poll 2: Closing to Closed drops them.
        app.demo_session.as_mut().expect("demo").auth = Auth::Closed;
        app.forget_sends_after_auth(&Auth::Closing, false);
        assert_eq!(app.composer_sends.slot_count(), 0);
        // Batched Ready to Closed drops them too.
        choose_everything(app);
        app.forget_sends_after_auth(&Auth::Ready, false);
        assert_eq!(app.composer_sends.slot_count(), 0);
    });
}
