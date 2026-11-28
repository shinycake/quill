//! Telegram Desktop's delete box for one message or one album, with the
//! admin checkboxes of a group moderator (`DeleteMessagesBox`,
//! `boxes/moderate_messages_box.cpp`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::lang::{keys, tr, tr_with, trn};

impl QuillApp {
    /// Telegram Desktop's delete box for one message: "Do you want to delete
    /// this message?" with "Also delete for {name}" (private chats) or
    /// "Delete for everyone", checked by default, when deleting for everyone
    /// is possible.
    pub(in crate::ui) fn open_delete_dialog(
        &mut self,
        confirm: quill::composer::DeleteConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let one = vec![confirm.message_id];
        self.open_delete_dialog_with(confirm, one, None, window, cx);
    }

    /// The delete box for `ids` (`confirm`'s message alone, or the whole
    /// album it is in: `deleteAsGroup`), with the admin checkboxes of a
    /// group moderator (Report Spam, Delete all from the user, Ban the
    /// user) when `moderation` offers them; they act on every id.
    pub(in crate::ui) fn open_delete_dialog_with(
        &mut self,
        confirm: quill::composer::DeleteConfirm,
        ids: Vec<MessageId>,
        moderation: Option<crate::ui::message_menu_ui::ModerationOffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = if ids.is_empty() {
            vec![confirm.message_id]
        } else {
            ids
        };
        let question = if ids.len() == 1 {
            tr(keys::SELECTED_DELETE_SURE_THIS).to_string()
        } else {
            trn(keys::Q_CHAT_DO_YOU_WANT_TO_DELETE, ids.len())
        };
        // A secret chat always deletes for both, so there is nothing to choose.
        let secret = self
            .session()
            .is_some_and(|s| s.is_secret_chat(confirm.chat_id));
        let account = self.session().map(|s| s.account.clone());
        let label = (confirm.can_revoke && !secret).then(|| {
            let session = self.session();
            match session
                .and_then(|s| s.chats.get(&confirm.chat_id.0))
                .map(|chat| &chat.kind)
            {
                Some(quill::telegram::envelope::ChatKind::Private { user_id }) => session
                    .and_then(|s| s.user(user_id.0))
                    .map(|user| {
                        tr_with(
                            keys::Q_CHAT_ALSO_DELETE_FOR,
                            &[("first_name", &user.first_name.to_string())],
                        )
                    })
                    .unwrap_or_else(|| tr(keys::Q_CHAT_DELETE_FOR_EVERYONE).into()),
                _ => tr(keys::Q_CHAT_DELETE_FOR_EVERYONE).into(),
            }
        });
        let revoke = std::rc::Rc::new(std::cell::Cell::new(confirm.revoke));
        // A paid suggested post deleted too early loses its payment.
        let warning = self.paid_post_delete_warning(&confirm);
        // Report Spam / Delete all / Ban: unchecked until the admin ticks them.
        let choice = std::rc::Rc::new(std::cell::Cell::new(
            quill::connect::ModerationChoice::default(),
        ));
        let community_ban = moderation.as_ref().and_then(|offer| {
            self.want_community_for_moderation(offer.chat_id.0);
            self.moderation_ban_community(offer.chat_id.0)
        });
        let ban_from_community = std::rc::Rc::new(std::cell::Cell::new(false));
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, revoke, confirm) = (app.clone(), revoke.clone(), confirm.clone());
            let (ids, question) = (ids.clone(), question.clone());
            let account = account.clone();
            let checkbox_state = revoke.clone();
            let (moderation_state, moderation_offer) = (choice.clone(), moderation.clone());
            let on_ok_choice = choice.clone();
            let on_ok_offer = moderation.clone();
            let community_state = ban_from_community.clone();
            let on_ok_community = ban_from_community.clone();
            alert
                .when_some(warning, |alert, (title, text)| {
                    alert.title(title).description(
                        div()
                            .debug_selector(|| "delete-paid-warning".to_string())
                            .child(text),
                    )
                })
                .when(warning.is_none(), |alert| {
                    alert.description(question.clone())
                })
                .when(label.is_some() || moderation_offer.is_some(), |alert| {
                    let label = label.clone();
                    let community_state = community_state.clone();
                    alert.content(move |content, _, cx| {
                        let state = checkbox_state.clone();
                        let mut content = content;
                        if let Some(label) = label.clone() {
                            content = content.child(
                                gpui_kit::component::checkbox::Checkbox::new(
                                    "delete-message-revoke",
                                )
                                .label(label)
                                .checked(state.get())
                                .on_click(
                                    move |checked, window, _| {
                                        state.set(*checked);
                                        window.refresh();
                                    },
                                ),
                            );
                        }
                        if let Some(offer) = moderation_offer.clone() {
                            type Get = fn(quill::connect::ModerationChoice) -> bool;
                            type Set = fn(&mut quill::connect::ModerationChoice, bool);
                            let name = offer.user_name.clone();
                            let rows: [(&'static str, String, bool, Get, Set); 3] = [
                                (
                                    "delete-moderate-spam",
                                    tr(keys::Q_CHAT_REPORT_SPAM_2).to_string(),
                                    offer.options.report_spam,
                                    |c| c.report_spam,
                                    |c, v| c.report_spam = v,
                                ),
                                (
                                    "delete-moderate-all",
                                    tr_with(
                                        keys::DELETE_ALL_FROM_USER,
                                        &[("user", &name.to_string())],
                                    ),
                                    offer.options.delete_all,
                                    |c| c.delete_all,
                                    |c, v| c.delete_all = v,
                                ),
                                (
                                    "delete-moderate-ban",
                                    tr_with(keys::Q_CHAT_BAN, &[("name", &name.to_string())]),
                                    offer.options.ban_or_restrict,
                                    |c| c.ban,
                                    |c, v| c.ban = v,
                                ),
                            ];
                            for (id, text, available, get, set) in rows {
                                if !available {
                                    continue;
                                }
                                let state = moderation_state.clone();
                                let checked = get(state.get());
                                content = content.child(
                                    gpui_kit::component::checkbox::Checkbox::new(id)
                                        .label(text)
                                        .checked(checked)
                                        .on_click(move |checked, window, _| {
                                            let mut now = state.get();
                                            set(&mut now, *checked);
                                            state.set(now);
                                            window.refresh();
                                        }),
                                );
                            }
                            // tdesktop's expander under "Ban": keep the
                            // member, take away what they may send.
                            if offer.can_restrict_instead && moderation_state.get().ban {
                                let state = moderation_state.clone();
                                content = content.child(
                                    gpui_kit::component::checkbox::Checkbox::new(
                                        "delete-moderate-restrict",
                                    )
                                    .label(tr(keys::Q_CHAT_ONLY_RESTRICT_THEY_STAY_AND))
                                    .checked(state.get().restrict_instead)
                                    .on_click(
                                        move |checked, window, _| {
                                            let mut now = state.get();
                                            now.restrict_instead = *checked;
                                            state.set(now);
                                            window.refresh();
                                        },
                                    ),
                                );
                            }
                        }
                        if let Some((_, chats)) = community_ban {
                            let state = community_state.clone();
                            content = content.child(
                                gpui_kit::component::checkbox::Checkbox::new(
                                    "delete-moderate-community-ban",
                                )
                                .label(tr(keys::COMMUNITY_BAN_TOGGLE))
                                .checked(state.get())
                                .on_click(
                                    move |checked, window, _| {
                                        state.set(*checked);
                                        window.refresh();
                                    },
                                ),
                            );
                            if community_state.get() && chats > 0 {
                                content = content.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(quill::lang::trn(keys::COMMUNITY_BAN_ABOUT, chats)),
                                );
                            }
                        }
                        content
                    })
                })
                .ok_text(if warning.is_some() {
                    quill::suggested_posts::DELETE_ANYWAY
                } else {
                    tr(keys::BOX_DELETE)
                })
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let ids = ids.clone();
                    let mut confirm = confirm.clone();
                    confirm.revoke = confirm.can_revoke && revoke.get();
                    let offer = on_ok_offer.clone();
                    let picked = match &offer {
                        Some(offer) => on_ok_choice.get().clamp(offer.options),
                        None => quill::connect::ModerationChoice::default(),
                    };
                    let community = community_ban
                        .filter(|_| on_ok_community.get())
                        .zip(offer.clone());
                    let mut refused = false;
                    let _ = app.update(cx, |this, cx| {
                        // Opened for one account: a switch since cancels it.
                        if this.session().map(|s| &s.account) != account.as_ref() {
                            return;
                        }
                        // Reports and "delete all" name the user's
                        // messages, so they go out before the delete. A
                        // send that fails keeps the box open and deletes
                        // nothing: the admin retries (every step is
                        // idempotent on the server).
                        if let (true, Some(offer), Some(live)) =
                            (picked.any(), offer.clone(), this.live.as_mut())
                            && live
                                .driver
                                .moderate_message(offer.chat_id, &ids, offer.user_id, picked)
                                .is_err()
                        {
                            refused = true;
                            this.set_note(StatusNote::error(tr(
                                keys::Q_CHAT_COULD_NOT_DELETE_MESSAGES,
                            )));
                            cx.notify();
                            return;
                        }
                        if let Some(((community_id, _), offer)) = community {
                            this.start_community_ban(
                                community_id,
                                quill::state::CommunityMember::User(offer.user_id),
                                offer.user_name,
                                cx,
                            );
                        }
                        if let [_, _, ..] = ids.as_slice() {
                            this.delete_messages(confirm.chat_id, &ids, confirm.revoke);
                            cx.notify();
                        } else {
                            this.confirm_delete(confirm, cx);
                        }
                    });
                    !refused
                })
        });
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "delete_dialog_tests.rs"]
mod tests;
