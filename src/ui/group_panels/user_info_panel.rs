//! Methods moved out of `group_panels.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// User profile panel: photo (downloaded `userFullInfo.photo` size, or
    /// an initials avatar), name, status, username/phone rows, bio, and an
    /// Add contact affordance for known non-contacts.
    pub(in crate::ui) fn user_info_panel(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let user = session.and_then(|s| s.user(user_id)).cloned();
        let info = session.and_then(|s| s.user_full_info(user_id)).cloned();
        let name = user
            .as_ref()
            .map(|u| u.display_name())
            .unwrap_or_else(|| format!("User {user_id}"));
        let status = user
            .as_ref()
            .map(|u| {
                if u.is_bot {
                    "bot".to_string()
                } else {
                    u.status.display()
                }
            })
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let photo_path: Option<PathBuf> = session
            .and_then(|s| {
                info.as_ref()
                    .and_then(|i| i.photo_file_id)
                    .and_then(|id| s.media.files.get(&id))
                    .and_then(|file| file.usable_path())
                    .or_else(|| s.user_photo_path(user_id))
            })
            .and_then(|path| sandboxed_display_path(path, &roots));
        let avatar: AnyElement = match photo_path {
            Some(path) => img(path)
                .id(("info-panel-photo", user_id as u64))
                .w(px(96.))
                .h(px(96.))
                .aspect_ratio(px(96.) / px(96.))
                .rounded_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => initials_avatar(&name, 96.).into_any_element(),
        };
        let is_self = session
            .and_then(|s| s.my_user_id)
            .is_some_and(|me| me == user_id);
        // B10: a profile with a photo opens the photo gallery in the media
        // viewer.
        let has_photo = info.as_ref().is_some_and(|i| i.photo_id.is_some())
            || user.as_ref().is_some_and(|u| u.photo_small_file_id != 0);
        let avatar: AnyElement = if has_photo {
            div()
                .id(("info-panel-photo-open", user_id as u64))
                .cursor_pointer()
                .role(gpui_kit::Role::Button)
                .aria_label("View profile photos")
                .tab_index(0)
                .on_click(cx.listener(move |this, _, _, cx| this.open_profile_photos(user_id, cx)))
                .child(avatar)
                .into_any_element()
        } else {
            avatar
        };
        let name_for_copy = name.clone();
        let mut body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .p_4()
            .child(avatar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_0p5()
                    .child(
                        div()
                            .id(("info-panel-name", user_id as u64))
                            .text_lg()
                            .font_semibold()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy_profile_text(
                                    &name_for_copy,
                                    "Name copied to clipboard",
                                    cx,
                                );
                            }))
                            .max_w_full()
                            .truncate()
                            .child(super::bidi_line::one_line_plain(name.clone())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    ),
            );
        // Primary actions as a row of labeled icon tiles
        // (`profile_panels::action_row`).
        let in_own_chat = session
            .and_then(|s| s.open_chat.map(|chat| (s, chat)))
            .is_some_and(|(s, chat)| s.private_chat_user_id(chat) == Some(user_id));
        if let Some(tiles) = self.user_profile_action_row(user_id, cx) {
            body = body.child(tiles);
        }
        if let Some(warning) = self.unofficial_client_warning(user_id, cx) {
            body = body.child(warning);
        }
        // Details: value over label, left-aligned like a contact card.
        // Rows copy on tap and in the right-click menu (B10).
        if let Some(card) = self.profile_details_card(user_id, cx) {
            body = body.child(card);
        }
        if let Some(actions) = self.profile_contact_actions(user_id, cx) {
            body = body.child(actions);
        }
        if let Some(actions) = self.bot_profile_actions(user_id, cx) {
            body = body.child(actions);
        }
        // tdesktop's "Change colors" (`addThemeEdit`): the chat's theme and
        // wallpaper, for the private chat open right now.
        if !is_self
            && in_own_chat
            && let Some(chat) = session.and_then(|s| s.open_chat)
        {
            body = body.child(
                action_row(
                    "info-panel-chat-look",
                    Some(gpui_kit::assets::IconName::Palette),
                    "Change colors and wallpaper",
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_chat_look_dialog(chat.0, cx);
                })),
            );
        }
        let groups_in_common = info.as_ref().map_or(0, |i| i.extras.groups_in_common);
        // B10: the group rows replace the bare count once the list is
        // loaded; until then the count row stays.
        let groups_loaded = session.is_some_and(|s| {
            !s.profile_chat_list(ProfileChatsKind::GroupsInCommon, user_id)
                .is_empty()
        });
        if let Some(section) =
            self.media_counts_section(if groups_loaded { 0 } else { groups_in_common }, cx)
        {
            body = body.child(section);
        }
        if let Some(section) = self.groups_in_common_section(user_id, cx) {
            body = body.child(section);
        }
        // Slice A6: contact management for any other user — Delete
        // contact (`removeContacts`) and Block/Unblock
        // (`setMessageSenderBlockList`; state from `userFullInfo.block_list`).
        // Destructive, so they sit last as plain danger rows.
        if !is_self && user.is_some() {
            let blocked = info.as_ref().is_some_and(|i| i.blocked);
            let mut danger = div().flex().flex_col().w_full().gap_0p5();
            danger = danger.child(
                action_row(
                    "info-panel-block-user",
                    Some(gpui_kit::assets::IconName::Ban),
                    if blocked {
                        "Unblock user"
                    } else {
                        "Block user"
                    },
                    !blocked,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_group_confirm(
                        ChatId(0),
                        GroupConfirmAction::BlockContact {
                            user_id,
                            block: !blocked,
                        },
                        cx,
                    );
                })),
            );
            if user.as_ref().is_some_and(|u| u.is_contact) {
                danger = danger.child(
                    action_row(
                        "info-panel-delete-contact",
                        Some(gpui_kit::assets::IconName::UserX),
                        "Delete contact",
                        true,
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_group_confirm(
                            ChatId(0),
                            GroupConfirmAction::DeleteContact { user_id },
                            cx,
                        );
                    })),
                );
            }
            body = body.child(danger);
        }
        // Phase B2: encryption-key section — only when the open chat is a
        // Ready secret chat with this user (the key is meaningful once
        // the session is established). Missing/short hashes render the
        // still-loading note inside the section.
        if let Some(record) = session
            .as_ref()
            .and_then(|s| s.open_ready_secret_chat_for_user(user_id))
        {
            body = body.child(self.encryption_key_section(record, cx));
            body = body.child(self.secret_passcode_hint(user_id, cx));
        }
        body.into_any_element()
    }

    /// Phase S1: per-secret-chat passcode hint (TGX `SecretPasscodeInfo`,
    /// paraphrased, with the peer's name). The hint only — setting an
    /// additional per-chat passcode needs a full app passcode feature,
    /// which is out of this slice.
    pub(in crate::ui) fn secret_passcode_hint(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = Self::secret_peer_name(self.session(), user_id, "this chat");
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap_2()
            .pt_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Passcode"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "When you set up an additional passcode, you'll need to enter it each \
                         time you access {name}. Message preview will be hidden on the chats \
                         page.\n\nNote: if you forget it, contents of this chat will be \
                         lost.\n\nIf you need a global passcode, use Settings > Privacy and \
                         Security > Passcode Lock."
                    )),
            )
            .into_any_element()
    }

    /// Supergroup/channel panel: title, member count, description.
    pub(in crate::ui) fn supergroup_info_panel(
        &self,
        supergroup_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let (title, chat_id, is_channel) = session
            .and_then(|s| {
                s.chats.values().find_map(|chat| match chat.kind {
                    ChatKind::Supergroup {
                        supergroup_id: id,
                        is_channel,
                    } if id == supergroup_id => Some((chat.title.clone(), chat.id, is_channel)),
                    _ => None,
                })
            })
            .unwrap_or_else(|| {
                (
                    format!("Group {supergroup_id}"),
                    ChatId(supergroup_id),
                    false,
                )
            });
        let info = session
            .and_then(|s| s.groups.supergroup_full_infos.get(&supergroup_id))
            .cloned();
        let description = info
            .as_ref()
            .map(|i| i.description.clone())
            .unwrap_or_default();
        let members = info.as_ref().map(|i| i.member_count).unwrap_or(0);
        let username = session
            .and_then(|s| s.supergroup_username(supergroup_id))
            .filter(|name| !name.is_empty())
            .map(|name| name.to_string());
        // Parity slice: the panel avatar reuses the chat-list avatar (photo
        // or colored initials).
        let roots = self.media_display_roots();
        let photo = session
            .and_then(|s| s.chat_photo_path(chat_id))
            .and_then(|path| sandboxed_display_path(path, &roots));
        let noun = if is_channel { "subscribers" } else { "members" };
        let mut body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .p_4()
            .child(chat_avatar(&title, photo.as_deref(), 96.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .max_w_full()
                            .truncate()
                            .child(super::bidi_line::one_line_plain(title.clone())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .when(members > 0, |this| {
                                this.child(format!("{} {noun}", compact_count(members)))
                            }),
                    )
                    .when_some(username, |this, name| {
                        // tdesktop's username row: click copies, the
                        // right-click menu offers Copy Username and Copy Link.
                        let owner = cx.entity().downgrade();
                        let mention = format!("@{name}");
                        let actions = [
                            CopyAction {
                                text: mention.clone(),
                                menu: "Copy Username",
                                toast: "Username copied to clipboard",
                            },
                            CopyAction {
                                text: quill::profile_forms::profile_link(&name),
                                menu: "Copy Link",
                                toast: "Link copied to clipboard",
                            },
                        ];
                        let tapped = actions[0].clone();
                        this.child(
                            div()
                                .id("group-username")
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .cursor_pointer()
                                .child(mention)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.copy_profile_text(&tapped.text, tapped.toast, cx);
                                }))
                                .context_menu(move |menu, _, _| copy_menu(menu, &owner, &actions)),
                        )
                    }),
            );
        if !description.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Description"),
                    )
                    .child(div().text_sm().child(description)),
            );
        }
        // Phase A1: slow-mode admin control (`setChatSlowModeDelay`,
        // schema 1.8.67 line 13551 — allowed values 0/5/10/30/60/300/900/
        // 3600, supergroups only, requires `can_restrict_members`).
        // Creators hold all rights implicitly; administrators need the
        // explicit `can_restrict_members` right (lines 2500/1092). The new
        // delay arrives via `updateSupergroupFullInfo`.
        if !is_channel && self.can_change_slow_mode(supergroup_id) {
            let current_delay = info.as_ref().map(|i| i.slow_mode_delay).unwrap_or(0);
            let mut value_row = div().id("slow-mode-values").flex().flex_wrap().gap_1();
            for (label, value) in [
                ("Off", 0),
                ("5s", 5),
                ("10s", 10),
                ("30s", 30),
                ("1m", 60),
                ("5m", 300),
                ("15m", 900),
                ("1h", 3600),
            ] {
                let label = if value == current_delay {
                    format!("✓ {label}")
                } else {
                    label.to_string()
                };
                value_row = value_row.child(
                    Button::new(format!("slow-mode-set-{value}"))
                        .label(label)
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_slow_mode_delay(chat_id, value, cx);
                        })),
                );
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Slow mode"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Delay between messages for members"),
                    )
                    .child(value_row),
            );
        }
        // Phase D2: statistics entry point — shown only when
        // `supergroupFullInfo.can_get_statistics` is true (schema 1.8.67,
        // line 2792); `getChatStatistics` errors when it is false.
        if info.as_ref().is_some_and(|i| i.can_get_statistics) {
            let stats_chat_id = chat_id;
            body = body.child(
                Button::new("open-channel-statistics")
                    .label("Statistics")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_info_panel_target(
                            InfoPanelTarget::Statistics(stats_chat_id.0),
                            window,
                            cx,
                        );
                    })),
            );
        }
        if let Some(section) = self.media_counts_section(0, cx) {
            body = body.child(section);
        }
        // B10: channels similar to this one (`getChatSimilarChats`).
        if is_channel && let Some(section) = self.similar_channels_section(chat_id, cx) {
            body = body.child(section);
        }
        // Phase D3a: invite-link + join-request management (admins with
        // `can_invite_users` only; the sections no-op otherwise).
        body = body.child(self.invite_links_section(chat_id, cx));
        body = body.child(self.join_requests_section(chat_id, cx));
        // Phase D3b: administrator management (owner / admins with
        // `can_promote_members` only; the section no-ops otherwise).
        body = body.child(self.administrators_section(chat_id, cx));
        // Slice G1: group/channel management — members, permissions,
        // invite link, join requests, username, broadcast upgrade,
        // leave/delete. Every row is capability-gated inside.
        body = body.child(self.group_management_section(chat_id, is_channel, false, cx));
        // Phase D3c: recent-actions admin log (administrators and the
        // creator only; the section no-ops otherwise).
        body = body.child(self.event_log_section(chat_id, cx));
        // Slice G2: channel boost status + boost action.
        body = body.child(self.boost_section(chat_id, cx));
        body.into_any_element()
    }

    /// Slice G1: basic-group info panel. Basic groups have no
    /// `supergroupFullInfo` — the panel shows the title, the member
    /// list entry point (`getBasicGroupFullInfo`), and the management
    /// section.
    pub(in crate::ui) fn basic_group_info_panel(
        &self,
        basic_group_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (title, chat_id) = self
            .session()
            .and_then(|s| {
                s.chats.values().find_map(|chat| match chat.kind {
                    ChatKind::BasicGroup { basic_group_id: id } if id == basic_group_id => {
                        Some((chat.title.clone(), chat.id))
                    }
                    _ => None,
                })
            })
            .unwrap_or_else(|| (format!("Group {basic_group_id}"), ChatId(basic_group_id)));
        let member_count = self
            .session()
            .and_then(|s| s.groups.basic_group_members.get(&chat_id.0))
            .and_then(|fetch| match fetch {
                SupergroupMembersFetch::Loaded { total_count, .. } => Some(*total_count),
                _ => None,
            });
        let roots = self.media_display_roots();
        let photo = self
            .session()
            .and_then(|s| s.chat_photo_path(chat_id))
            .and_then(|path| sandboxed_display_path(path, &roots));
        let mut body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .p_4()
            .child(chat_avatar(&title, photo.as_deref(), 96.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .max_w_full()
                            .truncate()
                            .child(super::bidi_line::one_line_plain(title.clone())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .when_some(member_count, |this, count| {
                                this.child(format!("{count} members"))
                            }),
                    ),
            );
        if let Some(section) = self.media_counts_section(0, cx) {
            body = body.child(section);
        }
        body = body.child(self.group_management_section(chat_id, false, true, cx));
        body.into_any_element()
    }
}
