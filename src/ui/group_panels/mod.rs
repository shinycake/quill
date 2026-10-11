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
}

/// Labeled icon tile for an info panel's primary actions.
pub(in crate::ui) fn info_tile(
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

mod user_info_panel;

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
