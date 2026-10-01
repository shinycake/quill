//! info panels (user/supergroup/community).

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::chat_row::{chat_avatar, compact_count};
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::local_path::sandboxed_display_path;
use quill::state::{ContactRow, InfoPanelTarget, SupergroupMembersFetch};
use quill::telegram::envelope::ChatKind;
use std::path::PathBuf;
/// Parity slice: data for the channel/supergroup conversation header —
/// photo, description snippet, primary @username, subscriber/member count,
/// and the linked discussion chat id (`linked_chat_id`, 0 = none).
pub(super) struct SupergroupHeaderExtras {
    pub(super) is_channel: bool,
    pub(super) photo: Option<PathBuf>,
    pub(super) username: Option<String>,
    pub(super) member_count: Option<i32>,
    pub(super) description_snippet: Option<String>,
    pub(super) discussion_chat_id: Option<i64>,
}

impl QuillApp {
    /// Slice G2: boost the channel (`getAvailableChatBoostSlots` →
    /// `boostChat` with the first slot; the driver chains them).
    pub(super) fn boost_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.request_chat_boost(chat_id) {
                Ok(_) => self.status_note = "boost requested".into(),
                Err(_) => self.status_note = "could not boost channel".into(),
            }
        } else {
            self.status_note = "boosts need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Slice G10: community info panel body (delegates to
    /// dialogs/community.rs so this file only holds the thin method).
    pub(super) fn community_info_panel(
        &self,
        community_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        community::render_community_info_panel(self, community_id, cx)
    }

    pub(super) fn open_user_panel(
        &mut self,
        user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_info_panel_target(InfoPanelTarget::User(user_id), window, cx);
    }

    /// Phase S1: "New secret chat" contact picker — eligible contacts
    /// (non-bot, not self, via `can_start_secret_chat_with`) rendered in
    /// the contacts-list style; picking one calls
    /// `start_secret_chat_for_user` and closes the picker.
    pub(super) fn new_secret_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<ContactRow> = self
            .session()
            .map(|s| {
                s.contact_rows()
                    .into_iter()
                    .filter(|row| Self::can_start_secret_chat_with(s, row.user_id))
                    .collect()
            })
            .unwrap_or_default();
        let mut panel = div()
            .id("new-secret-picker")
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Start a secret chat with:"),
            );
        if rows.is_empty() {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No eligible contacts."),
            );
        }
        for row in rows {
            let user_id = row.user_id;
            let name = row.name.clone();
            let status = row.status_text.clone();
            panel = panel.child(
                div()
                    .id(("new-secret-contact", user_id as u64))
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .bg(cx.theme().sidebar)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.new_secret_picker_open = false;
                        this.start_secret_chat_for_user(user_id, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(initials_avatar(&name, 32.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .child(div().font_medium().text_sm().child(name))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(status),
                                    ),
                            ),
                    ),
            );
        }
        panel
    }

    pub(super) fn open_supergroup_panel(
        &mut self,
        supergroup_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_info_panel_target(InfoPanelTarget::Supergroup(supergroup_id), window, cx);
    }

    /// Open an info panel: set the target, then fetch its data on the live
    /// path (full info + profile photo for users, full info for
    /// supergroups). The demo path only sets the target — the fixture
    /// pre-seeds the data.
    pub(super) fn open_info_panel_target(
        &mut self,
        target: InfoPanelTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Phase D3a: resolve the chat for a supergroup info-panel target
        // before the live driver is borrowed mutably below.
        let invite_panel_chat_id = match target {
            InfoPanelTarget::Supergroup(supergroup_id) => self.session().and_then(|session| {
                session.chats.values().find_map(|chat| match chat.kind {
                    ChatKind::Supergroup {
                        supergroup_id: id, ..
                    } if id == supergroup_id => Some(chat.id),
                    _ => None,
                })
            }),
            _ => None,
        };
        // Slice G1: resolve the basic-group chat before the driver
        // borrow below.
        let g1_basic_chat = match target {
            InfoPanelTarget::BasicGroup(basic_group_id) => self.session().and_then(|session| {
                session.chats.values().find_map(|chat| match chat.kind {
                    ChatKind::BasicGroup { basic_group_id: id } if id == basic_group_id => {
                        Some(chat.id)
                    }
                    _ => None,
                })
            }),
            _ => None,
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.set_info_panel(Some(target));
            let fetch = match target {
                InfoPanelTarget::User(user_id) => {
                    live.driver.fetch_user_full_info(user_id).map(|_| ())
                }
                InfoPanelTarget::Supergroup(supergroup_id) => {
                    let mut result = live
                        .driver
                        .fetch_supergroup_full_info(supergroup_id)
                        .map(|_| ());
                    if let Some(chat_id) = invite_panel_chat_id {
                        // Phase D3a: invite-link / join-request lists for
                        // admins; the driver no-ops when the
                        // `can_invite_users` gate is closed.
                        // Phase D3b: administrator list; the driver
                        // no-ops unless the viewer may manage admins
                        // (owner or `can_promote_members`).
                        // Phase D3c: recent-actions log; the driver no-ops
                        // unless the viewer is an administrator/creator.
                        for fetch in [
                            live.driver.fetch_chat_invite_links(chat_id).map(|_| ()),
                            live.driver.fetch_chat_join_requests(chat_id).map(|_| ()),
                            live.driver.fetch_chat_administrators(chat_id).map(|_| ()),
                            live.driver.fetch_chat_event_log(chat_id).map(|_| ()),
                        ] {
                            if fetch.is_err() {
                                result = fetch;
                            }
                        }
                        // Slice G2: channel boost status (channels only;
                        // cached and deduped by the driver).
                        let is_channel =
                            live.driver
                                .session
                                .chats
                                .get(&chat_id.0)
                                .is_some_and(|chat| {
                                    matches!(
                                        chat.kind,
                                        ChatKind::Supergroup {
                                            is_channel: true,
                                            ..
                                        }
                                    )
                                });
                        if is_channel && let Err(err) = live.driver.fetch_chat_boost_status(chat_id)
                        {
                            result = Err(err);
                        }
                        // Slice G2: welcome-message pack for admins who may
                        // send them (the driver dedupes on a cached pack).
                        if live.driver.session.chat_can_send_welcome_messages(chat_id)
                            && let Err(err) = live.driver.load_chat_welcome_messages(chat_id)
                        {
                            result = Err(err);
                        }
                    }
                    result
                }
                // Phase D2: `is_dark` only tints server-rendered graph
                // images; Quill draws its own sparklines from `json_data`
                // and the app has no dark-mode concept, so `false`.
                InfoPanelTarget::Statistics(chat_id) => live
                    .driver
                    .fetch_chat_statistics(ChatId(chat_id), false)
                    .map(|_| ()),
                // Slice G1: basic-group member list; the chat lookup
                // happened before the driver borrow.
                InfoPanelTarget::BasicGroup(_) => match g1_basic_chat {
                    Some(chat_id) => live.driver.fetch_basic_group_members(chat_id).map(|_| ()),
                    None => Ok(()),
                },
                // Slice G10: community full info (cached + deduped by
                // the driver); arrives as `updateCommunityFullInfo`.
                InfoPanelTarget::Community(community_id) => live
                    .driver
                    .load_community_full_info(community_id)
                    .map(|_| ()),
            };
            if let Err(err) = fetch {
                self.status_note = format!("info request failed: {err:?}");
            } else if let InfoPanelTarget::User(user_id) = target
                && let Err(err) = live.driver.download_user_photo(user_id)
            {
                self.status_note = format!("photo download failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_info_panel = Some(target);
        }
        // Slice G2: the event-log section's search box (created lazily;
        // its value syncs to the panel chat's stored query). The per-admin
        // filter resets whenever the panel target changes.
        self.event_log_admin_filter = None;
        if self.event_log_search.is_none() {
            self.event_log_search = Some(cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Search recent actions")
                    .auto_grow(1, 1)
                    .submit_on_enter(false)
            }));
        }
        if let Some(chat_id) = invite_panel_chat_id {
            let query = self
                .session()
                .and_then(|session| session.event_log_queries.get(&chat_id.0).cloned())
                .unwrap_or_default();
            if let Some(input) = self.event_log_search.clone() {
                input.update(cx, |state, cx| state.set_value(&query, window, cx));
            }
        }
        cx.notify();
    }

    pub(super) fn close_info_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.set_info_panel(None);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_info_panel = None;
        }
        cx.notify();
    }

    /// Right-side info panel for the open `InfoPanelTarget` (user profile
    /// or group info). Rendered next to the conversation in the shell.
    pub(super) fn info_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self.session()?.open_info_panel?;
        let (title, content) = match target {
            InfoPanelTarget::User(user_id) => ("Contact info", self.user_info_panel(user_id, cx)),
            InfoPanelTarget::Supergroup(supergroup_id) => {
                ("Group info", self.supergroup_info_panel(supergroup_id, cx))
            }
            // Slice G1: basic-group info panel (members, permissions,
            // invite link, leave/delete).
            InfoPanelTarget::BasicGroup(basic_group_id) => (
                "Group info",
                self.basic_group_info_panel(basic_group_id, cx),
            ),
            // Phase D2: channel/group statistics view.
            InfoPanelTarget::Statistics(chat_id) => {
                ("Statistics", self.chat_statistics_panel(chat_id, cx))
            }
            // Slice G10: community info (name edit, counts, chats).
            InfoPanelTarget::Community(community_id) => (
                "Community info",
                self.community_info_panel(community_id, cx),
            ),
        };
        Some(
            div()
                .id("info-panel")
                .w(px(300.))
                .h_full()
                .flex_shrink_0()
                .border_l_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().sidebar)
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(div().font_semibold().child(title))
                        .child(
                            Button::new("info-panel-close")
                                .icon(IconName::X)
                                .ghost()
                                .tooltip("Close panel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_info_panel(cx);
                                })),
                        ),
                )
                // Phase B2: the content scrolls — the encryption-key
                // section can push a contact panel past the window
                // height.
                .child(
                    div()
                        .id("info-panel-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(content),
                )
                .into_any_element(),
        )
    }

    /// User profile panel: photo (downloaded `userFullInfo.photo` size, or
    /// an initials avatar), name, status, username/phone rows, bio, and an
    /// Add contact affordance for known non-contacts.
    pub(super) fn user_info_panel(&self, user_id: i64, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let user = session.and_then(|s| s.user(user_id)).cloned();
        let info = session.and_then(|s| s.user_full_info(user_id)).cloned();
        let name = user
            .as_ref()
            .map(|u| u.display_name())
            .unwrap_or_else(|| format!("User {user_id}"));
        let status = user
            .as_ref()
            .map(|u| u.status.display())
            .unwrap_or_default();
        let username = user
            .as_ref()
            .map(|u| u.username.clone())
            .unwrap_or_default();
        let phone = user
            .as_ref()
            .map(|u| u.phone_number.clone())
            .unwrap_or_default();
        let bio = info.as_ref().map(|i| i.bio.clone()).unwrap_or_default();
        let show_add = user.as_ref().is_some_and(|u| !u.is_contact && !u.is_bot);
        let roots = self.media_display_roots();
        let photo_path: Option<PathBuf> = session
            .and_then(|s| {
                info.as_ref()
                    .and_then(|i| i.photo_file_id)
                    .and_then(|id| s.files.get(&id))
                    .and_then(|file| file.usable_path())
            })
            .and_then(|path| sandboxed_display_path(path, &roots));
        let avatar: AnyElement = match photo_path {
            Some(path) => img(path)
                .id(("info-panel-photo", user_id as u64))
                .w(px(96.))
                .h(px(96.))
                .rounded_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => initials_avatar(&name, 96.).into_any_element(),
        };
        let mut detail_rows: Vec<(&str, String)> = Vec::new();
        if !username.is_empty() {
            detail_rows.push(("Username", format!("@{username}")));
        }
        if !phone.is_empty() {
            detail_rows.push(("Phone", phone));
        }
        let mut body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .p_4()
            .child(avatar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(div().text_lg().font_semibold().child(name.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    ),
            );
        for (label, value) in detail_rows {
            body = body.child(
                div()
                    .flex()
                    .w_full()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(label),
                    )
                    .child(div().text_sm().child(value)),
            );
        }
        if !bio.is_empty() {
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
                            .child("Bio"),
                    )
                    .child(div().text_sm().child(bio)),
            );
        }
        if show_add {
            body = body.child(
                Button::new("info-panel-add-contact")
                    .label("Add contact")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_add_contact_dialog(user_id, window, cx);
                    })),
            );
        }
        // A5: the profile edit UI entry point — only on your own panel
        // (`setName` / `setBio` / `setUsername` / `setProfilePhoto` all
        // act on the current user).
        let is_self = session
            .and_then(|s| s.my_user_id)
            .is_some_and(|me| me == user_id);
        if is_self {
            body = body.child(
                Button::new("info-panel-edit-profile")
                    .label("Edit profile")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_edit_profile_dialog(window, cx);
                    })),
            );
        }
        // Slice A6: contact management for any other user — "Delete
        // contact" when they are a contact (`removeContacts`, schema
        // 1.8.67 line 14528; TGX `DeleteContactConfirm` "Delete %1$s
        // from contacts?") and Block/Unblock
        // (`setMessageSenderBlockList`, schema 1.8.67 line 14492; TGX
        // `BlockUserConfirm` "Are you sure you want to block %1$s?").
        // The blocked state comes from `userFullInfo.block_list`
        // (schema 1.8.67, line 2468).
        if !is_self && user.is_some() {
            let blocked = info.as_ref().is_some_and(|i| i.blocked);
            let mut actions = div().flex().flex_wrap().gap_2();
            if user.as_ref().is_some_and(|u| u.is_contact) {
                actions = actions.child(
                    Button::new("info-panel-delete-contact")
                        .label("Delete contact")
                        .ghost()
                        .danger()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_group_confirm(
                                ChatId(0),
                                GroupConfirmAction::DeleteContact { user_id },
                                cx,
                            );
                        })),
                );
            }
            actions = actions.child(
                Button::new("info-panel-block-user")
                    .label(if blocked {
                        "Unblock user"
                    } else {
                        "Block user"
                    })
                    .ghost()
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
            body = body.child(actions);
        }
        // Phase B1: "Start secret chat" from a user profile — E2E chat
        // with a non-bot user (`createNewSecretChat`, schema 1.8.67 line
        // 13340). Not offered for bots or for yourself.
        let show_start_secret = session
            .as_ref()
            .is_some_and(|s| Self::can_start_secret_chat_with(s, user_id));
        if show_start_secret {
            body = body.child(
                Button::new("info-panel-start-secret")
                    .label("Start secret chat")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.start_secret_chat_for_user(user_id, cx);
                    })),
            );
        }
        // Phase C1: "Call" from a user profile — `createCall`
        // (audio-only). Phase C1b: "🎥 Video call" — `createCall` with
        // `is_video: true` (signaling only; media transport is C2). Same
        // gating as secret chats: non-bot users, not yourself.
        let show_call = show_start_secret;
        if show_call {
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("info-panel-call")
                            .label("Call")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.start_call_for_user(user_id, false, cx);
                            })),
                    )
                    .child(
                        Button::new("info-panel-video-call")
                            .label("🎥 Video call")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.start_call_for_user(user_id, true, cx);
                            })),
                    ),
            );
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
    pub(super) fn secret_passcode_hint(&self, user_id: i64, cx: &mut Context<Self>) -> AnyElement {
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
    pub(super) fn supergroup_info_panel(
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
            .and_then(|s| s.supergroup_full_infos.get(&supergroup_id))
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
                    .child(div().text_lg().font_semibold().child(title.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if members > 0 {
                                format!("{} {noun}", compact_count(members))
                            } else {
                                format!("{noun} unknown")
                            }),
                    )
                    .when_some(username, |this, name| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("@{name}")),
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
    pub(super) fn basic_group_info_panel(
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
            .and_then(|s| s.basic_group_members.get(&chat_id.0))
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
                    .child(div().text_lg().font_semibold().child(title.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(match member_count {
                                Some(count) => format!("{count} members"),
                                None => "members unknown".to_string(),
                            }),
                    ),
            );
        body = body.child(self.group_management_section(chat_id, false, true, cx));
        body.into_any_element()
    }
}
