//! admin panels: management, invites, admins, boosts, sign flags.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::pressable::action_row;
use super::*;
use crate::ui::lean_buttons::ghost_text_button;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::state::{AdminListFetch, InviteLinkFetch, JoinRequestFetch, RequestPurpose};
use quill::telegram::envelope::{ChannelMemberStatus, ChatAdministratorEntry, ChatKind};
impl QuillApp {
    /// Slice G2: channel boost status (`getChatBoostStatus`, schema
    /// 1.8.67, line 13917) with the one-tap `boostChat` action. Shown
    /// for channels only; honest states: loading (request in flight),
    /// failed-with-retry, loaded "Level N · M boosts". `getChatBoostStatus`
    /// errors when boosts are unavailable for the chat — the failure
    /// renders instead of a fake number.
    pub(super) fn boost_section(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let is_channel = self.session().is_some_and(|session| {
            session.chats.get(&chat_id.0).is_some_and(|chat| {
                matches!(
                    chat.kind,
                    ChatKind::Supergroup {
                        is_channel: true,
                        ..
                    }
                )
            })
        });
        if !is_channel {
            return None;
        }
        let status = self
            .session()
            .and_then(|session| session.groups.chat_boost_status.get(&chat_id.0).copied());
        let in_flight = self.session().is_some_and(|session| {
            session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetChatBoostStatus, chat_id)
        });
        let mut section = div().flex().flex_col().w_full().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_PEOPLE_CHANNEL_BOOSTS)),
        );
        match status {
            Some((level, boost_count)) => {
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .w_full()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(trn_with(
                                    keys::Q_PEOPLE_LEVEL_BOOSTS,
                                    boost_count,
                                    &[("level", &level.to_string())],
                                )),
                        )
                        .child(ghost_text_button(
                            "g2-boost-channel",
                            tr(keys::Q_PEOPLE_BOOST_CHANNEL),
                            cx.listener(move |this, _, _, cx| {
                                this.boost_channel(chat_id, cx);
                            }),
                        )),
                );
            }
            None if in_flight => {
                section = section.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_PEOPLE_LOADING_BOOST_STATUS)),
                );
            }
            None => {
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .w_full()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(keys::Q_PEOPLE_BOOST_STATUS_UNAVAILABLE)),
                        )
                        .child(ghost_text_button(
                            "g2-boost-retry",
                            tr(keys::Q_PEOPLE_RETRY),
                            cx.listener(move |this, _, _, cx| {
                                this.refresh_boost_status(chat_id, cx);
                            }),
                        )),
                );
            }
        }
        Some(section.into_any_element())
    }

    /// Slice G2: re-request the boost status (bypasses the cache).
    pub(super) fn refresh_boost_status(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver
                .session
                .groups
                .chat_boost_status
                .remove(&chat_id.0);
            if live.driver.fetch_chat_boost_status(chat_id).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULD_NOT_LOAD_BOOST_STATUS,
                )));
            }
        }
        cx.notify();
    }

    /// The info panel's manage entry (tdesktop's "Manage group" /
    /// "Manage channel", which opens the Edit box), and Leave channel for a
    /// channel (`ActionsFiller::fillChannelActions`; groups leave from the
    /// action row and More). A member without the box still gets the
    /// member list.
    pub(super) fn group_management_section(
        &self,
        chat_id: ChatId,
        is_channel: bool,
        _is_basic_group: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use gpui_kit::assets::IconName as I;
        let session = self.session();
        let is_member = session
            .and_then(|s| s.chats.get(&chat_id.0))
            .and_then(|chat| chat.my_member_status)
            .is_some_and(|status| {
                matches!(
                    status,
                    ChannelMemberStatus::Creator
                        | ChannelMemberStatus::Administrator
                        | ChannelMemberStatus::Member
                        | ChannelMemberStatus::Restricted
                )
            });
        let mut section = div().flex().flex_col().w_full().gap_1();
        if session.is_some_and(|s| s.group_edit_box_available(chat_id)) {
            section = section.child(
                action_row(
                    "group-manage",
                    Some(I::Settings),
                    if is_channel {
                        tr(keys::Q_PEOPLE_MANAGE_CHANNEL)
                    } else {
                        tr(keys::MANAGE_GROUP_TITLE)
                    },
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_group_edit_box(chat_id, window, cx);
                })),
            );
        } else if is_member {
            section = section.child(
                action_row(
                    "g1-open-members",
                    Some(I::Users),
                    if is_channel {
                        tr(keys::Q_PEOPLE_SUBSCRIBERS)
                    } else {
                        tr(keys::Q_PEOPLE_MEMBERS)
                    },
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_member_dialog(chat_id, window, cx);
                })),
            );
        }
        if is_member && is_channel {
            section = section.child(
                action_row(
                    "g1-leave-chat",
                    Some(I::LogOut),
                    tr(keys::Q_PEOPLE_LEAVE_CHANNEL),
                    true,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    // An owner sees who inherits first (tdesktop
                    // `select_future_owner_box`).
                    if !this.open_owner_leave(chat_id, window, cx) {
                        this.open_group_confirm(chat_id, GroupConfirmAction::LeaveChat, cx);
                    }
                }))
                .test_support(),
            );
        }
        section.into_any_element()
    }

    /// Phase D3a: human expiry for an invite link (`expiration_date` is a
    /// unix timestamp; 0 = never expires).
    pub(super) fn invite_link_expiry(expiration_date: i32) -> String {
        if expiration_date == 0 {
            return tr(keys::Q_PEOPLE_NEVER_EXPIRES).into();
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        let remaining = i64::from(expiration_date) - now;
        if remaining <= 0 {
            tr(keys::GROUP_INVITE_LINK_EXPIRED).into()
        } else if remaining < 86_400 {
            tr(keys::Q_PEOPLE_EXPIRES_TODAY).into()
        } else if remaining < 172_800 {
            tr(keys::Q_PEOPLE_EXPIRES_TOMORROW).into()
        } else {
            tr_with(
                keys::Q_PEOPLE_EXPIRES_IN_DAYS,
                &[("remaining_86_400", &(remaining / 86_400).to_string())],
            )
        }
    }
}

mod invite_links_section;
