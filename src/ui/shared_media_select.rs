//! The shared-media gallery's row menu and multi-select (tdesktop
//! `Info::Media::ListWidget`): a right click offers Go to message, Cancel
//! download or Show in folder, Forward, Delete and Select; ticked rows get a
//! header with "N selected", Forward, Delete and Clear. Forward and Delete
//! reuse the chat's forward picker and delete box, and only what TDLib allows
//! for every ticked message (`messageProperties`) is offered.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::status_note::StatusNote;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::ForwardDraft;
use quill::ids::{FileId, MessageId};
use quill::lang::{keys, tr};
use quill::state::{
    MediaMenuFacts, MediaMenuItem, SelectedMedia, SharedMediaAction, SharedMediaItem,
    SharedMediaState, SharedMediaTab, media_menu_items,
};
use quill::telegram::envelope::MessageActions;

impl QuillApp {
    fn shared_media_gallery_mut(&mut self) -> Option<&mut SharedMediaState> {
        if self.live.is_some() {
            self.live
                .as_mut()
                .map(|live| &mut live.driver.session.media.shared_media)
        } else {
            self.demo_session
                .as_mut()
                .map(|session| &mut session.media.shared_media)
        }
    }

    /// What TDLib last said `message_id` may do, when it was asked for this
    /// chat's message.
    fn shared_media_actions(&self, message_id: MessageId) -> Option<MessageActions> {
        let session = self.session()?;
        let chat = session.media.shared_media.chat_id?;
        session
            .messages
            .message_menu_actions
            .filter(|(c, m, _)| *c == chat && *m == message_id)
            .map(|(_, _, actions)| actions)
    }

