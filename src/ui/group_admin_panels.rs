//! admin panels: management, invites, admins, boosts, sign flags.

use super::app::QuillApp;
use super::pressable::action_row;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::{AdminListFetch, InviteLinkFetch, JoinRequestFetch, RequestPurpose};
use quill::telegram::envelope::{ChannelMemberStatus, ChatAdministratorEntry, ChatKind};
impl QuillApp {
    fn group_sticker_choices(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session().expect("group info session");
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
                    "Loading packs…"
                } else {
                    "Load / refresh group packs"
                })
                .ghost()
                .disabled(loading)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut()
                        && live.driver.load_group_sticker_choices(chat_id).is_err()
                    {
                        this.status_note = "Could not load group packs. Retry the action.".into();
                    }
                    cx.notify();
                })),
        );
        if session.stickers.failed || session.emoji.failed {
            section = section.child(
                div()
                    .text_sm()
                    .child("Could not load packs. Retry Load / refresh."),
            );
        }
        for (custom, title, current, sets) in [
            (
                false,
                "Group stickers",
                info.map_or(0, |i| i.sticker_set_id),
                &session.stickers.sets,
            ),
            (
                true,
                "Group emoji",
                info.map_or(0, |i| i.custom_emoji_sticker_set_id),
                &session.emoji.installed_sets,
            ),
        ] {
            let current_label = if current == 0 {
                "None".to_string()
            } else {
                sets.iter()
                    .find(|s| s.id == current)
                    .map(|s| s.title.clone())
                    .unwrap_or_else(|| format!("Pack {current}"))
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
            for (id, label) in std::iter::once((0, "Remove pack".to_string()))
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
                                this.status_note = match result {
                                    Ok(Some(_)) => "Group pack change requested.".into(),
                                    Ok(None) => {
                                        "Group pack change is unavailable or already pending."
                                            .into()
                                    }
                                    Err(_) => {
                                        "Could not change group pack. Retry the action.".into()
                                    }
                                };
                            }
                            cx.notify();
                        })),
                );
            }
            section = section.child(choices);
        }
        section.into_any_element()
    }

    /// Slice G2: channel boost status (`getChatBoostStatus`, schema
    /// 1.8.67, line 13917) with the one-tap `boostChat` action. Shown
    /// for channels only; honest states: loading (request in flight),
    /// failed-with-retry, loaded "Level N · M boosts". `getChatBoostStatus`
    /// errors when boosts are unavailable for the chat — the failure
    /// renders instead of a fake number.
    pub(super) fn boost_section(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
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
            return div().into_any_element();
        }
        let status = self
            .session()
            .and_then(|session| session.chat_boost_status.get(&chat_id.0).copied());
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
                .child("Channel boosts"),
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
                                .child(format!("Level {level} · {boost_count} boosts")),
                        )
                        .child(
                            Button::new("g2-boost-channel")
                                .label("Boost channel")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.boost_channel(chat_id, cx);
                                })),
                        ),
                );
            }
            None if in_flight => {
                section = section.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading boost status…"),
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
                                .child("Boost status unavailable."),
                        )
                        .child(
                            Button::new("g2-boost-retry")
                                .label("Retry")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.refresh_boost_status(chat_id, cx);
                                })),
                        ),
                );
            }
        }
        section.into_any_element()
    }

    /// Slice G2: re-request the boost status (bypasses the cache).
    pub(super) fn refresh_boost_status(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.chat_boost_status.remove(&chat_id.0);
            if live.driver.fetch_chat_boost_status(chat_id).is_err() {
                self.status_note = "could not load boost status".into();
            }
        }
        cx.notify();
    }

    /// Slice G1: group/channel management section for the info panel.
    /// Every row is capability-gated (`chat_can_restrict_members`,
    /// `chat_is_owner`, `chat_can_invite_users`,
    /// `can_be_deleted_for_all_users`) so buttons only appear when the
    /// action can succeed. Basic groups get members / permissions /
    /// invite link / leave / delete; supergroups additionally get
    /// join-request toggle, username, and the one-way broadcast
    /// upgrade; channels get members (subscribers), username, invite
    /// link, leave / delete.
    /// Slice G2: `(sign_messages, show_message_sender)` for the
    /// channel's supergroup (`updateSupergroup` flags; unset = off).
    pub(super) fn sign_flags(&self, chat_id: ChatId) -> (bool, bool) {
        self.session()
            .and_then(|session| {
                session
                    .chats
                    .get(&chat_id.0)
                    .and_then(|chat| match chat.kind {
                        ChatKind::Supergroup { supergroup_id, .. } => Some((
                            session
                                .supergroup_sign_messages
                                .get(&supergroup_id)
                                .copied()
                                .unwrap_or(false),
                            session
                                .supergroup_show_message_sender
                                .get(&supergroup_id)
                                .copied()
                                .unwrap_or(false),
                        )),
                        _ => None,
                    })
            })
            .unwrap_or((false, false))
    }

    pub(super) fn group_management_section(
        &self,
        chat_id: ChatId,
        is_channel: bool,
        is_basic_group: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
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
        let mut section = div().flex().flex_col().w_full().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(if is_channel {
                    "Manage channel"
                } else {
                    "Manage group"
                }),
        );
        use gpui_kit::assets::IconName as I;
        macro_rules! row {
            ($id:expr, $icon:expr, $label:expr, |$this:ident, $window:ident, $cx:ident| $action:block) => {
                section = section.child(
                    action_row($id, Some($icon), $label, false, cx)
                        .on_click(cx.listener(move |$this, _, $window, $cx| $action)),
                );
            };
        }
        macro_rules! danger_row {
            ($id:expr, $icon:expr, $label:expr, |$this:ident, $window:ident, $cx:ident| $action:block) => {
                section = section.child(
                    action_row($id, Some($icon), $label, true, cx)
                        .on_click(cx.listener(move |$this, _, $window, $cx| $action)),
                );
            };
        }
        // An on/off setting: the row toggles it; the trailing switch shows
        // the state (it carries no handler of its own, so the click reaches
        // the row).
        macro_rules! toggle_row {
            ($id:expr, $icon:expr, $label:expr, $on:expr, |$this:ident, $window:ident, $cx:ident| $action:block) => {
                section = section.child(
                    action_row($id, Some($icon), $label, false, cx)
                        .aria_selected($on)
                        .child(div().flex_1())
                        .child(
                            gpui_kit::component::switch::Switch::new(($id, 1u64))
                                .checked($on)
                                .small(),
                        )
                        .on_click(cx.listener(move |$this, _, $window, $cx| $action)),
                );
            };
        }
        // Slice G8: title / description / photo editing — same gate as
        // the driver's `group_info_edit_allowed`: basic groups are
        // democratic (every member may edit), supergroups and channels
        // need `can_change_info`. These are the primary info actions,
        // so they lead the section.
        let can_edit_info = session
            .as_ref()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| match chat.kind {
                ChatKind::BasicGroup { .. } => true,
                ChatKind::Supergroup { .. } => {
                    session.is_some_and(|s| s.chat_can_change_info(chat_id))
                        || chat.permissions.as_ref().is_some_and(|p| p.can_change_info)
                }
                _ => false,
            });
        if can_edit_info {
            row!(
                "g8-edit-title",
                I::Pencil,
                "Edit title",
                |this, window, cx| {
                    this.open_group_title_dialog(chat_id, window, cx);
                }
            );
            row!(
                "g8-edit-description",
                I::FileText,
                "Edit description",
                |this, window, cx| {
                    this.open_group_description_dialog(chat_id, window, cx);
                }
            );
            row!(
                "g8-edit-photo",
                I::Camera,
                "Change photo",
                |this, window, cx| {
                    this.open_group_photo_dialog(chat_id, window, cx);
                }
            );
        }
        // B7: one entry for the settings the viewer's rights allow
        // (topics, history, reactions, discussion group, ...).
        if session.is_some_and(|s| !s.group_admin_controls(chat_id).is_empty()) {
            row!(
                "b7-open-settings",
                I::Settings,
                if is_channel {
                    "Channel settings"
                } else {
                    "Group settings"
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
            let label = if is_channel { "Subscribers" } else { "Members" };
            row!("g1-open-members", I::Users, label, |this, window, cx| {
                this.open_member_dialog(chat_id, window, cx);
            });
        }
        if can_restrict {
            row!(
                "g1-open-permissions",
                I::Shield,
                "Default permissions",
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
                "Replace primary invite link",
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
                    "Approve new members",
                    enabled,
                    |this, _window, cx| {
                        this.toggle_join_by_request(chat_id, cx);
                    }
                );
            }
            if is_owner {
                let username = self.chat_username(chat_id);
                let label = if username.is_empty() {
                    "Set public username".to_string()
                } else {
                    format!("Public username (@{username})")
                };
                row!("g1-open-username", I::AtSign, label, |this, window, cx| {
                    this.open_username_dialog(chat_id, window, cx);
                });
                if !self.chat_is_broadcast(chat_id) {
                    row!(
                        "g1-broadcast-upgrade",
                        I::Megaphone,
                        "Convert to broadcast group",
                        |this, _window, cx| {
                            this.open_group_confirm(
                                chat_id,
                                GroupConfirmAction::BroadcastUpgrade,
                                cx,
                            );
                        }
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
                "Sign messages",
                sign,
                |this, _window, cx| {
                    let (sign, show) = this.sign_flags(chat_id);
                    this.set_sign_messages(chat_id, !sign, !sign && show, cx);
                }
            );
            toggle_row!(
                "g2-toggle-show-authors",
                I::Eye,
                "Show message authors",
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
                "Auto-translate messages",
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
                "Aggressive anti-spam",
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
                "Manage topics",
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
                .and_then(|session| session.chat_has_welcome_messages.get(&chat_id.0).copied())
                .unwrap_or(false);
            section = section.child(
                action_row(
                    "g2-open-welcome",
                    Some(I::Hand),
                    "Welcome message",
                    false,
                    cx,
                )
                .child(div().flex_1())
                .when(has_welcome, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("On"),
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
                    "Transfer channel ownership"
                } else {
                    "Transfer group ownership"
                },
                |this, window, cx| {
                    this.open_transfer_ownership(chat_id, window, cx);
                }
            );
        }
        if is_member {
            let label = if is_channel {
                "Leave channel"
            } else {
                "Leave group"
            };
            danger_row!("g1-leave-chat", I::LogOut, label, |this, window, cx| {
                // An owner sees who inherits first (tdesktop
                // `select_future_owner_box`).
                if !this.open_owner_leave(chat_id, window, cx) {
                    this.open_group_confirm(chat_id, GroupConfirmAction::LeaveChat, cx);
                }
            });
        }
        // `deleteChat` (schema 1.8.67, line 11850): TDLib deletes for
        // everyone only when `chat.can_be_deleted_for_all_users` —
        // creator of a group/channel, or any private chat.
        if can_delete {
            let label = if is_channel {
                "Delete channel"
            } else {
                "Delete group"
            };
            danger_row!("g1-delete-chat", I::Trash, label, |this, _window, cx| {
                this.open_group_confirm(chat_id, GroupConfirmAction::DeleteChat, cx);
            });
        }
        section.into_any_element()
    }

    /// Phase D3a: human expiry for an invite link (`expiration_date` is a
    /// unix timestamp; 0 = never expires).
    pub(super) fn invite_link_expiry(expiration_date: i32) -> String {
        if expiration_date == 0 {
            return "Never expires".into();
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        let remaining = i64::from(expiration_date) - now;
        if remaining <= 0 {
            "Expired".into()
        } else if remaining < 86_400 {
            "Expires today".into()
        } else if remaining < 172_800 {
            "Expires tomorrow".into()
        } else {
            format!("Expires in {} days", remaining / 86_400)
        }
    }

    /// Phase D3a: invite-link management section for the channel/group
    /// info panel. Shown only to admins who may manage links
    /// (`ChatSummary::can_invite_users`). Honest states: loading /
    /// failed-with-retry / loaded list. Each row shows the link name (or
    /// "Primary link" / "Invite link" fallback), uses, expiry, a
    /// join-request badge when `pending_join_request_count` > 0, and
    /// Copy + Revoke buttons. Revoking is the delete path (TDLib 1.8.67
    /// has no `deleteChatInviteLink`).
    pub(super) fn invite_links_section(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_manage = self
            .session()
            .is_some_and(|session| session.chat_can_invite_users(chat_id));
        if !can_manage {
            return div().into_any_element();
        }
        let fetch = self
            .session()
            .and_then(|session| session.invite_links.get(&chat_id.0))
            .cloned();
        let mut section = div().flex().flex_col().w_full().gap_1().child(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("Invite links"),
                )
                .child(div().flex_1())
                .child(
                    Button::new("invite-links-refresh")
                        .icon(gpui_kit::assets::IconName::RotateCcw)
                        .ghost()
                        .xsmall()
                        .tooltip("Refresh")
                        .accessibility_label("Refresh")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.refresh_invite_links(chat_id, cx);
                        })),
                )
                .child(
                    Button::new("invite-link-create")
                        .icon(gpui_kit::assets::IconName::Plus)
                        .xsmall()
                        .tooltip("Create invite link")
                        .accessibility_label("Create invite link")
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_invite_link_dialog(chat_id, window, cx);
                        })),
                ),
        );
        match fetch {
            None | Some(InviteLinkFetch::Loading) => {
                section = section.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading invite links…"),
                );
            }
            Some(InviteLinkFetch::Failed(message)) => {
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
                                .child(message),
                        )
                        .child(
                            Button::new("invite-links-retry")
                                .label("Retry")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.refresh_invite_links(chat_id, cx);
                                })),
                        ),
                );
            }
            Some(InviteLinkFetch::Loaded(list)) => {
                if list.links.is_empty() {
                    section = section.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No invite links."),
                    );
                } else {
                    for (index, link) in list.links.into_iter().enumerate() {
                        let name = if !link.name.is_empty() {
                            link.name.clone()
                        } else if link.is_primary {
                            "Primary link".into()
                        } else {
                            "Invite link".into()
                        };
                        let uses = if link.member_limit > 0 {
                            format!("{}/{} uses", link.member_count, link.member_limit)
                        } else {
                            format!("{} uses", link.member_count)
                        };
                        let expiry = Self::invite_link_expiry(link.expiration_date);
                        let copy_link = link.invite_link.clone();
                        let revoke_link = link.invite_link.clone();
                        let mut row = div()
                            .id(("invite-link-row", index as u64))
                            .flex()
                            .flex_col()
                            .w_full()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .w_full()
                                    .gap_1()
                                    .child(div().flex_1().text_sm().child(name))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("{uses} · {expiry}")),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(link.invite_link),
                            );
                        if link.pending_join_request_count > 0 {
                            row = row.child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} pending requests",
                                        link.pending_join_request_count
                                    )),
                            );
                        }
                        row = row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new(format!("invite-link-copy-{index}"))
                                        .label("Copy")
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.copy_invite_link(&copy_link, cx);
                                        })),
                                )
                                .child(
                                    Button::new(format!("invite-link-revoke-{index}"))
                                        .label("Revoke")
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.revoke_invite_link(chat_id, &revoke_link, cx);
                                        })),
                                ),
                        );
                        section = section.child(row);
                    }
                }
            }
        }
        section.into_any_element()
    }

    /// Phase D3a: join-request section. Approve/Decline per request;
    /// shows the requester's name (falls back to "User <id>") and bio.
    pub(super) fn join_requests_section(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_manage = self
            .session()
            .is_some_and(|session| session.chat_can_invite_users(chat_id));
        if !can_manage {
            return div().into_any_element();
        }
        let fetch = self
            .session()
            .and_then(|session| session.join_requests.get(&chat_id.0))
            .cloned();
        let pending_count = match &fetch {
            Some(JoinRequestFetch::Loaded(list)) => list.total_count,
            _ => self
                .session()
                .and_then(|session| session.pending_join_request_counts.get(&chat_id.0).copied())
                .unwrap_or(0),
        };
        let mut header = div().flex().items_center().w_full().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child("Join requests"),
        );
        if pending_count > 0 {
            header = header.child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(pending_count.to_string()),
            );
        }
        header = header.child(div().flex_1()).child(
            Button::new("join-requests-refresh")
                .icon(gpui_kit::assets::IconName::RotateCcw)
                .ghost()
                .xsmall()
                .tooltip("Refresh")
                .accessibility_label("Refresh")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.refresh_join_requests(chat_id, cx);
                })),
        );
        let mut section = div().flex().flex_col().w_full().gap_1().child(header);
        match fetch {
            None | Some(JoinRequestFetch::Loading) => {
                section = section.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading join requests…"),
                );
            }
            Some(JoinRequestFetch::Failed(message)) => {
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
                                .child(message),
                        )
                        .child(
                            Button::new("join-requests-retry")
                                .label("Retry")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.refresh_join_requests(chat_id, cx);
                                })),
                        ),
                );
            }
            Some(JoinRequestFetch::Loaded(list)) => {
                if list.requests.is_empty() {
                    section = section.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No pending join requests."),
                    );
                } else {
                    for request in list.requests {
                        let user_id = request.user_id;
                        let name = self
                            .session()
                            .and_then(|session| session.user(user_id))
                            .map(|user| {
                                format!("{} {}", user.first_name, user.last_name)
                                    .trim()
                                    .to_owned()
                            })
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| format!("User {user_id}"));
                        let mut details = div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap_1()
                            .child(div().text_sm().child(name));
                        if !request.bio.is_empty() {
                            details = details.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(request.bio),
                            );
                        }
                        section = section.child(
                            div()
                                .id(("join-request-row", user_id as u64))
                                .flex()
                                .items_center()
                                .w_full()
                                .gap_1()
                                .child(details)
                                .child(
                                    Button::new(format!("join-request-approve-{user_id}"))
                                        .label("Approve")
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.process_join_request(chat_id, user_id, true, cx);
                                        })),
                                )
                                .child(
                                    Button::new(format!("join-request-decline-{user_id}"))
                                        .label("Decline")
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.process_join_request(chat_id, user_id, false, cx);
                                        })),
                                ),
                        );
                    }
                }
            }
        }
        section.into_any_element()
    }

    /// Phase D3b: administrator management section for the channel /
    /// supergroup info panel. Shown only when the viewer may manage
    /// admins (`Session::chat_can_manage_admins`: owner, or admin with
    /// `can_promote_members` — deny-by-default otherwise). Honest
    /// states: loading / failed-with-retry / loaded list. Each row
    /// shows the admin's name, custom title (a crown badge for the
    /// owner), and Edit + Remove buttons when the entry is editable
    /// (`can_be_edited` and not the owner — TDLib rejects edits to the
    /// creator and to admins it does not allow editing).
    pub(super) fn administrators_section(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self
            .session()
            .is_some_and(|session| session.chat_can_manage_admins(chat_id))
        {
            return div().into_any_element();
        }
        let fetch = self
            .session()
            .and_then(|session| session.admin_lists.get(&chat_id.0))
            .cloned();
        let mut section = div().flex().flex_col().w_full().gap_1().child(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("Administrators"),
                )
                .child(div().flex_1())
                .child(
                    Button::new("admin-list-refresh")
                        .icon(gpui_kit::assets::IconName::RotateCcw)
                        .ghost()
                        .xsmall()
                        .tooltip("Refresh")
                        .accessibility_label("Refresh")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.refresh_administrators(chat_id, cx);
                        })),
                )
                .child(
                    Button::new("admin-promote-open")
                        .icon(gpui_kit::assets::IconName::Plus)
                        .xsmall()
                        .tooltip("Add administrator")
                        .accessibility_label("Add administrator")
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_promote_picker(chat_id, window, cx);
                        })),
                ),
        );
        match fetch {
            None | Some(AdminListFetch::Loading) => {
                section = section.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading administrators…"),
                );
            }
            Some(AdminListFetch::Failed(message)) => {
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
                                .child(message),
                        )
                        .child(
                            Button::new("admin-list-retry")
                                .label("Retry")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.refresh_administrators(chat_id, cx);
                                })),
                        ),
                );
            }
            Some(AdminListFetch::Loaded(list)) => {
                if list.is_empty() {
                    section = section.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No administrators."),
                    );
                } else {
                    for entry in &list {
                        section = section.child(self.admin_row(chat_id, entry, cx));
                    }
                }
            }
        }
        section.into_any_element()
    }

    /// Phase D3b: one row of the administrators list — name, custom
    /// title or role badge, Edit + Remove buttons for editable admins.
    pub(super) fn admin_row(
        &self,
        chat_id: ChatId,
        entry: &ChatAdministratorEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .session()
            .and_then(|session| session.user(entry.user_id))
            .map(|user| user.display_name())
            .unwrap_or_else(|| format!("User {}", entry.user_id));
        let role = if entry.is_owner {
            "👑 Owner".to_string()
        } else if entry.custom_title.is_empty() {
            "Admin".to_string()
        } else {
            entry.custom_title.clone()
        };
        let mut row = div()
            .id(("admin-row", entry.user_id as u64))
            .flex()
            .items_center()
            .w_full()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_1()
                    .child(div().text_sm().child(name))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(role),
                    ),
            );
        // The owner cannot be edited or demoted; non-editable admins
        // (granted by someone else) are server-rejected too.
        if entry.can_be_edited && !entry.is_owner {
            let user_id = entry.user_id;
            row = row
                .child(
                    Button::new(format!("admin-edit-{user_id}"))
                        .label("Edit")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_rights_editor(chat_id, user_id, cx);
                        })),
                )
                .child(
                    Button::new(format!("admin-demote-{user_id}"))
                        .label("Remove")
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_demote_confirm(chat_id, user_id, cx);
                        })),
                );
        }
        row.into_any_element()
    }
}
