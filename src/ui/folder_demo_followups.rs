//! Screenshot fixtures of the folder follow-ups: tag chips and colours,
//! the folder tab menu, the shared folder's new chats, limit box and the
//! remove-folder box (injected through the reducer, no live Telegram).

use super::ScreenshotDemo as D;
use super::app::QuillApp;
use super::dialogs::{FolderDeleteConfirm, FolderEditorDialog};
use super::folder_demo::apply_ready_folders_share;
use super::folder_extras::{FolderNewChatsDialog, FolderTabMenu};
use gpui_kit::{Context, Window, point, px};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::folder_icons::{FolderTabsMode, FolderTabsView};
use quill::folder_limits::FolderLimitKind;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChatFolderSpec;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The shared-folder fixture with tag colours, optionally a Premium
/// account, and chats in several folders so rows show their tag chips.
fn apply_ready_folder_tags(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    premium: bool,
) {
    apply_ready_folders_share(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for (chat_id, folder_id, order) in [(11, 3, "50"), (12, 4, "45"), (13, 1, "58"), (14, 1, "57")]
    {
        let json = format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListFolder","chat_folder_id":{folder_id}}},"order":"{order}","is_pinned":false}}}}"#
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    // Work violet, News cyan, Family blue, Travel red.
    for (id, color) in [(1, 2), (2, 4), (3, 5), (4, 0)] {
        if let Some(folder) = session.chat_folders.iter_mut().find(|f| f.id == id) {
            folder.color_id = color;
        }
    }
    session.are_folder_tags_enabled = true;
    session.premium_option = Some(premium);
    // What `getChatFolderNewChats` and `getChatFolderChatsToLeave` answer.
    session.folder_new_chats.insert(1, vec![14, 16]);
    session.folder_chats_to_leave.insert(1, vec![13, 14, 16]);
}

impl QuillApp {
    /// Fixtures of the folder follow-ups captures.
    pub(super) fn demo_setup_folder_followups(
        &mut self,
        demo: Option<D>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(demo) = demo else { return };
        if !matches!(
            demo,
            D::ReadyFoldersTags
                | D::ReadyFoldersTagColor
                | D::ReadyFoldersMenu
                | D::ReadyFoldersNewChats
                | D::ReadyFoldersNewChatsJoin
                | D::ReadyFoldersLimit
                | D::ReadyFoldersDelete
        ) {
            return;
        }
        let premium = matches!(demo, D::ReadyFoldersTags | D::ReadyFoldersTagColor);
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_folder_tags(session, &self.demo_sink, &self.demo_seq, premium);
        }
        self.appearance.folder_tabs_view = FolderTabsView::Top;
        match demo {
            D::ReadyFoldersTags => {
                self.status_note = "screenshot demo — folder tags on chat rows".into();
            }
            D::ReadyFoldersTagColor => {
                let mut dialog = FolderEditorDialog::new(window, cx, Some(1));
                if let Some(spec) = self.session().and_then(|s| s.folder_specs.get(&1).cloned()) {
                    let spec = ChatFolderSpec {
                        color_id: 2,
                        ..spec
                    };
                    dialog.prefill_from_spec(&spec, window, cx);
                }
                self.folder_editor = Some(dialog);
            }
            D::ReadyFoldersMenu => {
                self.appearance.folder_tabs_mode = FolderTabsMode::TextAndIcons;
                self.folder_tab_menu = Some(FolderTabMenu {
                    folder_id: Some(1),
                    position: point(px(190.), px(96.)),
                });
                if let Some(session) = self.demo_session.as_mut()
                    && let Some(chat) = session.chats.get_mut(&13)
                {
                    chat.unread_count = 3;
                }
            }
            D::ReadyFoldersNewChats | D::ReadyFoldersNewChatsJoin => {
                self.folder_tab = Some(1);
                if matches!(demo, D::ReadyFoldersNewChatsJoin) {
                    self.folder_new_chats_dialog = Some(FolderNewChatsDialog {
                        folder_id: 1,
                        selected: [14, 16].into_iter().collect(),
                    });
                }
            }
            D::ReadyFoldersLimit => {
                self.folder_limit_box = Some(FolderLimitKind::Folders);
            }
            D::ReadyFoldersDelete => {
                self.folder_delete_confirm = Some(FolderDeleteConfirm {
                    folder_id: 1,
                    name: "Work".into(),
                    has_links: true,
                    shared: true,
                    keep: [14].into_iter().collect(),
                });
            }
            _ => {}
        }
        cx.notify();
    }
}
