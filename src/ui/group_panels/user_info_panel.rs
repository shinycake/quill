//! Methods moved out of `group_panels.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::profile_panels::action_row::ProfilePeer;
use gpui_kit::TestSupportExt;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// A group or channel's info-panel avatar; with a photo it opens the
    /// photo viewer on it, the earlier photos after it.
    fn chat_photo_avatar(
        &self,
        chat_id: ChatId,
        title: &str,
        photo: Option<&std::path::Path>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let avatar = chat_avatar(title, photo, 96.);
        let has_photo = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.photo_file_id.is_some());
        if !has_photo {
            return avatar.into_any_element();
        }
        div()
            .id(("info-panel-chat-photo-open", chat_id.0 as u64))
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(tr(keys::Q_PEOPLE_VIEW_PHOTOS))
            .tab_index(0)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_peer_photos(quill::state::PhotoOwner::Chat(chat_id), cx)
            }))
            .child(avatar)
            .test_support()
            .into_any_element()
    }

    /// The elements of a profile header's badges (`Info::Profile::Badge`);
    /// an emoji status plays through `status_glyph` for `owner`.
    fn info_header_badges(
        &self,
        badges: Vec<quill::peer_badge::TitleBadge>,
        owner: i64,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        badges
            .into_iter()
            .map(|badge| {
                let glyph = match badge {
                    quill::peer_badge::TitleBadge::EmojiStatus(id) => self.status_glyph(
                        crate::ui::status_emoji::StatusSurface::Profile,
                        owner,
                        id,
                        cx,
                    ),
                    _ => None,
                };
                crate::ui::chat_row::title_badge_element(badge, glyph, cx)
            })
            .collect()
    }

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
            .unwrap_or_else(|| tr_with(keys::Q_PEOPLE_USER, &[("user_id", &user_id.to_string())]));
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
                .aria_label(tr(keys::Q_PEOPLE_VIEW_PROFILE_PHOTOS))
                .tab_index(0)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_peer_photos(quill::state::PhotoOwner::User(user_id), cx)
                }))
                .child(avatar)
                .into_any_element()
        } else {
            avatar
        };
        let name_for_copy = name.clone();
        let verify_icon = self.user_bot_verify_icon_path(user_id);
        // Verified check, Premium star or animated status after the name
        // (`Info::Profile::Badge`).
        let badges = user
            .as_ref()
            .map(|u| {
                quill::peer_badge::header_badges(
                    u.verification,
                    u.is_premium && session.is_some_and(|s| s.premium_badges_shown()),
                    u.emoji_status_id,
                )
            })
            .unwrap_or_default();
        let badges = self.info_header_badges(badges, user_id, cx);
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
                            .flex()
                            .items_center()
                            .gap_1()
                            .max_w_full()
                            .when_some(verify_icon, |this, path| {
                                this.child(crate::ui::bot_verify_icon::bot_verify_icon(path))
                            })
                            .child(
                                div()
                                    .id(("info-panel-name", user_id as u64))
                                    .text_lg()
                                    .font_semibold()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.copy_profile_text(
                                            &name_for_copy,
                                            tr(keys::Q_PEOPLE_NAME_COPIED_TO_CLIPBOARD),
                                            cx,
                                        );
                                    }))
                                    .max_w_full()
                                    .truncate()
                                    .child(super::bidi_line::one_line_plain(name.clone())),
                            )
                            .children(badges),
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
        if let Some(tiles) = self.profile_action_row(ProfilePeer::User(user_id), cx) {
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
        // The verifying bot's description (tdesktop `addBotVerify`).
        if let Some(verification) = session.and_then(|s| s.user_bot_verification(user_id))
            && let Some(text) = quill::bot_verification::profile_description(
                Some(verification),
                user.as_ref().is_some_and(|u| u.has_main_web_app),
            )
        {
            body =
                body.child(self.bot_verification_row(text, verification.icon_custom_emoji_id, cx));
        }
        if let Some(actions) = self.profile_contact_actions(user_id, cx) {
            body = body.child(actions);
        }
        if let Some(actions) = self.bot_profile_actions(user_id, cx) {
            body = body.child(actions);
        }
        if let Some(rows) = self.bot_command_rows(user_id, cx) {
            body = body.child(rows);
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
                    tr(keys::Q_PEOPLE_CHANGE_COLORS_AND_WALLPAPER),
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
        if let Some(button) = self.playlist_music_button(user_id, cx) {
            body = body.child(button);
        }
        if let Some(section) =
            self.media_counts_section(if groups_loaded { 0 } else { groups_in_common }, cx)
        {
            body = body.child(section);
        }
        if let Some(section) = self.groups_in_common_section(user_id, groups_in_common, cx) {
            body = body.child(section);
        }
        // Delete contact, a bot's Report, then Block, in the order of
        // tdesktop `ActionsFiller::fillUserActions`.
        if !is_self && user.is_some() {
            let blocked = info.as_ref().is_some_and(|i| i.blocked);
            let is_bot = user.as_ref().is_some_and(|u| u.is_bot);
            let mut danger = div().flex().flex_col().w_full().gap_0p5();
            if user.as_ref().is_some_and(|u| u.is_contact) {
                danger = danger.child(
                    action_row(
                        "info-panel-delete-contact",
                        Some(gpui_kit::assets::IconName::UserX),
                        tr(keys::INFO_DELETE_CONTACT),
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
            danger = danger.children(self.bot_report_row(user_id, cx));
            danger = danger.child(
                action_row(
                    "info-panel-block-user",
                    Some(gpui_kit::assets::IconName::Ban),
                    match (is_bot, blocked) {
                        (true, true) => tr(keys::PROFILE_RESTART_BOT),
                        (true, false) => tr(keys::PROFILE_BLOCK_BOT),
                        (false, true) => tr(keys::PROFILE_UNBLOCK_USER),
                        (false, false) => tr(keys::PROFILE_BLOCK_USER),
                    },
                    !blocked,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    if is_bot {
                        this.toggle_bot_block(user_id, blocked, window, cx);
                        return;
                    }
                    this.open_group_confirm(
                        ChatId(0),
                        GroupConfirmAction::BlockContact {
                            user_id,
                            block: !blocked,
                        },
                        cx,
                    );
                }))
                .test_support(),
            );
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
        let name = Self::secret_peer_name(self.session(), user_id, tr(keys::Q_PEOPLE_THIS_CHAT));
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
                    .child(tr(keys::Q_PEOPLE_PASSCODE)),
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
            .and_then(|s| s.supergroup_chat(supergroup_id))
            .and_then(|chat| match chat.kind {
                ChatKind::Supergroup { is_channel, .. } => {
                    Some((chat.title.clone(), chat.id, is_channel))
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                (
                    tr_with(
                        keys::Q_PEOPLE_GROUP,
                        &[("supergroup_id", &supergroup_id.to_string())],
                    ),
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
        let mut body = self.supergroup_identity(chat_id, title.clone(), is_channel, members, cx);
        body = body.children(self.profile_action_row(ProfilePeer::Chat(chat_id), cx));
        body = body.children(self.group_link_card(supergroup_id, chat_id, &title, cx));
        if !description.is_empty() {
            // tdesktop strips external links from a group's description.
            let entities = info
                .as_ref()
                .map(|i| i.description_entities.clone())
                .unwrap_or_default();
            let entities = if is_channel {
                entities
            } else {
                quill::text::about::strip_external_links(&description, &entities)
            };
            body = body.child(self.about_text_block(chat_id, &description, &entities, cx));
        }
        if let Some(verification) =
            session.and_then(|s| s.supergroup_bot_verification(supergroup_id))
            && let Some(text) =
                quill::bot_verification::profile_description(Some(verification), false)
        {
            body =
                body.child(self.bot_verification_row(text, verification.icon_custom_emoji_id, cx));
        }
        // Phase D2: statistics entry point — shown only when
        // `supergroupFullInfo.can_get_statistics` is true (schema 1.8.67,
        // line 2792); `getChatStatistics` errors when it is false.
        if info.as_ref().is_some_and(|i| i.can_get_statistics) {
            let stats_chat_id = chat_id;
            body = body.child(
                Button::new("open-channel-statistics")
                    .label(tr(keys::Q_PEOPLE_STATISTICS))
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
        body = body.children(self.invite_links_section(chat_id, cx));
        body = body.children(self.join_requests_section(chat_id, cx));
        // Phase D3b: administrator management (owner / admins with
        // `can_promote_members` only; the section no-ops otherwise).
        body = body.children(self.administrators_section(chat_id, cx));
        body = body.children(self.channel_balance_row(chat_id, cx));
        // Slice G1: group/channel management — members, permissions,
        // invite link, join requests, username, broadcast upgrade,
        // leave/delete. Every row is capability-gated inside.
        // tdesktop's channel actions: Join Channel, Leave channel (in the
        // management section), then Report for a channel the viewer does
        // not own.
        let join_row = if is_channel {
            self.channel_join_row(chat_id, cx)
        } else {
            None
        };
        // A channel the viewer has not joined has nothing to manage, so the
        // section (an empty header) is left out.
        let not_joined = join_row.is_some();
        body = body.children(join_row);
        if !not_joined {
            body = body.child(self.group_management_section(chat_id, is_channel, false, cx));
        }
        if is_channel {
            body = body.children(self.channel_report_row(chat_id, cx));
        }
        // Phase D3c: recent-actions admin log (administrators and the
        // creator only; the section no-ops otherwise).
        body = body.children(self.event_log_section(chat_id, cx));
        // Slice G2: channel boost status + boost action.
        body = body.children(self.boost_section(chat_id, cx));
        body = body
            .children(self.inline_members_block(InfoPanelTarget::Supergroup(supergroup_id), cx));
        body.into_any_element()
    }

    /// The top of `supergroup_info_panel`: avatar, title with badges and
    /// the member count with how many are online. A method of its own, so the panel's
    /// frame does not also hold this builder chain
    /// (docs/decisions/main-thread-stack.md).
    fn supergroup_identity(
        &self,
        chat_id: ChatId,
        title: String,
        is_channel: bool,
        members: i32,
        cx: &mut Context<Self>,
    ) -> Div {
        let session = self.session();
        let online = session
            .and_then(|s| {
                s.chats
                    .get(&chat_id.0)
                    .and_then(|c| s.group_member_counts(c))
            })
            .map_or(0, |(_, online)| online);
        // Parity slice: the panel avatar reuses the chat-list avatar (photo
        // or colored initials).
        let roots = self.media_display_roots();
        let photo = session
            .and_then(|s| s.chat_photo_path(chat_id))
            .and_then(|path| sandboxed_display_path(path, &roots));
        let verify_icon = session
            .and_then(|s| s.chats.get(&chat_id.0))
            .and_then(|chat| self.bot_verify_icon_path(chat));
        // Verified check, emoji status or SCAM / FAKE chip after the title
        // (`Info::Profile::Badge`).
        let badges = session
            .and_then(|s| {
                s.chats
                    .get(&chat_id.0)
                    .map(|chat| s.chat_header_badges(chat))
            })
            .unwrap_or_default();
        let badges = self.info_header_badges(badges, chat_id.0, cx);
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .p_4()
            .child(self.chat_photo_avatar(chat_id, &title, photo.as_deref(), cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .max_w_full()
                            .when_some(verify_icon, |this, path| {
                                this.child(crate::ui::bot_verify_icon::bot_verify_icon(path))
                            })
                            .child(
                                div()
                                    .min_w_0()
                                    .text_lg()
                                    .font_semibold()
                                    .truncate()
                                    .child(super::bidi_line::one_line_plain(title.clone())),
                            )
                            .children(badges),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .when(members > 0, |this| {
                                this.child(quill::inline_members::member_status_line(
                                    members, is_channel, online,
                                ))
                            }),
                    ),
            )
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
            .unwrap_or_else(|| {
                (
                    tr_with(
                        keys::Q_PEOPLE_GROUP_2,
                        &[("basic_group_id", &basic_group_id.to_string())],
                    ),
                    ChatId(basic_group_id),
                )
            });
        let member_count = self
            .session()
            .and_then(|s| s.groups.basic_group_members.get(&chat_id.0))
            .and_then(|fetch| match fetch {
                SupergroupMembersFetch::Loaded { total_count, .. } => Some(*total_count),
                _ => None,
            });
        let online = self
            .session()
            .and_then(|s| {
                s.chats
                    .get(&chat_id.0)
                    .and_then(|c| s.group_member_counts(c))
            })
            .map_or(0, |(_, online)| online);
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
            .child(self.chat_photo_avatar(chat_id, &title, photo.as_deref(), cx))
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
                                this.child(quill::inline_members::member_status_line(
                                    count, false, online,
                                ))
                            }),
                    ),
            );
        body = body.children(self.profile_action_row(ProfilePeer::Chat(chat_id), cx));
        // `basicGroupFullInfo.description`, plain text.
        let description = self
            .session()
            .and_then(|s| s.chat_description(chat_id))
            .unwrap_or_default()
            .to_string();
        if !description.is_empty() {
            body = body.child(self.about_text_block(chat_id, &description, &[], cx));
        }
        if let Some(section) = self.media_counts_section(0, cx) {
            body = body.child(section);
        }
        body = body.child(self.group_management_section(chat_id, false, true, cx));
        body = body
            .children(self.inline_members_block(InfoPanelTarget::BasicGroup(basic_group_id), cx));
        body.into_any_element()
    }
}