    /// Ask what a hovered row allows, so its menu and a tick know before
    /// they are used. The answer lands in the session a moment later.
    fn shared_media_hover(&mut self, message_id: MessageId, hovered: bool, cx: &mut Context<Self>) {
        // The row just left was answered by now: its tick learns the answer.
        let answer = self
            .session()
            .and_then(|session| session.messages.message_menu_actions);
        if let Some(state) = self.shared_media_gallery_mut()
            && state.refresh_from_answer(answer)
        {
            cx.notify();
        }
        if !hovered || self.shared_media_actions(message_id).is_some() {
            return;
        }
        let Some(chat) = self
            .session()
            .and_then(|session| session.media.shared_media.chat_id)
        else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.refresh_message_menu_actions(chat, message_id);
        }
    }

    pub(super) fn shared_media_hover_listener(
        &self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        let owner = cx.entity().downgrade();
        move |hovered, _, cx| {
            let _ = owner.update(cx, |this, cx| {
                this.shared_media_hover(message_id, *hovered, cx);
            });
        }
    }

    pub(super) fn shared_media_selecting(&self) -> bool {
        self.session()
            .is_some_and(|session| !session.media.shared_media.selection.is_empty())
    }

    pub(super) fn shared_media_is_selected(&self, message_id: MessageId) -> bool {
        self.session()
            .is_some_and(|session| session.media.shared_media.is_selected(message_id))
    }

    /// A click on a row or tile while rows are ticked ticks or unticks it.
    pub(super) fn toggle_shared_media_selection(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let entry =
            SelectedMedia::from_actions(message_id, self.shared_media_actions(message_id).as_ref());
        if let Some(state) = self.shared_media_gallery_mut() {
            state.toggle_selected(entry);
        }
        cx.notify();
    }

    /// The round tick shown on rows and tiles while rows are ticked.
    pub(super) fn shared_media_check(&self, selected: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        div()
            .absolute()
            .top_1()
            .right_1()
            .size_5()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(if selected {
                theme.primary
            } else {
                theme.border
            })
            .bg(if selected {
                theme.primary
            } else {
                theme.background.opacity(0.6)
            })
            .when(selected, |this| {
                this.child(
                    Icon::new(IconName::Check)
                        .size_3()
                        .text_color(theme.primary_foreground),
                )
            })
            .into_any_element()
    }

    fn clear_shared_media_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.shared_media_gallery_mut() {
            state.clear_selection();
        }
        cx.notify();
    }

    fn shared_media_menu_facts(&self, item: &SharedMediaItem) -> MediaMenuFacts {
        let Some(session) = self.session() else {
            return MediaMenuFacts::default();
        };
        let chat = session.media.shared_media.chat_id;
        let actions = self.shared_media_actions(item.message_id);
        let downloading = item.file.0 != 0
            && (session
                .media
                .downloads
                .is_downloading(item.file.0, &session.media.files)
                || session.requests.has_download(item.file));
        let on_disk = session
            .media
            .files
            .get(&item.file.0)
            .is_some_and(|file| file.usable_path().is_some());
        let protected = chat.is_some_and(|chat| session.chat_has_protected_content(chat));
        let gallery = &session.media.shared_media;
        let can_forward = actions.is_some_and(|a| a.can_be_forwarded);
        let can_delete = actions
            .is_some_and(|a| a.can_be_deleted_only_for_self || a.can_be_deleted_for_all_users);
        // Show in folder is for documents, not for photos and videos in the cache.
        let document_tab = !matches!(
            gallery.active_tab,
            SharedMediaTab::Media
                | SharedMediaTab::Photos
                | SharedMediaTab::Videos
                | SharedMediaTab::Gifs
        );
        MediaMenuFacts {
            is_message: item.action == SharedMediaAction::Message,
            downloading,
            local: document_tab && on_disk && !protected && actions.is_none_or(|a| a.can_be_saved),
            can_forward,
            can_delete,
            selected: session.media.shared_media.is_selected(item.message_id),
            // tdesktop `hasSelectRestriction`: nothing to forward or delete.
            can_select: actions.is_none_or(|_| can_forward || can_delete),
            group_can_forward: gallery.can_forward_selection(),
            group_can_delete: gallery.can_delete_selection(),
        }
    }

    /// The right-click menu of a message row or tile.
    pub(super) fn shared_media_context_menu(
        &self,
        item: &SharedMediaItem,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let items = media_menu_items(&self.shared_media_menu_facts(item));
        let (message_id, file) = (item.message_id, item.file);
        let owner = cx.entity().downgrade();
        move |mut menu, _, _| {
            for kind in items.iter().copied() {
                let owner = owner.clone();
                menu = menu.item(PopupMenuItem::new(menu_label(kind)).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.shared_media_menu_action(kind, message_id, file, window, cx);
                        });
                    },
                ));
            }
            menu
        }
    }

    fn shared_media_menu_action(
        &mut self,
        kind: MediaMenuItem,
        message_id: MessageId,
        file: FileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self
            .session()
            .and_then(|session| session.media.shared_media.chat_id)
        else {
            return;
        };
        match kind {
            MediaMenuItem::GoToMessage => self.jump_to_shared_media_item_ui(message_id, cx),
            MediaMenuItem::CancelDownload => self.cancel_media_download(file, cx),
            MediaMenuItem::ShowInFolder => self.reveal_downloaded_file(file, cx),
            MediaMenuItem::Forward => {
                self.begin_forward_one(chat_id, message_id, false, window, cx)
            }
            MediaMenuItem::Delete => {
                let entry = SelectedMedia::from_actions(
                    message_id,
                    self.shared_media_actions(message_id).as_ref(),
                );
                if entry.can_delete {
                    self.confirm_delete_ids(
                        chat_id,
                        vec![message_id],
                        entry.can_revoke,
                        window,
                        cx,
                    );
                }
            }
            MediaMenuItem::Select => self.toggle_shared_media_selection(message_id, cx),
            MediaMenuItem::ForwardSelected => self.forward_shared_media_selection(window, cx),
            MediaMenuItem::DeleteSelected => self.delete_shared_media_selection(window, cx),
            MediaMenuItem::ClearSelection => self.clear_shared_media_selection(cx),
        }
    }

    /// The header over a ticked gallery: "N selected" with Forward, Delete
    /// and Clear. Forward and Delete show only when every ticked message
    /// allows it.
    pub(super) fn shared_media_selection_bar(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = &self.session()?.media.shared_media;
        let count = state.selection.len();
        if count == 0 {
            return None;
        }
        let (can_forward, can_delete) =
            (state.can_forward_selection(), state.can_delete_selection());
        Some(
            div()
                .id("shared-media-selection-bar")
                .debug_selector(|| "shared-media-selection-bar".to_owned())
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_semibold()
                        .truncate()
                        .child(tab.selection_title(count)),
                )
                .child(
                    Button::new("shared-media-forward")
                        .label(tr(keys::CONTEXT_FORWARD_MSG))
                        .primary()
                        .small()
                        .disabled(!can_forward)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.forward_shared_media_selection(window, cx);
                        })),
                )
                .child(
                    Button::new("shared-media-delete")
                        .label(tr(keys::BOX_DELETE))
                        .small()
                        .disabled(!can_delete)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.delete_shared_media_selection(window, cx);
                        })),
                )
                .child(
                    div()
                        .debug_selector(|| "shared-media-clear".to_owned())
                        .child(
                            Button::new("shared-media-clear")
                                .label(tr(keys::Q_OPS_CLEAR))
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clear_shared_media_selection(cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    fn forward_shared_media_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.session().map(|s| &s.media.shared_media) else {
            return;
        };
        let (Some(chat_id), true) = (state.chat_id, state.can_forward_selection()) else {
            return;
        };
        // The chat's own selection stays: its draft is not replaced.
        if self.share.pending_forward.is_some() && !self.share.forward_picker_open {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_OPS_FINISH_THE_CHAT_SELECTION_FIRST,
            )));
            cx.notify();
            return;
        }
        let draft = ForwardDraft {
            from_chat_id: chat_id,
            message_ids: state.selected_ids(),
            send_copy: false,
            remove_caption: false,
        };
        self.share.pending_forward = Some(draft);
        self.clear_shared_media_selection(cx);
        self.open_forward_picker(window, cx);
    }

    fn delete_shared_media_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.session().map(|s| &s.media.shared_media) else {
            return;
        };
        let (Some(chat_id), true) = (state.chat_id, state.can_delete_selection()) else {
            return;
        };
        let (ids, revoke) = (state.selected_ids(), state.can_revoke_selection());
        self.confirm_delete_ids(chat_id, ids, revoke, window, cx);
    }
}

fn menu_label(kind: MediaMenuItem) -> String {
    match kind {
        MediaMenuItem::GoToMessage => tr(keys::Q_OPS_GO_TO_MESSAGE),
        MediaMenuItem::CancelDownload => tr(keys::Q_OPS_CANCEL_DOWNLOAD),
        MediaMenuItem::ShowInFolder if cfg!(target_os = "macos") => tr(keys::Q_AV_SHOW_IN_FINDER),
        MediaMenuItem::ShowInFolder => tr(keys::Q_AV_SHOW_IN_FOLDER),
        MediaMenuItem::Forward => tr(keys::CONTEXT_FORWARD_MSG),
        MediaMenuItem::Delete => tr(keys::BOX_DELETE),
        MediaMenuItem::Select => tr(keys::CONTEXT_SELECT_MSG),
        MediaMenuItem::ForwardSelected => tr(keys::CONTEXT_FORWARD_SELECTED),
        MediaMenuItem::DeleteSelected => tr(keys::CONTEXT_DELETE_SELECTED),
        MediaMenuItem::ClearSelection => tr(keys::CONTEXT_CLEAR_SELECTION),
    }
    .to_string()
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "shared_media_select_tests.rs"]
mod tests;
