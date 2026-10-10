//! Screenshot demos: groups.

use crate::ui::app::QuillApp;
use crate::ui::chat::apply_ready_slow_mode;
use crate::ui::chat_list::apply_ready_chat_avatars;
use crate::ui::contacts::apply_ready_contacts;
use crate::ui::folder_demo::{apply_ready_folder_invite, apply_ready_folders_share};
use crate::ui::folders::apply_ready_folders;
use crate::ui::groups_forum::{apply_ready_forum_topics, apply_ready_topic_post};
use crate::ui::notification_settings::apply_ready_folder_badges;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::subsection_tabs::apply_ready_bot_topics;
use crate::ui::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::InfoPanelTarget;
use quill::subsection_tabs::SubsectionTabsMode;
use std::sync::atomic::Ordering;

const NOTE_READY_FOLDERS_SHARE: &str = "screenshot demo — Share Folder dialog: invite links over the folder fixture (injected, no live Telegram)";
const NOTE_READY_FOLDERS_SIDEBAR: &str = "screenshot demo — folders in the left column (Tabs on the left) with icons (injected, no live Telegram)";

register_demos![
    // Slice A6: the block-user confirm dialog open for Ada ("Are you
    // sure you want to block Ada Lovelace?", red Block button — TGX
    // `BlockUserConfirm`) over the contacts fixture (injected, no live
    // Telegram).
    DemoSpec::chats("ready-block-user", "screenshot demo — block user confirm")
        .setup(QuillApp::demo_ready_block_user),
    // Parity slice: chat-list avatars (injected, no live Telegram) — the
    // chat list mixes photo avatars (private chat A, the demo channel)
    // and colored-initial fallbacks (private chat B, a basic group, the
    // discussion supergroup); the demo channel (id 13) is open with its
    // header photo, @username, subscriber count, description snippet, and
    // a "Discuss" link to the injected discussion group (id 16).
    DemoSpec::chats(
        "ready-chat-avatars",
        "screenshot demo — chat avatars & channel header (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_chat_avatars),
    // Contacts demo (injected, no live Telegram): the sidebar shows the
    // **Contacts** tab (three injected contacts: Ada online, Zed last
    // seen within a week, Noor recently) and the user info panel is open
    // for Zed (bio from an injected `userFullInfo`) with the **Add
    // contact** affordance (Phase 6).
    DemoSpec::chats("ready-contacts", "screenshot demo — contacts & profile")
        .setup(QuillApp::demo_ready_contacts),
    // Slice A6: contacts management — the **Contacts** tab with the
    // settings section (sync toggle on, Import contacts…, Delete
    // synced contacts…, notice) and the user info panel open for Ada
    // (a contact) showing **Delete contact** + **Block user**
    // (injected, no live Telegram).
    DemoSpec::chats(
        "ready-contacts-manage",
        "screenshot demo — contacts management"
    )
    .setup(QuillApp::demo_ready_contacts_manage),
    // Folder tabs with unread-chat counters, one of them muted-only
    // (parity cluster notify-os, "Include muted chats in folder counters").
    DemoSpec::chats(
        "ready-folder-badges",
        "screenshot demo — notification settings and folder counters (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_notify_settings(
        NotifySettingsDemo::FolderBadges,
        window,
        cx
    )),
    // Phase 7.1: folder tabs (injected `updateChatFolders` + folder
    // positions) with the non-default "News" folder selected, so the chat
    // list shows only that folder's chats.
    DemoSpec::chats(
        "ready-folders",
        "screenshot demo — chat folder tabs (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_folders),
    // Shareable folders slice: "Add folder" for an addlist link.
    DemoSpec::chats(
        "ready-folders-add-link",
        "screenshot demo — Add folder for an addlist link (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_folders_add_link),
    // Shareable folders slice: folder editor with the icon picker.
    DemoSpec::chats(
        "ready-folders-icons",
        "screenshot demo — folder editor with the icon picker (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_folders_icons),
    // Parity slice: folder manage dialog over the ReadyFolders fixture
    // (injected, no live Telegram).
    DemoSpec::chats(
        "ready-folders-manage",
        "screenshot demo — folder management (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_folders_manage),
    // Shareable folders slice: Share Folder dialog: invite links over the folder fixture.
    DemoSpec::chats("ready-folders-share", NOTE_READY_FOLDERS_SHARE)
        .setup(QuillApp::demo_ready_folders_share),
    // Shareable folders slice: folders in the left column ("Tabs on the left") with icons.
    DemoSpec::chats("ready-folders-sidebar", NOTE_READY_FOLDERS_SIDEBAR)
        .setup(QuillApp::demo_ready_folders_sidebar),
    // Forum-topics demo (injected, no live Telegram): a forum supergroup
    // whose `updateSupergroup` marks it a forum and whose `getForumTopics`
    // response seeds three topics (General pinned + unread, Announcements
    // with a last-message preview, Random closed), shown as the topic
    // list (Phase 5.1).
    DemoSpec::chats("ready-forum-topics", "screenshot demo — forum topics")
        .setup(QuillApp::demo_ready_forum_topics),
    // Notification settings (parity cluster notify-os): the defaults
    // dialog with the flash/bounce switch, the Events section and the
    // folder-counter switch (injected, English fixtures).
    DemoSpec::chats(
        "ready-notify-os",
        "screenshot demo — notification settings and folder counters (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_notify_settings(
        NotifySettingsDemo::NotifyOs,
        window,
        cx
    )),
    // Phase A1: slow-mode enforcement (injected, no live Telegram) — a
    // dedicated supergroup (id 17) with `slow_mode_delay: 30` and
    // `slow_mode_delay_expires_in: 25.0`, the viewer a plain member (no
    // bypass), opened with two messages. The composer shows the
    // "Slow mode · wait Ns" countdown and blocks sends until expiry.
    DemoSpec::chats(
        "ready-slow-mode",
        "screenshot demo — slow-mode enforcement (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_slow_mode),
    // Topic-posting demo (injected, no live Telegram): the forum's General
    // topic is open with an injected two-message history and the composer
    // enabled — posting routes `sendMessage` with
    // `topic_id = messageTopicForum` (parity slice 4).
    DemoSpec::chats(
        "ready-topic-post",
        "screenshot demo — posting to a forum topic"
    )
    .setup(QuillApp::demo_ready_topic_post),
    // Subsection-tabs demos (injected): a bot with topics
    // (`userTypeBot.has_topics`) with the tabs on Top / Bottom / Left.
    DemoSpec::chats("ready-bot-topics", "screenshot demo — bot topic tabs")
        .setup(|app, _, _| app.demo_bot_topics(SubsectionTabsMode::Top)),
    DemoSpec::chats(
        "ready-bot-topics-bottom",
        "screenshot demo — bot topic tabs"
    )
    .setup(|app, _, _| app.demo_bot_topics(SubsectionTabsMode::Bottom)),
    DemoSpec::chats("ready-bot-topics-left", "screenshot demo — bot topic tabs")
        .setup(|app, _, _| app.demo_bot_topics(SubsectionTabsMode::Left)),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum NotifySettingsDemo {
    NotifyOs,
    FolderBadges,
}

impl QuillApp {
    fn demo_bot_topics(&mut self, mode: SubsectionTabsMode) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_topics(session, &self.demo_ui.sink, &self.demo_ui.seq, mode);
        }
        self.connection.status_note = "screenshot demo — bot topic tabs".into();
    }

    fn demo_ready_block_user(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice A6: block-user confirm dialog open for Ada (31) over the
        // contacts fixture — TGX `BlockUserConfirm`. Ada's info panel
        // is open behind the dialog so the demo is consistent.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_contacts(session, &self.demo_ui.sink, &self.demo_ui.seq);
            session.open_info_panel = Some(InfoPanelTarget::User(31));
        }
        self.chat_list.contacts_tab_open = true;
        self.admin.group_confirm_dialog = Some(GroupConfirmDialog {
            chat_id: ChatId(0),
            action: GroupConfirmAction::BlockContact {
                user_id: 31,
                block: true,
            },
        });
        self.connection.status_note = "screenshot demo — block user confirm".into();
    }

    fn demo_ready_chat_avatars(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Parity slice: chat-list avatars + channel header extras fixture.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_avatars(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "chat avatars & channel header".into();
    }

    fn demo_ready_contacts(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_contacts(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.chat_list.contacts_tab_open = true;
        self.connection.status_note = "screenshot demo — contacts tab + user info panel".into();
    }

    fn demo_ready_contacts_manage(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice A6: contacts management — the fixture's panel is moved
        // to Ada (31, a contact) so Delete contact + Block user show,
        // and a notice proves the settings-section wiring.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_contacts(session, &self.demo_ui.sink, &self.demo_ui.seq);
            session.open_info_panel = Some(InfoPanelTarget::User(31));
            session.contacts_notice = Some("Imported 2 contacts.".to_string());
        }
        self.chat_list.contacts_tab_open = true;
        self.connection.status_note = "screenshot demo — contacts management".into();
    }

    fn demo_ready_folders(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_folders(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        // Select the non-default "News" folder so the screenshot shows
        // the filtered chat list.
        self.folders.tab = Some(2);
        self.connection.status_note = "screenshot demo — folder tabs · News folder".into();
    }

    fn demo_ready_folders_add_link(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Shareable folders slice: "Add folder" for an addlist link.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_folders_share(session, &self.demo_ui.sink, &self.demo_ui.seq);
            apply_ready_folder_invite(session, "https://t.me/addlist/Xk3pQ9aBn2");
        }
        let mut dialog = FolderInviteDialog::new("https://t.me/addlist/Xk3pQ9aBn2".into());
        dialog.selected = [13, 16].into_iter().collect();
        dialog.seeded = true;
        self.folders.invite = Some(dialog);
        self.connection.status_note = "screenshot demo — Add folder by link".into();
    }

    fn demo_ready_folders_icons(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Shareable folders slice: the folder editor with the icon picker.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_folders_share(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        let mut dialog = FolderEditorDialog::new(window, cx, None);
        dialog.editor.name = "Family".into();
        dialog.editor.icon_name = Some("Home".into());
        dialog.editor.included = [11, 12].into_iter().collect();
        dialog
            .name_input
            .update(cx, |input, cx| input.set_value("Family", window, cx));
        self.folders.editor = Some(dialog);
        self.connection.status_note = "screenshot demo — folder icon picker".into();
    }

    fn demo_ready_folders_manage(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Parity slice: manage dialog over the same folder fixture.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_folders_share(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        // Open the manage dialog over the folder fixture.
        self.folders.manage_open = true;
        self.connection.status_note = "screenshot demo — folder management dialog".into();
    }

    fn demo_ready_folders_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Shareable folders slice: the Share Folder dialog over four folders.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_folders_share(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_folder_share(1, window, cx);
        self.connection.status_note = "screenshot demo — Share Folder".into();
    }

    fn demo_ready_folders_sidebar(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Shareable folders slice: the folders in a left column with icons.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_folders_share(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.appearance.folder_tabs_view = quill::folder_icons::FolderTabsView::Left;
        self.folders.tab = Some(2);
        self.connection.status_note = "screenshot demo — folders on the left".into();
    }

    fn demo_ready_forum_topics(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_forum_topics(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — forum topics list".into();
    }

    fn demo_ready_slow_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Phase A1: slow-mode enforcement fixture — the composer shows the
        // countdown and blocks sends until it expires.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_slow_mode(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.composer.update(cx, |input, cx| {
            input.set_value("this send will be blocked by slow mode…", window, cx);
        });
        self.connection.status_note =
            "slow-mode enforcement — sends blocked until the timer expires".into();
    }

    fn demo_ready_topic_post(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_topic_post(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.composer.update(cx, |input, cx| {
            input.set_value("Posting into the General topic…", window, cx);
        });
        self.connection.status_note = "screenshot demo — posting to a forum topic".into();
    }

    fn demo_notify_settings(
        &mut self,
        demo: NotifySettingsDemo,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_folders(session, &self.demo_ui.sink, &self.demo_ui.seq);
            apply_ready_folder_badges(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        if matches!(demo, NotifySettingsDemo::NotifyOs) {
            self.notify.notification_defaults_open = true;
        }
        self.connection.status_note =
            "screenshot demo — notification settings · folder counters".into();
    }
}
