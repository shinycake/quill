//! Screenshot demos: chat list.

use crate::ui::app::QuillApp;
use crate::ui::chat_list::{
    apply_ready_chat_list, apply_ready_chat_list_3, apply_ready_chat_list_menu,
    apply_ready_chat_preview, apply_ready_chat_rows,
};
use crate::ui::chat_row::ChatPreviewState;
use crate::ui::chatlist_demo::{
    apply_ready_archive_row, apply_ready_join_bar, apply_ready_multiline_rows,
    apply_ready_search_previews,
};
use crate::ui::conversation::apply_ready_typing;
use crate::ui::notification_settings::apply_ready_notification_sound;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::shared_media::apply_ready_shared_media;
use crate::ui::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::SearchStatus;
use std::sync::atomic::Ordering;

const NOTE_READY_NOTIFICATION_SOUND: &str = "screenshot demo — notification sounds + settings (injected saved sounds + chat/scope settings)";

register_demos![
    // Same, with `archiveCollapsed`: the slim bar.
    DemoSpec::chats(
        "ready-archive-bar",
        "screenshot demo — chat list: archive row · story rings · pinned drag"
    )
    .setup(|app, window, cx| app.demo_archive_row(ArchiveRowDemo::ArchiveBar, window, cx)),
    // Same, with the archive row's context menu open.
    DemoSpec::chats(
        "ready-archive-menu",
        "screenshot demo — chat list: archive row · story rings · pinned drag"
    )
    .setup(|app, window, cx| app.demo_archive_row(ArchiveRowDemo::ArchiveMenu, window, cx)),
    // Archived-chats row on top of the chat list (names + muted unread
    // badge) with story rings on avatars and three pinned chats.
    DemoSpec::chats(
        "ready-archive-row",
        "screenshot demo — chat list: archive row · story rings · pinned drag"
    )
    .setup(|app, window, cx| app.demo_archive_row(ArchiveRowDemo::ArchiveRow, window, cx)),
    // Chat header badges and the bars that replace the composer
    // (`QUILL_DEMO_HEADER=<variant>`; see `chat_header_demo`).
    DemoSpec::chats(
        "ready-chat-header",
        "screenshot demo — join bar · search and chat-row previews"
    )
    .setup(QuillApp::demo_ready_chat_header),
    // Slice CL1: chat list with a pinned chat, archived section,
    // marked-as-unread row, and the row context menu open (injected,
    // no live Telegram).
    DemoSpec::chats(
        "ready-chat-list",
        "screenshot demo — chat list: pinned, archived, marked unread, row menu"
    )
    .setup(QuillApp::demo_ready_chat_list_menu),
    // Slice CL2: chat list with folder tabs, All/Unread/Archived
    // category chips, pinned + unread chats, an expanded archive
    // section, and the Saved Messages entry (injected, no live
    // Telegram).
    DemoSpec::chats(
        "ready-chat-list-2",
        "screenshot demo — chat list: folders, category filters, pinned drag, archive"
    )
    .setup(QuillApp::demo_ready_chat_list),
    // Slice CL3: chat list with the @ mention badge, the ♥ reaction
    // badge, multi-select mode (two chats checked + the select bar),
    // and the row menu open showing Report / Block user (injected,
    // no live Telegram).
    DemoSpec::chats(
        "ready-chat-list-3",
        "screenshot demo — chat list: mentions · reactions · multi-select"
    )
    .setup(QuillApp::demo_ready_chat_list3),
    // Slice CL2: the archive auto-settings dialog over the
    // `ReadyChatList` fixture (injected settings, no live Telegram).
    DemoSpec::chats(
        "ready-chat-list-archive",
        "screenshot demo — chat list: archive settings dialog"
    )
    .setup(QuillApp::demo_ready_chat_list_archive),
    // Slice CL2: sidebar search with an empty result (injected, no
    // live Telegram).
    DemoSpec::chats(
        "ready-chat-list-search",
        "screenshot demo — chat list: search empty state"
    )
    .setup(QuillApp::demo_ready_chat_list_search),
    // Slice CL: the floating peek preview open on "Demo chat B" with
    // recent messages (injected, no live Telegram).
    DemoSpec::chats(
        "ready-chat-preview",
        "screenshot demo — chat list peek preview (injected updates, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_chat_preview),
    // Chat-row polish: Draft prefix, sending / failed marks, verified /
    // Premium / SCAM / FAKE title badges, online dot (injected, no live
    // Telegram).
    DemoSpec::chats(
        "ready-chat-rows",
        "screenshot demo — chat rows: drafts · send state · title badges"
    )
    .setup(QuillApp::demo_ready_chat_rows),
    // Non-member public channel opened from search: the bottom bar must
    // resolve to "Join channel" (injected, no live Telegram).
    DemoSpec::chats(
        "ready-join-bar",
        "screenshot demo — join bar · search and chat-row previews"
    )
    .setup(QuillApp::demo_ready_join_bar),
    // Chat-list rows whose last message has hard newlines / a leading
    // custom emoji (injected, no live Telegram).
    DemoSpec::chats(
        "ready-multiline-rows",
        "screenshot demo — join bar · search and chat-row previews"
    )
    .setup(QuillApp::demo_ready_multiline_rows),
    // Notification settings demo (injected, no live Telegram): the open
    // chat has a custom notification sound (`getSavedNotificationSounds`
    // fixture) and the per-chat notifications panel is open with the sound
    // picker expanded (parity slice: notification sounds).
    DemoSpec::chats("ready-notification-sound", NOTE_READY_NOTIFICATION_SOUND)
        .setup(QuillApp::demo_ready_notification_sound),
    // Same, mid pinned-drag: the dragged row follows the pointer while
    // the displaced one slides home.
    DemoSpec::chats(
        "ready-pin-drag",
        "screenshot demo — chat list: archive row · story rings · pinned drag"
    )
    .setup(|app, window, cx| app.demo_archive_row(ArchiveRowDemo::PinDrag, window, cx)),
    // Chat-list search with a custom-emoji, multi-line public-chat preview
    // and a multi-line chat preview (injected, no live Telegram).
    DemoSpec::chats(
        "ready-search-previews",
        "screenshot demo — join bar · search and chat-row previews"
    )
    .setup(QuillApp::demo_ready_search_previews),
    // Slice media-shared-gallery: per-chat shared-media gallery open on
    // chat 11 — the Media tab shows its empty state, the Files tab two
    // injected documents (injected `foundChatMessages` through the real
    // reducer, no live Telegram).
    DemoSpec::chats(
        "ready-shared-media",
        "screenshot demo — shared media gallery: per-tab empty states"
    )
    .setup(QuillApp::demo_ready_shared_media),
    // The list scrolled past the strip: collapsed to the small stack.
    DemoSpec::chats(
        "ready-stories-collapsed",
        "screenshot demo — chat list: swipe actions · stories strip"
    )
    .setup(|app, window, cx| app.demo_swipe_stories(
        SwipeStoriesDemo::StoriesCollapsed,
        window,
        cx
    )),
    // The list scrolled half the strip's height: the compact stack is
    // fading in beside the search field.
    DemoSpec::chats(
        "ready-stories-collapsing",
        "screenshot demo — chat list: swipe actions · stories strip"
    )
    .setup(|app, window, cx| app.demo_swipe_stories(
        SwipeStoriesDemo::StoriesCollapsing,
        window,
        cx
    )),
    // Stories strip expanded at the top of a long chat list, swipe action
    // Mute configured, nothing held (scripted gestures start here).
    DemoSpec::chats(
        "ready-stories-expanded",
        "screenshot demo — chat list: swipe actions · stories strip"
    )
    .setup(|app, window, cx| app.demo_swipe_stories(
        SwipeStoriesDemo::StoriesExpanded,
        window,
        cx
    )),
    // Chat-row swipe: "Mira Cohen" held mid-swipe (ratio 0.6) with the
    // Mute action revealed; the list is long enough to scroll.
    DemoSpec::chats(
        "ready-swipe-mute",
        "screenshot demo — chat list: swipe actions · stories strip"
    )
    .setup(|app, window, cx| app.demo_swipe_stories(SwipeStoriesDemo::SwipeMute, window, cx)),
    // "Noam Katz" with action Delete, held past the threshold (ratio 1.25) with the
    // reach circle fully grown.
    DemoSpec::chats(
        "ready-swipe-reached",
        "screenshot demo — chat list: swipe actions · stories strip"
    )
    .setup(|app, window, cx| app.demo_swipe_stories(
        SwipeStoriesDemo::SwipeReached,
        window,
        cx
    )),
    // Batch 8: chat top bars; `QUILL_DEMO_BAR` picks the variant.
    DemoSpec::chats(
        "ready-top-bars",
        "screenshot demo — join bar · search and chat-row previews"
    )
    .setup(QuillApp::demo_ready_top_bars),
    // Peer `chatActionTyping` in the open-chat header and sidebar row.
    DemoSpec::chats(
        "ready-typing",
        "screenshot demo — peer typing (injected updateChatAction)"
    )
    .setup(QuillApp::demo_ready_typing),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArchiveRowDemo {
    ArchiveRow,
    ArchiveBar,
    ArchiveMenu,
    PinDrag,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SwipeStoriesDemo {
    SwipeMute,
    SwipeReached,
    StoriesExpanded,
    StoriesCollapsing,
    StoriesCollapsed,
}

impl QuillApp {
    fn demo_ready_chat_header(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let variant = std::env::var("QUILL_DEMO_HEADER").unwrap_or_default();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::chat_header_demo::apply_ready_chat_header(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
                &variant,
            );
        }
        self.connection.status_note = "screenshot demo — chat header".into();
    }

    fn demo_ready_chat_list_menu(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice CL1: pinned + archived + marked-as-unread rows, with the
        // row context menu open over the pinned chat.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_list_menu(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.chat_list.menu = Some(ChatMenuState {
            chat_id: ChatId(11),
            position: Point::new(px(120.), px(490.)),
        });
        self.connection.status_note =
            "screenshot demo — pin · archive · marked unread · row menu".into();
    }

    fn demo_ready_chat_list(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice CL2: folder tabs + category chips + pinned chat +
        // expanded archive section; Main stays selected.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_list(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note =
            "screenshot demo — folders · categories · pinned · archive".into();
    }

    fn demo_ready_chat_list3(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice CL3: select mode (chats 11 + 12 checked, select bar),
        // the @ mention badge on chat 11, the ♥ reaction badge on chat
        // 12, and the row menu open on chat 11 showing Report / Block
        // user / Select. The menu sits below the rows so both badges
        // and the select bar stay visible (capture at
        // QUILL_DEMO_WINDOW_SIZE=1200x1250 on a 1400x1400 display).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_list_3(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.chat_list.selected.insert(11);
        self.chat_list.selected.insert(12);
        self.chat_list.menu = Some(ChatMenuState {
            chat_id: ChatId(11),
            position: Point::new(px(120.), px(770.)),
        });
        self.connection.status_note =
            "screenshot demo — mentions · reactions · multi-select · report · block".into();
    }

    fn demo_ready_chat_list_archive(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice CL2: same chat-list fixture with the archive
        // auto-settings dialog open.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_list(session, &self.demo_ui.sink, &self.demo_ui.seq);
            session.archive_settings_open = true;
        }
        self.connection.status_note = "screenshot demo — archive settings dialog".into();
    }

    fn demo_ready_chat_list_search(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice CL2: sidebar search showing the empty-result state.
        if let Some(session) = self.demo_session.as_mut() {
            session.open_search();
            session.search.begin_query("xyzzy-no-such-chat");
            session.search.status = SearchStatus::Empty;
        }
        self.connection.status_note = "screenshot demo — search empty state".into();
    }

    fn demo_ready_chat_preview(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice CL: peek preview open on chat 12 ("Demo chat B") with a
        // few injected messages; chat 11 stays the open chat so the
        // panel floats over the chat list. The anchor sits just right of
        // the second chat row.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_preview(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.chat_list.preview = Some(ChatPreviewState {
            chat_id: ChatId(12),
            anchor: Point::new(px(170.), px(470.)),
        });
        self.connection.status_note = "screenshot demo — chat peek preview".into();
    }

    fn demo_ready_chat_rows(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_rows(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — chat rows".into();
    }

    fn demo_ready_join_bar(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_join_bar(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — non-member channel".into();
    }

    fn demo_ready_multiline_rows(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_multiline_rows(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — multi-line row previews".into();
    }

    fn demo_ready_notification_sound(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_notification_sound(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.notify.mute_menu_open = true;
        self.notify.notif_sound_picker_open = true;
        self.connection.status_note =
            "screenshot demo — notification sounds · per-chat panel · scope defaults".into();
    }

    fn demo_ready_search_previews(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_search_previews(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — search previews".into();
    }

    fn demo_ready_shared_media(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice media-shared-gallery: the gallery open on chat 11 — the
        // Media tab shows its empty state, the Files tab two injected
        // documents (through the real `foundChatMessages` reducer).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_shared_media(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — shared media gallery empty state".into();
    }

    fn demo_ready_top_bars(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let variant = std::env::var("QUILL_DEMO_BAR").unwrap_or_default();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::chat_bars::apply_ready_top_bars(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
                &variant,
            );
        }
        match variant.as_str() {
            "requests-box" => {
                self.open_join_requests_dialog(
                    quill::ids::ChatId(crate::ui::chat_bars::DEMO_GROUP),
                    window,
                    cx,
                );
            }
            "block-box" => {
                self.dialogs.block_bar_dialog = Some(crate::ui::chat_bars::BlockBarDialog {
                    chat_id: quill::ids::ChatId(crate::ui::chat_bars::DEMO_STRANGER),
                    user_id: crate::ui::chat_bars::DEMO_STRANGER,
                    report: true,
                    delete_chat: true,
                });
            }
            _ => {}
        }
        self.connection.status_note = "screenshot demo — chat top bars".into();
    }

    fn demo_ready_typing(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_typing(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — typing…".into();
    }

    fn demo_archive_row(
        &mut self,
        demo: ArchiveRowDemo,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Archive row / bar / menu and the pinned drag, over the same
        // fixture (archived chats, story rings, three pinned chats).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_archive_row(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        if matches!(demo, ArchiveRowDemo::ArchiveBar) {
            self.appearance.archive_collapsed = true;
        }
        if matches!(demo, ArchiveRowDemo::ArchiveMenu) {
            self.chat_list.archive_menu = Some(Point::new(px(120.), px(150.)));
        }
        if matches!(demo, ArchiveRowDemo::PinDrag) {
            // Pinned 11 / 12 / 13: drag 12 past 13. 13 has just
            // started sliding back up into the slot above.
            let heights = [11, 12, 13].into_iter().map(|id| (id, 64.0)).collect();
            if let Some(mut drag) =
                quill::pin_reorder::PinReorder::begin(vec![11, 12, 13], heights, 12, 300.)
            {
                drag.drag_to(352., std::time::Instant::now());
                self.chat_list.pin_reorder = Some(drag);
            }
        }
        self.connection.status_note = "screenshot demo — archive row · story rings".into();
    }

    fn demo_swipe_stories(
        &mut self,
        demo: SwipeStoriesDemo,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Chat-row swipe and stories-strip collapse over one long list:
        // the archive-row fixture (story rings, pins, archive) plus the
        // chat-row fixture's extra chats, so the list scrolls.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_archive_row(session, &self.demo_ui.sink, &self.demo_ui.seq);
            apply_ready_chat_rows(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        match demo {
            SwipeStoriesDemo::SwipeMute => {
                self.appearance.swipe_action = quill::chat_swipe::SwipeAction::Mute;
                self.demo_hold_swipe(21, 0.6);
            }
            SwipeStoriesDemo::SwipeReached => {
                self.appearance.swipe_action = quill::chat_swipe::SwipeAction::Delete;
                if let Some(chat) = self
                    .demo_session
                    .as_mut()
                    .and_then(|session| session.chats.get_mut(&22))
                {
                    chat.can_be_deleted_only_for_self = true;
                }
                self.demo_hold_swipe(22, 1.25);
            }
            SwipeStoriesDemo::StoriesExpanded => {
                // Resting list, Mute configured: the base for scripted
                // gestures (`QUILL_DEMO_CLICK=w:x,y,dx,dy,s|m|e`).
                self.appearance.swipe_action = quill::chat_swipe::SwipeAction::Mute;
            }
            SwipeStoriesDemo::StoriesCollapsing => {
                self.chat_list.scroll.set_offset(point(px(0.), px(-38.)));
            }
            SwipeStoriesDemo::StoriesCollapsed => {
                self.chat_list.scroll.set_offset(point(px(0.), px(-96.)));
            }
        }
        self.connection.status_note = "screenshot demo — swipe actions · stories strip".into();
    }
}
