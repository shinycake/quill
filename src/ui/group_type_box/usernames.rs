//! The Type box's "Link order" list (tdesktop `UsernamesList`): every
//! username with its state; the editable one focuses the link field, the
//! others show or hide after a confirmation, active ones drag to reorder,
//! and each copies its link from the context menu.
#![deny(clippy::indexing_slicing)]

use super::sections::{about, heading};
use super::{GroupTypeBox, TypeBoxFacts};
use crate::ui::app::QuillApp;
use crate::ui::pressable::action_row;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::group_type::{ACTIVE_USERNAMES_MAX, display_order, reorder_active, username_row_status};
use quill::ids::ChatId;
use quill::lang::{keys, tr, trn};
use quill::state::UsernameToggleError;

/// A username being dragged to a new place.
#[derive(Clone)]
struct UsernameDrag {
    name: String,
}

impl Render for UsernameDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(cx.theme().secondary)
            .text_sm()
            .child(format!("@{}", self.name))
    }
}

impl QuillApp {
    pub(super) fn group_type_usernames(
        &self,
        type_box: &GroupTypeBox,
        facts: &TypeBoxFacts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let lists = self.session()?.chat_usernames(facts.chat_id)?;
        if !lists.is_manageable() {
            return None;
        }
        let active = display_order(&type_box.draft.order, &lists.active);
        let rows: Vec<(String, bool)> = active
            .iter()
            .map(|name| (name.clone(), true))
            .chain(lists.disabled.iter().map(|name| (name.clone(), false)))
            .collect();
        let chat_id = facts.chat_id;
        let mut list = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(heading(tr(keys::CHANNEL_USERNAMES_SUBTITLE), cx));
        for (name, is_active) in rows {
            let editable = name == facts.editable;
            list = list.child(self.username_row(chat_id, name, editable, is_active, cx));
        }
        if let Some(error) = self.username_toggle_error(chat_id) {
            list = list.child(div().text_xs().text_color(cx.theme().danger).child(error));
        }
        Some(
            list.child(about(tr(keys::CHANNEL_USERNAMES_DESCRIPTION), cx))
                .into_any_element(),
        )
    }

    fn username_row(
        &self,
        chat_id: ChatId,
        name: String,
        editable: bool,
        is_active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (fg, muted) = (theme.foreground, theme.muted_foreground);
        let id = SharedString::from(format!("group-type-username-{name}"));
        let owner = cx.entity().downgrade();
        let copy_name = name.clone();
        let click_name = name.clone();
        let drop_name = name.clone();
        let drag = UsernameDrag { name: name.clone() };
        let status = tr(username_row_status(editable, is_active));
        action_row(id, None, format!("@{name}"), false, cx)
            .text_color(if is_active { fg } else { muted })
            .child(div().flex_1())
            .child(div().text_xs().text_color(muted).child(status))
            .when(is_active, |row| {
                row.cursor_move()
                    .on_drag(drag, |drag: &UsernameDrag, _, _, cx| {
                        cx.new(|_| drag.clone())
                    })
                    .on_drop(cx.listener(move |this, drag: &UsernameDrag, _, cx| {
                        this.drop_group_type_username(&drag.name, &drop_name, cx);
                    }))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                if editable {
                    if let Some(type_box) = this.group_type_box() {
                        type_box
                            .link_input
                            .update(cx, |input, cx| input.focus(window, cx));
                    }
                } else {
                    this.confirm_group_type_toggle(
                        chat_id,
                        click_name.clone(),
                        !is_active,
                        window,
                        cx,
                    );
                }
            }))
            .context_menu(move |menu, _, _| {
                let owner = owner.clone();
                let name = copy_name.clone();
                menu.item(
                    PopupMenuItem::new(tr(keys::GROUP_INVITE_CONTEXT_COPY)).on_click(
                        move |_, _, cx| {
                            let _ = owner
                                .update(cx, |this, cx| this.copy_group_type_username(&name, cx));
                        },
                    ),
                )
            })
            .into_any_element()
    }

    fn drop_group_type_username(&mut self, dragged: &str, onto: &str, cx: &mut Context<Self>) {
        let active = self
            .group_edit_box()
            .and_then(|edit| self.session()?.chat_usernames(edit.chat_id))
            .map(|lists| lists.active.clone())
            .unwrap_or_default();
        let Some(type_box) = self.group_edit_box_mut().and_then(|e| e.type_box.as_mut()) else {
            return;
        };
        let shown = display_order(&type_box.draft.order, &active);
        if let Some(order) = reorder_active(&shown, dragged, onto) {
            type_box.draft.order = order;
            cx.notify();
        }
    }

    /// tdesktop's confirmation before a link is shown or hidden; the
    /// change is sent at once, as tdesktop does.
    fn confirm_group_type_toggle(
        &mut self,
        chat_id: ChatId,
        name: String,
        show: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            let name = name.clone();
            alert
                .description(tr(if show {
                    keys::CHANNEL_USERNAMES_ACTIVATE_DESCRIPTION
                } else {
                    keys::CHANNEL_USERNAMES_DEACTIVATE_DESCRIPTION
                }))
                .ok_text(tr(if show {
                    keys::USERNAMES_ACTIVATE_CONFIRM
                } else {
                    keys::USERNAMES_DEACTIVATE_CONFIRM
                }))
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| {
                        this.toggle_group_type_username(chat_id, &name, show, cx);
                    });
                    true
                })
        });
    }

    fn toggle_group_type_username(
        &mut self,
        chat_id: ChatId,
        name: &str,
        show: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
            cx.notify();
            return;
        };
        live.driver.session.groups.group_type.toggle_error = None;
        if live
            .driver
            .toggle_group_username(chat_id, name, show)
            .is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_SEND_THE_CHANGE_TRY,
            )));
        }
        cx.notify();
    }

    fn username_toggle_error(&self, chat_id: ChatId) -> Option<String> {
        let (chat, error) = self.session()?.groups.group_type.toggle_error?;
        if chat != chat_id {
            return None;
        }
        Some(match error {
            UsernameToggleError::TooMany => {
                trn(keys::USERNAMES_ACTIVATE_ERROR, ACTIVE_USERNAMES_MAX).replace("**", "")
            }
            UsernameToggleError::Flood => tr(keys::FLOOD_ERROR).to_string(),
            UsernameToggleError::Other => {
                tr(keys::Q_PEOPLE_COULDNT_SEND_THE_CHANGE_TRY).to_string()
            }
        })
    }
}
