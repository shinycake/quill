//! The "Add exception" and "Remove user" pickers over the member dialog
//! (tdesktop `AddSpecialBoxController`, `add_participants_box.cpp`): the
//! group's members with live search; a pick opens the restrict box or
//! removes the member, after the question `quill::add_special` asks.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::dialogs::AddSpecialDialog;
use super::super::pressable::action_row;
use super::super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::add_special::{SpecialConfirm, SpecialRole, SpecialStep, confirmed_step, pick_step};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::moderation::GroupFlavor;
use quill::state::{MemberListFilter, SupergroupMembersFetch};
use quill::telegram::envelope::{MessageSender, ParsedChatMember};
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    pub(in crate::ui) fn open_add_special(
        &mut self,
        chat_id: ChatId,
        role: SpecialRole,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.admin.add_special = Some(AddSpecialDialog::new(window, cx, chat_id, role));
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .fetch_supergroup_members(chat_id, MemberListFilter::Recent, "")
                .is_err()
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_LOAD_MEMBERS)));
        }
        cx.notify();
    }

    fn close_add_special(&mut self, cx: &mut Context<Self>) {
        self.admin.add_special = None;
        cx.notify();
    }

    pub(in crate::ui) fn search_add_special(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.admin.add_special.as_ref() else {
            return;
        };
        let chat_id = dialog.chat_id;
        let query = dialog.search.read(cx).value().trim().to_string();
        if !query.is_empty()
            && let Some(live) = self.live.as_mut()
            && live
                .driver
                .refresh_supergroup_members(chat_id, MemberListFilter::Search, &query)
                .is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULD_NOT_SEARCH_MEMBERS,
            )));
        }
        cx.notify();
    }

    fn add_special_filter(dialog: &AddSpecialDialog, cx: &App) -> (MemberListFilter, String) {
        let query = dialog.search.read(cx).value().trim().to_string();
        let filter = if query.is_empty() {
            MemberListFilter::Recent
        } else {
            MemberListFilter::Search
        };
        (filter, query)
    }

    fn add_special_load_more(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.admin.add_special.as_ref() else {
            return;
        };
        let (filter, query) = Self::add_special_filter(dialog, cx);
        let (chat_id, scroll) = (dialog.chat_id, dialog.scroll.clone());
        self.load_more_members(chat_id, filter, &query, &scroll);
    }

    fn add_special_pick(
        &mut self,
        member: &ParsedChatMember,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let MessageSender::User { user_id } = member.member_id else {
            return;
        };
        let Some(role) = self.admin.add_special.as_ref().map(|dialog| dialog.role) else {
            return;
        };
        let step = pick_step(role, member.status, member.can_be_edited);
        self.run_special_step(user_id, step, window, cx);
    }

    fn add_special_confirmed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.admin.add_special.as_mut() else {
            return;
        };
        let Some((user_id, question)) = dialog.confirm.take() else {
            return;
        };
        match confirmed_step(dialog.role, question) {
            Some(step) => self.run_special_step(user_id, step, window, cx),
            None => cx.notify(),
        }
    }

    fn run_special_step(
        &mut self,
        user_id: i64,
        step: SpecialStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.admin.add_special.as_mut() else {
            return;
        };
        let chat_id = dialog.chat_id;
        match step {
            SpecialStep::Ask(question) => dialog.confirm = Some((user_id, question)),
            SpecialStep::Restrict => self.open_restrict_dialog(chat_id, user_id, false, window, cx),
            SpecialStep::Kick => self.kick_from_add_special(chat_id, user_id, cx),
        }
        cx.notify();
    }

    /// tdesktop `kickUser`: banned with `KickedRestrictedRights`, forever.
    fn kick_from_add_special(&mut self, chat_id: ChatId, user_id: i64, cx: &mut Context<Self>) {
        let note = match self.live.as_mut() {
            Some(live) => match live.driver.ban_chat_member(chat_id, user_id, 0) {
                Ok(Some(_)) => {
                    self.admin.add_special = None;
                    StatusNote::silent(tr(keys::Q_PEOPLE_MEMBER_BANNED))
                }
                Ok(None) | Err(_) => {
                    StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_UPDATE_MEMBER_STATUS))
                }
            },
            None => StatusNote::info(tr(keys::Q_PEOPLE_MEMBER_ACTIONS_NEED_A_LIVE)),
        };
        self.connection.set_note(note);
        self.refresh_member_dialog(cx);
    }

    fn special_member_name(&self, user_id: i64) -> String {
        self.session()
            .and_then(|session| session.user(user_id))
            .map(|user| user.display_name())
            .unwrap_or_else(|| tr_with(keys::Q_PEOPLE_USER, &[("user_id", &user_id.to_string())]))
    }

    pub(super) fn build_add_special_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::AddSpecial, |this, _, cx| {
                this.close_add_special(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(state) = this.admin.add_special.as_ref() else {
                return dialog.on_close(on_close);
            };
            let title = match state.role {
                SpecialRole::Restricted => tr(keys::CHANNEL_ADD_EXCEPTION),
                SpecialRole::Kicked => tr(keys::CHANNEL_ADD_REMOVED),
            };
            let dialog = dialog.title(crate::ui::shell::dialog_title(title));
            let (body, footer) = match state.confirm {
                Some((user_id, question)) => {
                    this.add_special_question(state, user_id, question, cx)
                }
                None => (this.add_special_list(state, cx), None),
            };
            let dialog = match footer {
                Some(footer) => dialog.footer(footer),
                None => dialog,
            };
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    fn add_special_list(&self, state: &AddSpecialDialog, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let (filter, _) = Self::add_special_filter(state, cx);
        let me = self.session().and_then(|session| session.my_user_id);
        let fetch = self.session().and_then(|session| {
            session
                .groups
                .supergroup_members
                .get(&(state.chat_id.0, filter))
                .cloned()
        });
        let mut list = div()
            .id("add-special-list")
            .flex()
            .flex_col()
            .gap_0p5()
            .max_h(px(320.))
            .overflow_y_scroll()
            .track_scroll(&state.scroll)
            .on_scroll(cx.listener(|this, _, _, cx| this.add_special_load_more(cx)));
        let message =
            |text: &'static str| div().px_2().py_2().text_sm().text_color(muted).child(text);
        match fetch {
            None | Some(SupergroupMembersFetch::Loading) => {
                list = list.child(message(tr(keys::CONTACTS_LOADING)));
            }
            Some(SupergroupMembersFetch::Failed(error)) => {
                list = list.child(div().px_2().text_sm().text_color(muted).child(error));
            }
            Some(SupergroupMembersFetch::Loaded { members, .. }) => {
                let rows: Vec<ParsedChatMember> = members
                    .into_iter()
                    .filter(|member| {
                        !matches!(member.member_id, MessageSender::User { user_id } if Some(user_id) == me)
                            && matches!(member.member_id, MessageSender::User { .. })
                    })
                    .collect();
                if rows.is_empty() {
                    list = list.child(message(tr(keys::BLOCKED_LIST_NOT_FOUND)));
                }
                for member in rows {
                    list = list.child(self.add_special_row(member, cx));
                }
            }
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Input::new(&state.search).cleanable(true))
            .child(list)
            .into_any_element()
    }

    fn add_special_row(&self, member: ParsedChatMember, cx: &mut Context<Self>) -> AnyElement {
        let MessageSender::User { user_id } = member.member_id else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let status = match member.restriction {
            Some(restriction) => {
                super::super::restricted_exceptions::restricted_row_status(restriction)
            }
            None => Self::member_status_label(member.status).to_string(),
        };
        action_row(
            SharedString::from(format!("add-special-{user_id}")),
            None,
            self.special_member_name(user_id),
            false,
            cx,
        )
        .debug_selector(move || format!("add-special-{user_id}"))
        .child(div().flex_1())
        .child(div().text_xs().text_color(muted).child(status))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.add_special_pick(&member, window, cx);
        }))
        .into_any_element()
    }

    /// tdesktop's confirm and inform boxes, shown in place of the list.
    fn add_special_question(
        &self,
        state: &AddSpecialDialog,
        user_id: i64,
        question: SpecialConfirm,
        cx: &mut Context<Self>,
    ) -> (AnyElement, Option<AnyElement>) {
        let name = self.special_member_name(user_id);
        let text = match question {
            SpecialConfirm::SureBanAdmin => tr(keys::SURE_BAN_ADMIN).to_string(),
            SpecialConfirm::CantBanAdmin => tr(keys::ERROR_CANT_BAN_ADMIN).to_string(),
            SpecialConfirm::Kick => {
                let key = if self.group_flavor(state.chat_id) == Some(GroupFlavor::Channel) {
                    keys::PROFILE_SURE_KICK_CHANNEL
                } else {
                    keys::PROFILE_SURE_KICK
                };
                tr_with(key, &[("user", &name)])
            }
        };
        let back = cx.listener(|this, _, _, cx| {
            if let Some(dialog) = this.admin.add_special.as_mut() {
                dialog.confirm = None;
            }
            cx.notify();
        });
        let footer = match question {
            SpecialConfirm::CantBanAdmin => div()
                .flex()
                .justify_end()
                .child(
                    Button::new("add-special-ok")
                        .primary()
                        .label(tr(keys::BOX_OK))
                        .on_click(back),
                )
                .into_any_element(),
            SpecialConfirm::SureBanAdmin | SpecialConfirm::Kick => {
                let confirm = match state.role {
                    SpecialRole::Restricted => tr(keys::Q_PEOPLE_RESTRICT),
                    SpecialRole::Kicked => tr(keys::BOX_REMOVE),
                };
                let button = Button::new("add-special-confirm").label(confirm);
                let button = match state.role {
                    SpecialRole::Restricted => button.primary(),
                    SpecialRole::Kicked => button.danger(),
                };
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        div().debug_selector(|| "add-special-back".into()).child(
                            Button::new("add-special-back")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(back),
                        ),
                    )
                    .child(button.on_click(cx.listener(|this, _, window, cx| {
                        this.add_special_confirmed(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::AddSpecial, window, cx);
                    })))
                    .into_any_element()
            }
        };
        (div().text_sm().child(text).into_any_element(), Some(footer))
    }
}

crate::ui::shell::register_dialogs! {
    AddSpecial => DialogSpec::new(
        4295,
        |app| app.admin.add_special.is_some(),
        QuillApp::build_add_special_dialog,
    ),
}
