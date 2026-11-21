//! "Set Status" and "Remove Status" on a custom emoji outside the status
//! selector: the emoji panel and an emoji pack's box (tdesktop
//! `menu/menu_emoji_status.cpp`). The item removes the status when the
//! emoji is the status now. Either way a toast says what changed, and a
//! click on it puts the old status back (`ShowDoneToast`).
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::status_note::{NoteClick, StatusNote};
use quill::emoji_status_selector::{
    MenuStatusAction, StatusUndo, menu_status_action, status_undo, undo_change,
};
use quill::lang::tr_with;

impl QuillApp {
    /// The status item a custom emoji's menu shows: none without Premium.
    pub(in crate::ui) fn menu_status_action(&self, emoji: Option<i64>) -> Option<MenuStatusAction> {
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        menu_status_action(premium, self.current_status_id(), emoji?)
    }

    /// The menu item chosen: change the status and toast the change with
    /// its Undo.
    pub(in crate::ui) fn apply_menu_status_action(
        &mut self,
        action: MenuStatusAction,
        cx: &mut Context<Self>,
    ) {
        let undo = self.status_undo_now();
        let (emoji, done) = match action {
            MenuStatusAction::Set(id) => (Some(id), keys::EMOJI_STATUS_SET_DONE),
            MenuStatusAction::Remove => (None, keys::EMOJI_STATUS_REMOVE_DONE),
        };
        if !self.change_emoji_status_for(emoji, 0, cx) {
            return;
        }
        let link = undo.map_or("", |_| tr(keys::EMOJI_STATUS_UNDO));
        let text = tr_with(done, &[("link", link)]);
        let note = StatusNote::info(text.trim_end());
        self.set_note(match undo {
            Some(undo) => note.with_click(NoteClick::UndoEmojiStatus(undo)),
            None => note,
        });
        cx.notify();
    }

    /// What Undo would put back now: the account's status and its end.
    fn status_undo_now(&self) -> Option<StatusUndo> {
        let user = self
            .session()
            .and_then(|s| s.my_user_id.and_then(|id| s.user(id)));
        match user {
            Some(user) => status_undo(user.emoji_status_id, user.emoji_status_until),
            None => Some(StatusUndo::Remove),
        }
    }

    /// The toast's Undo: put back the status the change replaced.
    pub(in crate::ui) fn undo_emoji_status(&mut self, undo: StatusUndo, cx: &mut Context<Self>) {
        let now = i64::try_from(quill::state::unix_ms_now() / 1000).unwrap_or(i64::MAX);
        let (emoji, secs) = undo_change(undo, now);
        self.change_emoji_status_for(emoji, secs, cx);
    }
}

/// `menu` with the status item for `action`, when there is one.
pub(in crate::ui) fn with_status_item(
    menu: gpui_kit::component::menu::PopupMenu,
    action: Option<MenuStatusAction>,
    owner: WeakEntity<QuillApp>,
) -> gpui_kit::component::menu::PopupMenu {
    let Some(action) = action else {
        return menu;
    };
    let label = match action {
        MenuStatusAction::Set(_) => tr(keys::EMOJI_STATUS_FOR_SUBMIT),
        MenuStatusAction::Remove => tr(keys::EMOJI_STATUS_REMOVE),
    };
    menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
        let _ = owner.update(cx, |this, cx| this.apply_menu_status_action(action, cx));
    }))
}
