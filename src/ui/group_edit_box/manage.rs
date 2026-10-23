//! The Edit box's manage rows: the Quill Manage dialogs (settings,
//! members, permissions, invite link, stickers, topics, welcome, ...)
//! hosted in the box as tdesktop lists them, and Delete at the bottom.
#![deny(clippy::indexing_slicing)]

use crate::ui::GroupConfirmAction;
use crate::ui::app::QuillApp;
use crate::ui::pressable::action_row;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::RequestPurpose;
use quill::telegram::envelope::{ChannelMemberStatus, ChatKind};

impl QuillApp {
    fn group_sticker_choices(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
        // The caller only builds this section with a session.
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let info = session.chats.get(&chat_id.0).and_then(|c| match c.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                session.supergroup_full_info(supergroup_id)
            }
            _ => None,
        });
        let busy = [
            RequestPurpose::SetSupergroupStickerSet,
            RequestPurpose::SetSupergroupCustomEmojiStickerSet,
        ]
        .iter()
        .any(|p| session.requests.has_purpose_for_chat(*p, chat_id));
        let loading = [
            RequestPurpose::GetInstalledStickerSets,
            RequestPurpose::GetInstalledEmojiSets,
        ]
        .iter()
        .any(|p| session.requests.has_purpose(*p));
        let mut section = div().flex().flex_col().gap_1().child(
            Button::new("group-load-sticker-packs")
                .label(if loading {
                    tr(keys::Q_PEOPLE_LOADING_PACKS)
                } else {
                    tr(keys::Q_PEOPLE_LOAD_REFRESH_GROUP_PACKS)
                })
                .ghost()
                .disabled(loading)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut()
                        && live.driver.load_group_sticker_choices(chat_id).is_err()
                    {
                        this.connection.set_note(StatusNote::error(tr(
                            keys::Q_PEOPLE_COULD_NOT_LOAD_GROUP_PACKS,
                        )));
                    }
                    cx.notify();
                })),
        );
        if session.stickers.stickers.failed || session.stickers.emoji.failed {
            section = section.child(
                div()
                    .text_sm()
                    .child(tr(keys::Q_PEOPLE_COULD_NOT_LOAD_PACKS_RETRY)),
            );
        }
        for (custom, title, current, sets) in [
            (
                false,
                tr(keys::GROUP_STICKERS),
                info.map_or(0, |i| i.sticker_set_id),
                &session.stickers.stickers.sets,
            ),
            (
                true,
                tr(keys::EMOJI_GROUP_BADGE),
                info.map_or(0, |i| i.custom_emoji_sticker_set_id),
                &session.stickers.emoji.installed_sets,
            ),
        ] {
            let current_label = if current == 0 {
                tr(keys::Q_PEOPLE_NONE).to_string()
            } else {
                sets.iter()
                    .find(|s| s.id == current)
                    .map(|s| s.title.clone())
                    .unwrap_or_else(|| {
                        tr_with(keys::Q_PEOPLE_PACK, &[("current", &current.to_string())])
                    })
            };
            let mut choices = div()
                .id(if custom {
                    "group-emoji-choices"
                } else {
                    "group-sticker-choices"
                })
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(160.))
                .overflow_y_scroll()
                .child(div().text_sm().child(format!("{title}: {current_label}")));
            for (id, label) in std::iter::once((0, tr(keys::Q_PEOPLE_REMOVE_PACK).to_string()))
                .chain(sets.iter().map(|s| (s.id, s.title.clone())))
            {
                choices = choices.child(
                    Button::new(format!("group-pack-{custom}-{id}"))
                        .label(label)
                        .ghost()
                        .selected(id == current)
                        .disabled(busy || id == current)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(live) = this.live.as_mut() {
                                let result = if custom {
                                    live.driver
                                        .set_supergroup_custom_emoji_sticker_set(chat_id, id)
                                } else {
                                    live.driver.set_supergroup_sticker_set(chat_id, id)
                                };
                                this.connection.set_note(match result {
                                    Ok(Some(_)) => StatusNote::silent(tr(
                                        keys::Q_PEOPLE_GROUP_PACK_CHANGE_REQUESTED,
                                    )),
                                    Ok(None) => StatusNote::info(tr(
                                        keys::Q_PEOPLE_GROUP_PACK_CHANGE_IS_UNAVAILABLE,
                                    )),
                                    Err(_) => StatusNote::error(tr(
                                        keys::Q_PEOPLE_COULD_NOT_CHANGE_GROUP_PACK,
                                    )),
                                });
                            }
                            cx.notify();
                        })),
                );
            }
            section = section.child(choices);
        }
        section.into_any_element()
    }

    /// The Edit box's manage rows under the name and description
    /// (tdesktop `Controller::fillManageSection`): each Quill Manage
    /// dialog the viewer's rights allow, then Delete.
    pub(super) fn group_edit_box_manage_rows(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let kind = session
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|chat| &chat.kind);
        let is_channel = kind.is_some_and(|kind| kind.is_channel());
        let is_basic_group = matches!(kind, Some(ChatKind::BasicGroup { .. }));
        let can_restrict = session.is_some_and(|s| s.chat_can_restrict_members(chat_id));
        let is_owner = session.is_some_and(|s| s.chat_is_owner(chat_id));
        let can_invite = session.is_some_and(|s| s.chat_can_invite_users(chat_id));
        let can_add = session.is_some_and(|s| s.chat_can_add_members(chat_id));
        let can_delete = session
            .as_ref()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.can_be_deleted_for_all_users);
        let is_member = session
            .as_ref()
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
        use gpui_kit::assets::IconName as I;
        // Each row is built by a function of its own, so this section's
        // frame does not hold every row's builder chain at once
        // (docs/decisions/main-thread-stack.md).
        macro_rules! row {
            ($id:expr, $icon:expr, $label:expr, |$this:ident, $window:ident, $cx:ident| $action:block) => {
                section = section.child(management_row(
                    $id,
                    $icon,
                    $label,
                    false,
                    cx.listener(move |$this, _, $window, $cx| $action),
                    cx,
                ));
            };
        }
        macro_rules! danger_row {
            ($id:expr, $icon:expr, $label:expr, |$this:ident, $window:ident, $cx:ident| $action:block) => {
                section = section.child(management_row(
                    $id,
                    $icon,
                    $label,
                    true,
                    cx.listener(move |$this, _, $window, $cx| $action),
                    cx,
                ));
            };
        }
        macro_rules! toggle_row {
            ($id:expr, $icon:expr, $label:expr, $on:expr, |$this:ident, $window:ident, $cx:ident| $action:block) => {
                section = section.child(management_toggle_row(
                    $id,
                    $icon,
                    $label,
                    $on,
                    cx.listener(move |$this, _, $window, $cx| $action),
                    cx,
                ));
            };
        }
        // B7: one entry for the settings the viewer's rights allow
        // (topics, history, reactions, discussion group, ...).
        if session.is_some_and(|s| !s.group_admin_controls(chat_id).is_empty()) {
            row!(
                "b7-open-settings",
                I::Settings,
                if is_channel {
                    tr(keys::ADMIN_LOG_FILTER_ACTIONS_CHANNEL_SETTINGS_SECTION)
                } else {
                    tr(keys::ADMIN_LOG_FILTER_ACTIONS_SETTINGS_SECTION)
                },
                |this, _window, cx| {
                    this.open_group_settings_dialog(chat_id, cx);
                }
            );
        }
        // Members / subscribers — everyone who can see the panel and
        // add or restrict may manage; plain members get a read-only
        // list through the dialog's All tab.
        if can_add || can_restrict || is_member {
            let label = if is_channel {
                tr(keys::Q_PEOPLE_SUBSCRIBERS)
            } else {
                tr(keys::Q_PEOPLE_MEMBERS)
            };
            row!("g1-open-members", I::Users, label, |this, window, cx| {
                this.open_member_dialog(chat_id, window, cx);
            });
        }
        // Channels have no member permissions (tdesktop lists them for
        // groups only).
        if can_restrict && !is_channel {
            row!(
                "g1-open-permissions",
                I::Shield,
                tr(keys::Q_PEOPLE_DEFAULT_PERMISSIONS),
                |this, _window, cx| {
                    this.open_permissions_dialog(chat_id, cx);
                }
            );
        }
        if !is_channel
            && !is_basic_group
            && session.is_some_and(|s| s.chat_can_set_sticker_set(chat_id))
        {
            section = section.child(self.group_sticker_choices(chat_id, cx));
        }
        if can_invite {
            row!(
                "g1-replace-invite-link",
                I::Link,
                tr(keys::Q_PEOPLE_REPLACE_PRIMARY_INVITE_LINK),
                |this, _window, cx| {
                    this.replace_primary_invite_link(chat_id, cx);
                }
            );
        }
        // Supergroup-only: join-request toggle, username, broadcast
        // upgrade. `setSupergroupUsername` is owner-only (driver
        // enforces); broadcast groups can't toggle join-by-request.
        if !is_channel && !is_basic_group {
            if can_restrict && !self.chat_is_broadcast(chat_id) {
                let enabled = self.chat_join_by_request(chat_id);
                toggle_row!(
                    "g1-toggle-join-request",
                    I::UserCheck,
                    tr(keys::GROUP_INVITE_REQUEST_APPROVE),
                    enabled,
                    |this, _window, cx| {
                        this.toggle_join_by_request(chat_id, cx);
                    }
                );
            }
            if is_owner {
                let username = self.chat_username(chat_id);
                let label = if username.is_empty() {
                    tr(keys::Q_PEOPLE_SET_PUBLIC_USERNAME).to_string()
                } else {
                    tr_with(
                        keys::Q_PEOPLE_PUBLIC_USERNAME,
                        &[("username", &username.to_string())],
                    )
                };
                row!("g1-open-username", I::AtSign, label, |this, window, cx| {
                    this.open_username_dialog(chat_id, window, cx);
                });
                if !self.chat_is_broadcast(chat_id) {
                    row!(
                        "g1-broadcast-upgrade",
                        I::Megaphone,
                        tr(keys::RIGHTS_GIGAGROUP_CONVERT),
                        |this, _window, cx| {
                            this.open_group_confirm(
                                chat_id,
                                GroupConfirmAction::BroadcastIntro,
                                cx,
                            );
                        }
                    );
                    section = section.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(quill::admin_extras::BROADCAST_ABOUT),
                    );
                }
            }
        }
        // Slice G2: channel signatures + show authors (Telegram X
        // ProfileController: two toggles, gated on `can_change_info`;
        // disabling signatures also hides authors, mirroring
        // `ToggleSupergroupSignMessages(id, sign, sign && show)`).
        if is_channel && session.is_some_and(|session| session.chat_can_change_info(chat_id)) {
            let (sign, show) = self.sign_flags(chat_id);
            toggle_row!(
                "g2-toggle-sign-messages",
                I::PenLine,
                tr(keys::Q_PEOPLE_SIGN_MESSAGES),
                sign,
                |this, _window, cx| {
                    let (sign, show) = this.sign_flags(chat_id);
                    this.set_sign_messages(chat_id, !sign, !sign && show, cx);
                }
            );
            toggle_row!(
                "g2-toggle-show-authors",
                I::Eye,
                tr(keys::Q_PEOPLE_SHOW_MESSAGE_AUTHORS),
                show && sign,
                |this, _window, cx| {
                    let (_, show) = this.sign_flags(chat_id);
                    // Enabling authors implies signatures (Telegram X forces
                    // `show = sign && show`).
                    this.set_sign_messages(chat_id, true, !show, cx);
                }
            );
        }
        // tdesktop `edit_peer_info_box` "Auto-translate messages": channels,
        // `toggleSupergroupHasAutomaticTranslation` (needs can_change_info
        // and boosts; TDLib's refusal rolls the switch back).
        if is_channel && session.is_some_and(|session| session.chat_can_change_info(chat_id)) {
            let auto = session.is_some_and(|session| session.chat_auto_translate(chat_id));
            toggle_row!(
                "tr-toggle-auto-translate",
                I::Languages,
                tr(keys::Q_PEOPLE_AUTO_TRANSLATE_MESSAGES),
                auto,
                |this, _window, cx| {
                    this.set_auto_translate(chat_id, cx);
                }
            );
        }
        // Slice G2: aggressive anti-spam toggle (supergroups only;
        // gated on `supergroupFullInfo.can_toggle_aggressive_anti_spam`).
        if !is_channel
            && !is_basic_group
            && session.is_some_and(|session| session.chat_can_toggle_anti_spam(chat_id))
        {
            let enabled = self
                .session()
                .and_then(|session| {
                    session
                        .chats
                        .get(&chat_id.0)
                        .and_then(|chat| match chat.kind {
                            ChatKind::Supergroup { supergroup_id, .. } => session
                                .groups
                                .supergroup_anti_spam_enabled
                                .get(&supergroup_id)
                                .copied(),
                            _ => None,
                        })
                })
                .unwrap_or(false);
            toggle_row!(
                "g2-toggle-anti-spam",
                I::ShieldCheck,
                tr(keys::Q_PEOPLE_AGGRESSIVE_ANTI_SPAM),
                enabled,
                |this, _window, cx| {
                    let enabled = this
                        .session()
                        .and_then(|session| {
                            session
                                .chats
                                .get(&chat_id.0)
                                .and_then(|chat| match chat.kind {
                                    ChatKind::Supergroup { supergroup_id, .. } => session
                                        .groups
                                        .supergroup_anti_spam_enabled
                                        .get(&supergroup_id)
                                        .copied(),
                                    _ => None,
                                })
                        })
                        .unwrap_or(false);
                    this.set_anti_spam(chat_id, !enabled, cx);
                }
            );
        }
        // Slice G2: forum-topic management (admins with
        // `can_manage_topics` in forum supergroups).
        if !is_channel
            && !is_basic_group
            && session.is_some_and(|session| {
                session
                    .chats
                    .get(&chat_id.0)
                    .is_some_and(|chat| chat.is_forum_chat())
                    && session.chat_can_manage_topics(chat_id)
            })
        {
            row!(
                "g2-open-forum-manage",
                I::MessagesSquare,
                tr(keys::RIGHTS_GROUP_TOPICS),
                |this, window, cx| {
                    this.open_forum_manage_dialog(chat_id, window, cx);
                }
            );
        }
        // Slice G2: welcome-message editor (admins with
        // `can_send_welcome_messages`; the check mark reflects
        // `Session::chat_has_welcome_messages`).
        if session.is_some_and(|session| session.chat_can_send_welcome_messages(chat_id)) {
            let has_welcome = session
                .as_ref()
                .and_then(|session| {
                    session
                        .groups
                        .chat_has_welcome_messages
                        .get(&chat_id.0)
                        .copied()
                })
                .unwrap_or(false);
            section = section.child(
                action_row(
                    "g2-open-welcome",
                    Some(I::Hand),
                    tr(keys::Q_PEOPLE_WELCOME_MESSAGE),
                    false,
                    cx,
                )
                .child(div().flex_1())
                .when(has_welcome, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PEOPLE_ON)),
                    )
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_welcome_dialog(chat_id, window, cx);
                })),
            );
        }
        // The owner hands the chat to someone else (password-confirmed).
        if self.group_flavor(chat_id).is_some() && session.is_some_and(|s| s.chat_is_owner(chat_id))
        {
            row!(
                "g1-transfer-ownership",
                I::Crown,
                if is_channel {
                    tr(keys::Q_PEOPLE_TRANSFER_CHANNEL_OWNERSHIP)
                } else {
                    tr(keys::RIGHTS_TRANSFER_GROUP)
                },
                |this, window, cx| {
                    this.open_transfer_ownership(chat_id, window, cx);
                }
            );
        }
        // `deleteChat` (schema 1.8.67, line 11850): TDLib deletes for
        // everyone only when `chat.can_be_deleted_for_all_users` —
        // creator of a group/channel. Last, in the danger color, as in
        // tdesktop's box.
        if can_delete {
            let label = if is_channel {
                tr(keys::Q_PEOPLE_DELETE_CHANNEL)
            } else {
                tr(keys::PROFILE_DELETE_GROUP)
            };
            danger_row!("g1-delete-chat", I::Trash, label, |this, _window, cx| {
                this.open_group_confirm(chat_id, GroupConfirmAction::DeleteChat, cx);
            });
        }
        section.into_any_element()
    }
}

/// One action of the manage section (`action_row` plus its handler).
fn management_row(
    id: &'static str,
    icon: gpui_kit::assets::IconName,
    label: impl Into<SharedString>,
    destructive: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    action_row(id, Some(icon), label, destructive, cx)
        .on_click(on_click)
        .into_any_element()
}

/// An on/off setting of the manage section: the row toggles it; the
/// trailing switch shows the state (it carries no handler of its own, so
/// the click reaches the row).
fn management_toggle_row(
    id: &'static str,
    icon: gpui_kit::assets::IconName,
    label: impl Into<SharedString>,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let label = label.into();
    action_row(id, Some(icon), label.clone(), false, cx)
        .aria_selected(on)
        .child(div().flex_1())
        .child(
            gpui_kit::component::switch::Switch::new((id, 1u64))
                .checked(on)
                .accessibility_label(label)
                .small(),
        )
        .on_click(on_click)
        .into_any_element()
}
