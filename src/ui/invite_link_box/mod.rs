//! The New Link / Edit Link box (tdesktop `EditInviteLinkBox`): the
//! monthly fee of a channel link, "Limit by time period" and "Limit by
//! number of users" with their presets and Custom values, admin approval
//! and the link name. The rules live in `quill::invite_link_form`; this
//! module holds the fields, opens the Custom boxes and sends the request.
#![deny(clippy::indexing_slicing)]

mod custom;
mod sections;

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use crate::ui::scheduled::TimePicker;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::invite_link_form::{
    ChatFacts, InviteLinkForm, InviteLinkRequest, NAME_MAX, STARS_MAX_DEFAULT, stars_amount,
};
use quill::lang::{keys, tr};
use quill::local_time::now_unix;
use quill::telegram::envelope::ParsedChatInviteLink;

/// The open box: its choices and its two text fields.
pub(crate) struct InviteLinkBox {
    pub(crate) chat_id: ChatId,
    pub(crate) form: InviteLinkForm,
    pub(crate) name_input: Entity<InputState>,
    pub(crate) stars_input: Entity<InputState>,
    /// A Custom box shown in place of this one.
    pub(crate) custom: Option<CustomBox>,
    /// The Custom number field takes focus on its box's next frame.
    pub(crate) focus_custom: bool,
}

/// tdesktop's boxes over the link box: the date and time picker, and the
/// number of users.
pub(crate) enum CustomBox {
    Date(TimePicker),
    Limit(Entity<InputState>),
}

impl QuillApp {
    pub(crate) fn invite_link_box(&self) -> Option<&InviteLinkBox> {
        self.admin.invite_link_box.as_ref()
    }

    fn invite_link_box_mut(&mut self) -> Option<&mut InviteLinkBox> {
        self.admin.invite_link_box.as_mut()
    }

