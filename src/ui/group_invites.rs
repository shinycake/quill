//! invite links + join requests.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Slice G1: `replacePrimaryChatInviteLink` (schema 1.8.67, line
    /// 14089) — revokes the current primary link and creates a fresh
    /// one; the new link arrives as `updateChatInviteLink`.
    pub(super) fn replace_primary_invite_link(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let note = match self.live.as_mut() {
            Some(live) => match live.driver.replace_primary_chat_invite_link(chat_id) {
                Ok(_) => StatusNote::info(tr(keys::Q_PEOPLE_REPLACING_PRIMARY_INVITE_LINK)),
                Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_REPLACE_INVITE_LINK)),
            },
            None => StatusNote::info(tr(keys::Q_PEOPLE_INVITE_LINKS_NEED_A_LIVE)),
        };
        self.connection.set_note(note);
        cx.notify();
    }

    /// Slice G1: `toggleSupergroupJoinByRequest` (schema 1.8.67, line
    /// 15188).
    pub(super) fn toggle_join_by_request(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let current = self.chat_join_by_request(chat_id);
        let note = match self.live.as_mut() {
            Some(live) => {
                match live
                    .driver
                    .toggle_supergroup_join_by_request(chat_id, !current)
                {
                    Ok(_) => {
                        if current {
                            StatusNote::silent(tr(keys::Q_PEOPLE_JOIN_REQUESTS_DISABLED))
                        } else {
                            StatusNote::silent(tr(keys::Q_PEOPLE_JOIN_REQUESTS_ENABLED))
                        }
                    }
                    Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_TOGGLE_JOIN_REQUESTS)),
                }
            }
            None => StatusNote::info(tr(keys::Q_PEOPLE_JOIN_REQUESTS_NEED_A_LIVE)),
        };
        self.connection.set_note(note);
        cx.notify();
    }

    /// Phase D3a: open the invite-link create dialog for a chat.
    pub(super) fn open_invite_link_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let allow_subscription = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.chat_supports_subscription_links(chat_id))
            || self
                .session()
                .and_then(|s| s.chats.get(&chat_id.0))
                .is_some_and(|chat| chat.kind.is_channel());
        self.admin.invite_link_dialog = Some(InviteLinkDialog::new(
            window,
            cx,
            chat_id,
            allow_subscription,
        ));
        cx.notify();
    }

    /// B8: rename a subscription link (the only editable field).
    pub(super) fn open_subscription_link_rename(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut dialog = InviteLinkDialog::new(window, cx, chat_id, false);
        dialog.edit_link = Some(invite_link.to_owned());
        dialog
            .name_input
            .update(cx, |input, cx| input.set_value(name.to_owned(), window, cx));
        self.admin.invite_link_dialog = Some(dialog);
        cx.notify();
    }

    /// Phase D3a: close the invite-link create dialog.
    pub(super) fn close_invite_link_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.invite_link_dialog = None;
        cx.notify();
    }

    /// Phase D3a: validate the dialog and send `createChatInviteLink`.
    pub(super) fn submit_invite_link_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(dialog) = self.admin.invite_link_dialog.as_ref() {
            let chat_id = dialog.chat_id;
            let name = dialog.name_input.read(cx).value().to_string();
            let edit_link = dialog.edit_link.clone();
            let stars_text = dialog.stars_input.read(cx).value().to_string();
            let stars: i64 = stars_text.trim().parse().unwrap_or(0);
            if edit_link.is_some() || (dialog.allow_subscription && stars > 0) {
                let result = match (self.live.as_mut(), edit_link) {
                    (Some(live), Some(link)) => Some(
                        live.driver
                            .edit_chat_subscription_invite_link(chat_id, &link, &name),
                    ),
                    (Some(live), None) => Some(
                        live.driver
                            .create_chat_subscription_invite_link(chat_id, &name, stars),
                    ),
                    (None, _) => None,
                };
                self.admin.invite_link_dialog = None;
                self.connection.set_note(match result {
                    Some(Ok(_)) => StatusNote::info(tr(keys::Q_PEOPLE_SAVING_INVITE_LINK)),
                    Some(Err(_)) => {
                        StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_SAVE_INVITE_LINK))
                    }
                    None => StatusNote::info(tr(keys::Q_PEOPLE_INVITE_LINKS_NEED_A_LIVE)),
                });
                cx.notify();
                return;
            }
            if !stars_text.trim().is_empty() && stars_text.trim().parse::<i64>().is_err() {
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_STARS_PRICE_MUST_BE_A)));
                cx.notify();
                return;
            }
        }
        let (chat_id, name, expiration_date, member_limit, creates_join_request) =
            match self.admin.invite_link_dialog.as_ref() {
                Some(dialog) => {
                    let name = dialog.name_input.read(cx).value().to_string();
                    let days = dialog.expiration_days_input.read(cx).value().to_string();
                    let limit = dialog.member_limit_input.read(cx).value().to_string();
                    let days: i64 = match days.trim().parse() {
                        Ok(days) if days >= 0 => days,
                        _ => {
                            self.connection.set_note(StatusNote::silent(tr(
                                keys::Q_PEOPLE_EXPIRATION_MUST_BE_A_NON,
                            )));
                            cx.notify();
                            return;
                        }
                    };
                    let member_limit: i32 = match limit.trim().parse() {
                        Ok(limit) if limit >= 0 => limit,
                        _ => {
                            self.connection.set_note(StatusNote::info(tr(
                                keys::Q_PEOPLE_MEMBER_LIMIT_MUST_BE_A,
                            )));
                            cx.notify();
                            return;
                        }
                    };
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_secs() as i64)
                        .unwrap_or(0);
                    let expiration_date = if days == 0 {
                        0
                    } else {
                        now.saturating_add(days.saturating_mul(86_400))
                            .min(i64::from(i32::MAX)) as i32
                    };
                    (
                        dialog.chat_id,
                        name,
                        expiration_date,
                        member_limit,
                        dialog.creates_join_request,
                    )
                }
                None => return,
            };
        if let Some(live) = self.live.as_mut() {
            match live.driver.create_chat_invite_link(
                chat_id,
                &name,
                expiration_date,
                member_limit,
                creates_join_request,
            ) {
                Ok(_) => {
                    self.admin.invite_link_dialog = None;
                    self.connection
                        .set_note(StatusNote::info(tr(keys::Q_PEOPLE_CREATING_INVITE_LINK)));
                }
                Err(_) => {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_PEOPLE_COULD_NOT_CREATE_INVITE_LINK,
                    )));
                }
            }
        } else {
            // Screenshot demos have no live driver; close the dialog honestly.
            self.admin.invite_link_dialog = None;
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_INVITE_LINKS_NEED_A_LIVE,
            )));
        }
        let _ = window;
        cx.notify();
    }

    /// Phase D3a: the invite-link creation dialog, rendered above the
    /// composer like the poll dialog.
    pub(super) fn invite_link_dialog_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.admin.invite_link_dialog.as_ref()?;
        self.invite_link_dialog_panel_body(cx)
    }

    /// `invite_link_dialog_panel` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    #[inline(never)]
    fn invite_link_dialog_panel_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.admin.invite_link_dialog.as_ref()?;
        let creates_join_request = dialog.creates_join_request;
        let editing = dialog.edit_link.is_some();
        let title = if editing {
            tr(keys::Q_PEOPLE_RENAME_SUBSCRIPTION_LINK)
        } else {
            tr(keys::Q_PEOPLE_NEW_INVITE_LINK)
        };
        let panel = div()
            .id("invite-link-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(div().text_sm().font_semibold().child(title))
            .child(
                Textarea::new(&dialog.name_input)
                    .aria_label(tr(keys::Q_PEOPLE_INVITE_LINK_NAME))
                    .h(px(40.)),
            )
            .when(!editing, |panel| {
                panel
                    .child(
                        Textarea::new(&dialog.expiration_days_input)
                            .aria_label(tr(keys::Q_PEOPLE_INVITE_LINK_DURATION_IN_DAYS))
                            .h(px(40.)),
                    )
                    .child(
                        Textarea::new(&dialog.member_limit_input)
                            .aria_label(tr(keys::Q_PEOPLE_INVITE_LINK_MEMBER_LIMIT))
                            .h(px(40.)),
                    )
                    .child(
                        // Phase 6: kit Checkbox (was: ghost button with a
                        // check label). Controlled: writes the requested
                        // value.
                        Checkbox::new("invite-link-dialog-toggle-join-request")
                            .label(tr(keys::Q_PEOPLE_APPROVAL_REQUIRED_TO_JOIN))
                            .checked(creates_join_request)
                            .on_click(cx.listener(|this, &on, _, cx| {
                                if let Some(dialog) = this.admin.invite_link_dialog.as_mut() {
                                    dialog.creates_join_request = on;
                                }
                                cx.notify();
                            })),
                    )
            })
            .when(dialog.allow_subscription && !editing, |panel| {
                // tdesktop "Require Monthly Fee": a Stars price makes a
                // 30-day subscription link; expiry and limit do not apply.
                panel
                    .child(
                        Textarea::new(&dialog.stars_input)
                            .aria_label(tr(keys::Q_PEOPLE_STARS_PER_MONTH))
                            .h(px(40.)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child(tr(keys::Q_PEOPLE_A_STARS_PRICE_CHARGES_PEOPLE)),
                    )
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("invite-link-dialog-create")
                            .label(if editing {
                                tr(keys::SETTINGS_SAVE)
                            } else {
                                tr(keys::Q_PEOPLE_CREATE_LINK)
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_invite_link_dialog(window, cx);
                            })),
                    )
                    .child(
                        Button::new("invite-link-dialog-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_invite_link_dialog(cx);
                            })),
                    ),
            );
        Some(panel.into_any_element())
    }
}
