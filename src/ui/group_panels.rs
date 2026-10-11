//! info panels (user/supergroup/community).

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::chat_row::{chat_avatar, compact_count};
use super::pressable::PressableDiv;
use super::pressable::action_row;
use super::profile_panels::{CopyAction, copy_menu};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::local_path::sandboxed_display_path;
use quill::state::ProfileChatsKind;
use quill::state::{ContactRow, InfoPanelTarget, SupergroupMembersFetch};
use quill::telegram::envelope::ChatKind;
use quill::telegram::envelope::MUTE_FOREVER;
use std::path::PathBuf;
/// Parity slice: data for the channel/supergroup conversation header —
/// primary @username, subscriber/member count, and the linked discussion
/// chat id (`linked_chat_id`, 0 = none).
pub(super) struct SupergroupHeaderExtras {
    pub(super) is_channel: bool,
    pub(super) username: Option<String>,
    pub(super) member_count: Option<i32>,
    /// Members online (groups, while open); 0 when unknown.
    pub(super) online_count: i32,
}

impl QuillApp {
    /// Slice G2: boost the channel (`getAvailableChatBoostSlots` →
    /// `boostChat` with the first slot; the driver chains them).
    pub(super) fn boost_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.request_chat_boost(chat_id) {
                Ok(_) => self.connection.status_note = "boost requested".into(),
                Err(_) => self.connection.status_note = "could not boost channel".into(),
            }
        } else {
            self.connection.status_note = "boosts need a live connection (demo)".into();
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
            let photo = self
                .session()
                .and_then(|s| s.user_photo_path(user_id))
                .and_then(|path| sandboxed_display_path(path, &self.media_display_roots()));
            panel = panel.child(
                div()
                    .id(("new-secret-contact", user_id as u64))
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("Start secret chat with {name}"))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .bg(cx.theme().sidebar)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.share.new_secret_picker_open = false;
                        this.start_secret_chat_for_user(user_id, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(chat_avatar(&name, photo.as_deref(), 32.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .font_medium()
                                            .text_sm()
                                            .truncate()
                                            .child(super::bidi_line::one_line_plain(name)),
                                    )
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
            // Shared-media counts for the open chat (Telegram Desktop's
            // "N photos · N videos …" rows).
            if !matches!(
                target,
                InfoPanelTarget::Statistics(_) | InfoPanelTarget::Community(_)
            ) && let Some(chat_id) = live.driver.session.open_chat
            {
                let _ = live.driver.fetch_chat_media_counts(chat_id);
            }
            let fetch = match target {
                InfoPanelTarget::User(user_id) => {
                    // B10: the groups you share with the user, listed under
                    // the media counts (yourself has none).
                    if live.driver.session.my_user_id != Some(user_id) {
                        let _ = live.driver.fetch_groups_in_common(user_id);
                    }
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
                            // B8: other admins' link counts (owner only).
                            live.driver
                                .fetch_chat_invite_link_counts(chat_id)
                                .map(|_| ()),
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
                        // B10: similar channels (cached and deduped by the
                        // driver; best-effort, the section just stays out).
                        if is_channel {
                            let _ = live.driver.fetch_similar_chats(chat_id);
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
                // the driver); the answer is the `communityFullInfo` pack.
                InfoPanelTarget::Community(community_id) => live
                    .driver
                    .get_community_full_info(community_id)
                    .map(|_| ()),
            };
            if let Err(err) = fetch {
                self.connection.status_note = format!("info request failed: {err:?}");
            } else if let InfoPanelTarget::User(user_id) = target
                && let Err(err) = live.driver.download_user_photo(user_id)
            {
                self.connection.status_note = format!("photo download failed: {err:?}");
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.users_state.open_info_panel = Some(target);
        }
        // Slice G2: the event-log section's search box (created lazily;
        // its value syncs to the panel chat's stored query). The per-admin
        // filter lives in the session and follows the chat.
        if self.admin.event_log_search.is_none() {
            self.admin.event_log_search = Some(cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Search recent actions")
                    .auto_grow(1, 1)
                    .submit_on_enter(false)
            }));
        }
        if let Some(chat_id) = invite_panel_chat_id {
            let query = self
                .session()
                .and_then(|session| session.groups.event_log_queries.get(&chat_id.0).cloned())
                .unwrap_or_default();
            if let Some(input) = self.admin.event_log_search.clone() {
                input.update(cx, |state, cx| state.set_value(&query, window, cx));
            }
        }
        cx.notify();
    }

    pub(super) fn close_info_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.set_info_panel(None);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.users_state.open_info_panel = None;
        }
        cx.notify();
    }

    /// Right-side info panel for the open `InfoPanelTarget` (user profile
    /// or group info). Rendered next to the conversation in the shell.
    /// Telegram Desktop's shared-media rows ("161 photos", "10 videos" …,
    /// "6 groups in common") for the open chat; a row opens that tab of
    /// the shared-media gallery. `None` until a count arrives.
    pub(super) fn media_counts_section(
        &self,
        groups_in_common: i32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use quill::state::SharedMediaTab;
        let session = self.session()?;
        // The counts belong to the open chat: they describe the panel's
        // peer only when the panel is that chat's own (not a group
        // member's profile opened from a sender avatar).
        let counts = session
            .open_chat
            .filter(|chat_id| {
                session.info_panel_target_for_chat(*chat_id) == session.users_state.open_info_panel
            })
            .and_then(|chat_id| session.media.chat_media_counts.get(&chat_id.0));
        let icons = [
            IconName::Image,
            IconName::Video,
            IconName::File,
            IconName::AudioLines,
            IconName::Link,
            IconName::Mic,
            IconName::Film,
        ];
        let tabs = [
            SharedMediaTab::Media,
            SharedMediaTab::Media,
            SharedMediaTab::Files,
            SharedMediaTab::Music,
            SharedMediaTab::Links,
            SharedMediaTab::Voice,
            SharedMediaTab::Gifs,
        ];
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, (_, one, many)) in quill::telegram::requests::MEDIA_COUNT_FILTERS
            .iter()
            .enumerate()
        {
            let Some(&count) = counts
                .and_then(|counts| counts.get(&(index as u8)))
                .filter(|count| **count > 0)
            else {
                continue;
            };
            let tab = tabs[index];
            rows.push(
                action_row(
                    ("info-media-count", index as u64),
                    Some(icons[index]),
                    format!(
                        "{} {}",
                        compact_count(count),
                        if count == 1 { *one } else { *many }
                    ),
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.open_shared_media_tab(tab, cx)))
                .into_any_element(),
            );
        }
        if groups_in_common > 0 {
            rows.push(
                action_row(
                    "info-groups-in-common",
                    Some(IconName::Users),
                    format!(
                        "{groups_in_common} group{} in common",
                        if groups_in_common == 1 { "" } else { "s" }
                    ),
                    false,
                    cx,
                )
                .into_any_element(),
            );
        }
        (!rows.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .w_full()
                .gap_0p5()
                .pt_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .children(rows)
                .into_any_element()
        })
    }

    /// Open the shared-media gallery on `tab`.
    pub(super) fn open_shared_media_tab(
        &mut self,
        tab: quill::state::SharedMediaTab,
        cx: &mut Context<Self>,
    ) {
        self.open_shared_media_ui(cx);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.select_shared_media_tab(tab);
        }
        cx.notify();
    }

    pub(super) fn info_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // A profile opened from a sender avatar is presented as a modal
        // layer (`profile_modal_overlay`), not in the right column.
        if self.profile_modal_active() {
            return None;
        }
        let (title, content) = self.info_panel_parts(cx)?;
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
                                .accessibility_label("Close information panel")
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

    /// Title and content of the open info panel, shared by the right
    /// column and the profile modal.
    pub(super) fn info_panel_parts(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(&'static str, AnyElement)> {
        let target = self.session()?.users_state.open_info_panel?;
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
        Some((title, content))
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
            .map(|u| {
                if u.is_bot {
                    "bot".to_string()
                } else {
                    u.status.display()
                }
            })
            .unwrap_or_default();
        let show_add = user.as_ref().is_some_and(|u| !u.is_contact && !u.is_bot);
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
        // Phase B1 / C1: secret chat and calls share the eligibility rule
        // (non-bot users, not yourself).
        let can_reach = session
            .as_ref()
            .is_some_and(|s| Self::can_start_secret_chat_with(s, user_id));
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
        // Primary actions as a row of labeled icon tiles.
        let mut tiles = div().flex().justify_center().gap_2().w_full();
        let mut any_tile = false;
        // The open chat is this very user's private chat (header panel):
        // Message and Mute then act on it; from a group member's profile
        // (avatar click) Message opens the private chat and Mute is left
        // out (it would mute the group).
        let in_own_chat = session
            .and_then(|s| s.open_chat.map(|chat| (s, chat)))
            .is_some_and(|(s, chat)| s.private_chat_user_id(chat) == Some(user_id));
        if !is_self && !in_own_chat {
            any_tile = true;
            tiles = tiles.child(info_tile(
                "info-panel-message",
                gpui_kit::assets::IconName::MessageSquare,
                "Message",
                cx.listener(move |this, _, window, cx| {
                    this.dismiss_profile_modal();
                    this.open_user_chat(user_id, window, cx);
                }),
                cx,
            ));
        }
        if can_reach {
            any_tile = true;
            tiles = tiles
                .child(info_tile(
                    "info-panel-call",
                    gpui_kit::assets::IconName::Phone,
                    "Call",
                    cx.listener(move |this, _, _, cx| this.start_call_for_user(user_id, false, cx)),
                    cx,
                ))
                .child(info_tile(
                    "info-panel-video-call",
                    gpui_kit::assets::IconName::Video,
                    "Video",
                    cx.listener(move |this, _, _, cx| this.start_call_for_user(user_id, true, cx)),
                    cx,
                ))
                .child(info_tile(
                    "info-panel-start-secret",
                    gpui_kit::assets::IconName::Lock,
                    "Secret chat",
                    cx.listener(move |this, _, _, cx| this.start_secret_chat_for_user(user_id, cx)),
                    cx,
                ));
        }
        if show_add {
            any_tile = true;
            tiles = tiles.child(info_tile(
                "info-panel-add-contact",
                gpui_kit::assets::IconName::UserPlus,
                "Add contact",
                cx.listener(move |this, _, window, cx| {
                    this.open_add_contact_dialog(user_id, window, cx)
                }),
                cx,
            ));
        }
        // A5: profile editing only on your own panel.
        if is_self {
            any_tile = true;
            tiles = tiles.child(info_tile(
                "info-panel-edit-profile",
                gpui_kit::assets::IconName::Pencil,
                "Edit profile",
                cx.listener(|this, _, window, cx| this.open_edit_profile_dialog(window, cx)),
                cx,
            ));
        }
        // Telegram Desktop's Mute button, for the chat with this user.
        if !is_self
            && in_own_chat
            && let Some(chat) = session.and_then(|s| s.open_chat)
        {
            let muted = session
                .and_then(|s| s.chats.get(&chat.0))
                .is_some_and(|c| c.is_muted());
            any_tile = true;
            tiles = tiles.child(info_tile(
                "info-panel-mute",
                if muted {
                    gpui_kit::assets::IconName::Bell
                } else {
                    gpui_kit::assets::IconName::BellOff
                },
                if muted { "Unmute" } else { "Mute" },
                cx.listener(move |this, _, _, cx| {
                    this.apply_chat_mute(chat, if muted { 0 } else { MUTE_FOREVER }, cx);
                }),
                cx,
            ));
        }
        if any_tile {
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

/// Labeled icon tile for an info panel's primary actions.
fn info_tile(
    id: &'static str,
    icon: gpui_kit::assets::IconName,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .w(px(76.))
        .py_2()
        .rounded_lg()
        .bg(cx.theme().secondary)
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .role(gpui_kit::Role::Button)
        .aria_label(label)
        .tab_index(0)
        .on_click(on_click)
        .child(Icon::new(icon).size(px(18.)).text_color(cx.theme().primary))
        .child(div().text_xs().child(label))
}

/// `+15550101031` → `+1 555 010 1031`-style grouping for readability;
/// numbers that don't look like E.164 pass through unchanged.
pub(super) fn format_phone(raw: &str) -> String {
    // Grouping by country code, as Telegram's phone formatter does for the
    // common ones; other codes keep their digits.
    const PATTERNS: [(&str, &[usize]); 14] = [
        ("1", &[3, 3, 4]),
        ("7", &[3, 3, 2, 2]),
        ("20", &[2, 4, 4]),
        ("33", &[1, 2, 2, 2, 2]),
        ("34", &[3, 3, 3]),
        ("39", &[3, 3, 4]),
        ("44", &[4, 6]),
        ("49", &[3, 4, 4]),
        ("55", &[2, 5, 4]),
        ("61", &[1, 4, 4]),
        ("86", &[3, 4, 4]),
        ("91", &[5, 5]),
        ("380", &[2, 3, 2, 2]),
        ("972", &[2, 3, 4]),
    ];
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 7 || digits.len() > 15 {
        return raw.to_string();
    }
    for (code, groups) in PATTERNS {
        let rest = match digits.strip_prefix(code) {
            Some(rest) if rest.len() == groups.iter().sum::<usize>() => rest,
            _ => continue,
        };
        let mut out = format!("+{code}");
        let mut at = 0;
        for len in groups {
            out.push(' ');
            out.push_str(&rest[at..at + len]);
            at += len;
        }
        return out;
    }
    format!("+{digits}")
}

/// "Jul 30, 1965 (61 years old)" — Telegram Desktop's birthday row.
pub(super) fn format_birthday(date: quill::telegram::envelope::Birthdate) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = MONTHS[usize::from(date.month.clamp(1, 12)) - 1];
    match date.year {
        Some(year) => {
            let today = quill::local_time::civil_local(quill::local_time::now_unix());
            let mut age = today.year as i32 - year;
            if (today.month, today.day) < (date.month, date.day) {
                age -= 1;
            }
            format!("{month} {}, {year} ({age} years old)", date.day)
        }
        None => format!("{month} {}", date.day),
    }
}

#[cfg(test)]
mod tests {
    use super::format_phone;

    #[test]
    fn phone_numbers_group_for_reading() {
        assert_eq!(format_phone("15550101031"), "+1 555 010 1031");
        assert_eq!(format_phone("+15550101031"), "+1 555 010 1031");
        assert_eq!(format_phone("+972502002287"), "+972 50 200 2287");
        assert_eq!(format_phone("442071838750"), "+44 2071 838750");
        // Unknown country codes keep their digits rather than guessing groups.
        assert_eq!(format_phone("3530861234567"), "+3530861234567");
        assert_eq!(format_phone("12"), "12");
    }
}