    fn invite_link_facts(&self, chat_id: ChatId) -> ChatFacts {
        let Some(session) = self.session() else {
            return ChatFacts::default();
        };
        let current = session.group_type_current(chat_id);
        ChatFacts {
            group: !session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| chat.kind.is_channel()),
            public: current.public,
            global_request_approval: current.request_to_join,
        }
    }

    /// The dearest monthly fee (`subscription_star_count_max`).
    pub(super) fn invite_link_stars_max(&self) -> i64 {
        self.session()
            .and_then(|s| s.groups.subscription_star_max)
            .unwrap_or(STARS_MAX_DEFAULT)
    }

    /// tdesktop `EditLinkBox`: a new link, or `link` to edit.
    pub(in crate::ui) fn open_invite_link_box(
        &mut self,
        chat_id: ChatId,
        link: Option<&ParsedChatInviteLink>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let facts = self.invite_link_facts(chat_id);
        let form = match link {
            Some(link) => InviteLinkForm::edit_link(facts, link),
            None => InviteLinkForm::new_link(facts),
        };
        let name = link.map(|link| link.name.clone()).unwrap_or_default();
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::GROUP_INVITE_LABEL_HEADER))
                .validate(|text, _| text.chars().count() <= NAME_MAX)
                .default_value(name)
        });
        let stars = form
            .locked_stars
            .map(|stars| stars.to_string())
            .unwrap_or_default();
        let stars_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::GROUP_INVITE_SUBSCRIPTION_PH))
                .validate(|text, _| text.chars().all(|c| c.is_ascii_digit()))
                .default_value(stars)
        });
        cx.subscribe_in(
            &stars_input,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let max = this.invite_link_stars_max();
                    let text = input.read(cx).value().to_string();
                    if text.trim().parse::<i64>().is_ok_and(|stars| stars > max) {
                        let capped = max.to_string();
                        input.update(cx, |input, cx| input.set_value(capped, window, cx));
                    }
                    cx.notify();
                }
            },
        )
        .detach();
        if form.offers_subscription()
            && self
                .session()
                .is_some_and(|s| s.groups.stars_usd_rate.is_none())
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.ensure_earn_app_config();
        }
        self.admin.invite_link_box = Some(InviteLinkBox {
            chat_id,
            form,
            name_input,
            stars_input,
            custom: None,
            focus_custom: false,
        });
        cx.notify();
    }

    pub(in crate::ui) fn close_invite_link_box(&mut self, cx: &mut Context<Self>) {
        self.admin.invite_link_box = None;
        cx.notify();
    }

    pub(super) fn edit_invite_link_form(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut InviteLinkForm),
    ) {
        if let Some(link_box) = self.invite_link_box_mut() {
            change(&mut link_box.form);
        }
        cx.notify();
    }

    /// tdesktop `addSaveButtons`: send the box's request and close it.
    pub(in crate::ui) fn save_invite_link_box(&mut self, cx: &mut Context<Self>) {
        let Some(link_box) = self.admin.invite_link_box.take() else {
            return;
        };
        let name = link_box.name_input.read(cx).value().to_string();
        let stars = stars_amount(
            &link_box.stars_input.read(cx).value(),
            self.invite_link_stars_max(),
        );
        let request = link_box.form.request(&name, stars, now_unix());
        let chat_id = link_box.chat_id;
        let creating = matches!(
            request,
            InviteLinkRequest::Create { .. } | InviteLinkRequest::CreateSubscription { .. }
        );
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_INVITE_LINKS_NEED_A_LIVE,
            )));
            cx.notify();
            return;
        };
        let driver = &mut live.driver;
        let sent = match &request {
            InviteLinkRequest::Create {
                name,
                expiration_date,
                member_limit,
                creates_join_request,
            } => driver
                .create_chat_invite_link(
                    chat_id,
                    name,
                    *expiration_date,
                    *member_limit,
                    *creates_join_request,
                )
                .map(drop),
            InviteLinkRequest::Edit {
                link,
                name,
                expiration_date,
                member_limit,
                creates_join_request,
            } => driver
                .edit_chat_invite_link(
                    chat_id,
                    link,
                    name,
                    *expiration_date,
                    *member_limit,
                    *creates_join_request,
                )
                .map(drop),
            InviteLinkRequest::CreateSubscription { name, star_count } => driver
                .create_chat_subscription_invite_link(chat_id, name, *star_count)
                .map(drop),
            InviteLinkRequest::EditSubscription { link, name } => driver
                .edit_chat_subscription_invite_link(chat_id, link, name)
                .map(drop),
        };
        let note = match (sent, creating) {
            (Ok(()), true) => StatusNote::info(tr(keys::Q_PEOPLE_CREATING_INVITE_LINK)),
            (Ok(()), false) => StatusNote::info(tr(keys::Q_PEOPLE_SAVING_INVITE_LINK)),
            (Err(_), true) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_CREATE_INVITE_LINK)),
            (Err(_), false) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_SAVE_INVITE_LINK)),
        };
        self.connection.set_note(note);
        cx.notify();
    }

    fn invite_link_box_open(&self) -> bool {
        self.invite_link_box()
            .is_some_and(|link_box| link_box.custom.is_none())
    }

    fn build_invite_link_box_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::InviteLinkBox, |this, _, cx| {
                this.close_invite_link_box(cx);
            });
        app.update(cx, |this, cx| {
            let editing = this
                .invite_link_box()
                .is_some_and(|link_box| link_box.form.editing());
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(if editing {
                    tr(keys::GROUP_INVITE_EDIT_TITLE)
                } else {
                    tr(keys::GROUP_INVITE_NEW_TITLE)
                }));
            if this.invite_link_box().is_none() {
                return dialog.on_close(on_close);
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("invite-link-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_invite_link_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::InviteLinkBox, window, cx);
                        })),
                )
                .child(
                    Button::new("invite-link-save")
                        .label(if editing {
                            tr(keys::SETTINGS_SAVE)
                        } else {
                            tr(keys::FORMATTING_LINK_CREATE)
                        })
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_invite_link_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::InviteLinkBox, window, cx);
                        })),
                );
            let body_app = cx.entity();
            dialog
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, cx| {
                        let body = body_app
                            .update(cx, |this, cx| this.invite_link_box_body(cx))
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// "New Link" / "Edit Link"; over the invite links page, which it
    /// returns to.
    InviteLinkBox => DialogSpec::new(
        4870,
        |app| app.invite_link_box_open(),
        QuillApp::build_invite_link_box_dialog,
    ),
}

mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod ui_tests;
