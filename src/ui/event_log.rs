//! admin log (recent actions) panel.

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::{ChatEventLogFetch, event_log_relative_time};
use quill::telegram::envelope::{
    ChannelMemberStatus, ChatEventAction, MessageSender, ParsedChatEvent,
};
use quill::telegram::requests::ChatEventLogFilterSet;
impl QuillApp {
    /// Slice G2: apply the event-log search box value (the driver
    /// stores it per chat; an empty box clears it).
    pub(super) fn apply_event_log_search(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let query = self
            .event_log_search
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_string())
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            live.driver.set_chat_event_log_query(chat_id, &query);
            if live.driver.refresh_chat_event_log(chat_id).is_err() {
                self.status_note = "could not search recent actions".into();
            }
        }
        cx.notify();
    }

    /// Slice G2: flip one event-log filter category and refetch.
    pub(super) fn toggle_event_log_filter(
        &mut self,
        chat_id: ChatId,
        toggle: impl FnOnce(&mut ChatEventLogFilterSet) + 'static,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            live.driver.toggle_chat_event_log_filter(chat_id, toggle);
            if live.driver.refresh_chat_event_log(chat_id).is_err() {
                self.status_note = "could not filter recent actions".into();
            }
        }
        cx.notify();
    }

    pub(super) fn refresh_event_log(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.refresh_chat_event_log(chat_id) {
                Ok(_) => {}
                Err(_) => {
                    self.status_note = "could not refresh recent actions".into();
                }
            }
        } else {
            self.status_note = "recent actions need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3c: fetch the next older page of the event log.
    pub(super) fn load_more_event_log(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            match live.driver.fetch_chat_event_log_more(chat_id) {
                Ok(_) => {}
                Err(_) => {
                    self.status_note = "could not load more actions".into();
                }
            }
        } else {
            self.status_note = "recent actions need a live connection (demo)".into();
        }
        cx.notify();
    }

    /// Phase D3c: "Recent actions" admin-log section for the channel/group
    /// info panel. Shown only to administrators and the creator
    /// (`Session::chat_can_view_event_log` — `getChatEventLog` "requires
    /// administrator rights", schema 1.8.67 line 15252). Honest states:
    /// loading / failed-with-retry / loaded rows with actor name, action
    /// description, and relative timestamp, plus "Load more" while older
    /// pages exist. Unhandled event types render as generic rows — never
    /// faked details.
    pub(super) fn event_log_section(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
        if !self
            .session()
            .is_some_and(|session| session.chat_can_view_event_log(chat_id))
        {
            return div().into_any_element();
        }
        let fetch = self
            .session()
            .and_then(|session| session.event_logs.get(&chat_id.0))
            .cloned();
        let mut section = div()
            .id("event-log-section")
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
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Recent actions"),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("event-log-refresh")
                            .label("Refresh")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.refresh_event_log(chat_id, cx);
                            })),
                    ),
            );
        // Slice G2: compact search + filter controls. The search box
        // feeds `getChatEventLog.query` (schema 1.8.67, line 15252);
        // the chips flip `chatEventLogFilters` categories (line 7956).
        // An empty selection means "all types" (the driver passes
        // `null`), so "Clear" just removes every active chip.
        let mut controls = div().flex().items_center().w_full().gap_1();
        if let Some(input) = self.event_log_search.clone() {
            controls = controls.child(div().flex_1().child(Textarea::new(&input).h(px(32.))));
        }
        controls = controls.child(
            Button::new("event-log-search")
                .label("Search")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_event_log_search(chat_id, cx);
                })),
        );
        section = section.child(controls);
        let active_filters = self
            .session()
            .and_then(|session| session.event_log_filters.get(&chat_id.0).copied())
            .unwrap_or_default();
        section = section.child(self.event_log_filter_chips(chat_id, active_filters, cx));
        match fetch {
            None | Some(ChatEventLogFetch::Loading) => {
                section = section.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading recent actions…"),
                );
            }
            Some(ChatEventLogFetch::Failed(message)) => {
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
                            Button::new("event-log-retry")
                                .label("Retry")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.refresh_event_log(chat_id, cx);
                                })),
                        ),
                );
            }
            Some(ChatEventLogFetch::Loaded(page)) => {
                if page.events.is_empty() {
                    section = section.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No recent actions."),
                    );
                } else {
                    // Slice G2: per-admin filter chips (client-side).
                    let admins = page.admin_user_ids();
                    if admins.len() > 1 {
                        section = section.child(self.event_log_admin_chips(&admins, cx));
                    }
                    let admin_filter = self.event_log_admin_filter;
                    let visible = page.events.iter().filter(|event| match admin_filter {
                        None => true,
                        Some(user_id) => {
                            matches!(event.member_id, MessageSender::User { user_id: id } if id == user_id)
                        }
                    });
                    for event in visible {
                        section = section.child(self.event_log_row(event, cx));
                    }
                    if page.has_more {
                        section = section.child(
                            Button::new("event-log-load-more")
                                .label("Load more")
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.load_more_event_log(chat_id, cx);
                                })),
                        );
                    }
                }
            }
        }
        section.into_any_element()
    }

    /// Slice G2: per-admin filter chips for the event log. `None`
    /// (the "All admins" chip) shows every row; picking one admin filters
    /// the loaded page client-side.
    pub(super) fn event_log_admin_chips(
        &self,
        admins: &[i64],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.event_log_admin_filter;
        let mut chips = div()
            .id("event-log-admin-filters")
            .flex()
            .flex_wrap()
            .w_full()
            .gap_1();
        let all_label = if current.is_none() {
            "✓ All admins".to_string()
        } else {
            "All admins".to_string()
        };
        chips = chips.child(
            Button::new("event-log-admin-all")
                .label(all_label)
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.event_log_admin_filter = None;
                    cx.notify();
                })),
        );
        for user_id in admins.iter().copied() {
            let selected = current == Some(user_id);
            let name = self.group_call_participant_name(&MessageSender::User { user_id });
            let label = if selected {
                format!("✓ {name}")
            } else {
                name
            };
            chips = chips.child(
                Button::new(format!("event-log-admin-{user_id}"))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.event_log_admin_filter = Some(user_id);
                        cx.notify();
                    })),
            );
        }
        chips.into_any_element()
    }

    /// Slice G2: one toggle chip per `chatEventLogFilters` category
    /// (schema 1.8.67, line 7956). Active chips show a check mark; the
    /// "Clear" chip appears when any category is active (an empty
    /// selection means "all types" — the driver passes `null`).
    pub(super) fn event_log_filter_chips(
        &self,
        chat_id: ChatId,
        active: ChatEventLogFilterSet,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let categories: Vec<(&str, &str, bool, fn(&mut ChatEventLogFilterSet))> = vec![
            ("message-edits", "Edits", active.message_edits, |f| {
                f.message_edits = !f.message_edits
            }),
            (
                "message-deletions",
                "Deletions",
                active.message_deletions,
                |f| f.message_deletions = !f.message_deletions,
            ),
            ("message-pins", "Pins", active.message_pins, |f| {
                f.message_pins = !f.message_pins
            }),
            ("member-joins", "Joins", active.member_joins, |f| {
                f.member_joins = !f.member_joins
            }),
            ("member-leaves", "Leaves", active.member_leaves, |f| {
                f.member_leaves = !f.member_leaves
            }),
            ("member-invites", "Invites", active.member_invites, |f| {
                f.member_invites = !f.member_invites
            }),
            (
                "member-promotions",
                "Promotions",
                active.member_promotions,
                |f| f.member_promotions = !f.member_promotions,
            ),
            (
                "member-restrictions",
                "Restrictions",
                active.member_restrictions,
                |f| f.member_restrictions = !f.member_restrictions,
            ),
            (
                "member-tag-changes",
                "Tags",
                active.member_tag_changes,
                |f| f.member_tag_changes = !f.member_tag_changes,
            ),
            ("info-changes", "Info", active.info_changes, |f| {
                f.info_changes = !f.info_changes
            }),
            ("setting-changes", "Settings", active.setting_changes, |f| {
                f.setting_changes = !f.setting_changes
            }),
            (
                "invite-link-changes",
                "Invite links",
                active.invite_link_changes,
                |f| f.invite_link_changes = !f.invite_link_changes,
            ),
            (
                "video-chat-changes",
                "Video chats",
                active.video_chat_changes,
                |f| f.video_chat_changes = !f.video_chat_changes,
            ),
            ("forum-changes", "Forum", active.forum_changes, |f| {
                f.forum_changes = !f.forum_changes
            }),
            (
                "subscription-extensions",
                "Stars",
                active.subscription_extensions,
                |f| f.subscription_extensions = !f.subscription_extensions,
            ),
        ];
        let mut chips = div()
            .id("event-log-filters")
            .flex()
            .flex_wrap()
            .w_full()
            .gap_1();
        for (id, label, enabled, toggle) in categories {
            let label = if enabled {
                format!("✓ {label}")
            } else {
                label.to_string()
            };
            chips = chips.child(
                Button::new(format!("event-log-filter-{id}"))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_event_log_filter(chat_id, toggle, cx);
                    })),
            );
        }
        if active.any_enabled() {
            chips = chips.child(
                Button::new("event-log-filter-clear")
                    .label("Clear")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.clear_event_log_filters(chat_id, cx);
                    })),
            );
        }
        chips.into_any_element()
    }

    /// Slice G2: clear every event-log filter category for the chat and
    /// refetch (empty selection = all types).
    pub(super) fn clear_event_log_filters(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.event_log_filters.remove(&chat_id.0);
            if live.driver.refresh_chat_event_log(chat_id).is_err() {
                self.status_note = "could not clear filters".into();
            }
        }
        cx.notify();
    }

    /// Phase D3c: one admin-log row — actor name, action description, and
    /// relative timestamp.
    pub(super) fn event_log_row(
        &self,
        event: &ParsedChatEvent,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let actor = self.group_call_participant_name(&event.member_id);
        let description = self.event_log_action_description(event);
        div()
            .id(("event-log-row", event.id as u64))
            .flex()
            .w_full()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_1()
                    .child(div().text_sm().child(actor))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(description),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(event_log_relative_time(event.date)),
            )
            .into_any_element()
    }

    /// Phase D3c: human description of an admin-log action. Every string
    /// is built only from parsed fields; unhandled constructors render
    /// the honest generic "performed an action".
    pub(super) fn event_log_action_description(&self, event: &ParsedChatEvent) -> String {
        let user_name = |user_id: i64| {
            self.session()
                .and_then(|session| session.user(user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| format!("User {user_id}"))
        };
        let sender_name = |sender: &MessageSender| match sender {
            MessageSender::User { user_id } => user_name(*user_id),
            MessageSender::Chat { chat_id } => self
                .session()
                .and_then(|session| session.chats.get(chat_id))
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| format!("Chat {chat_id}")),
        };
        // Short quoted excerpt; long titles/descriptions are cut rather
        // than wrapping the row.
        let quote = |text: &str| {
            let mut short: String = text.chars().take(60).collect();
            if text.chars().count() > 60 {
                short.push('…');
            }
            format!("\"{short}\"")
        };
        let with_text = |verb: &str, text: &str| {
            if text.is_empty() {
                verb.to_owned()
            } else {
                format!("{verb} {}", quote(text))
            }
        };
        match &event.action {
            ChatEventAction::MessageEdited { text, .. } => with_text("edited a message", text),
            ChatEventAction::MessageDeleted { text, .. } => with_text("deleted a message", text),
            ChatEventAction::MessagePinned { text, .. } => with_text("pinned a message", text),
            ChatEventAction::MessageUnpinned { text, .. } => with_text("unpinned a message", text),
            ChatEventAction::MemberJoined => "joined the chat".to_owned(),
            // B4: `chatEventPollStopped` (schema 1.8.67 line 7776) — TGX
            // `EventLogPollStopped` / `EventLogQuizStopped` copy.
            ChatEventAction::PollStopped { is_quiz } => {
                if *is_quiz {
                    "stopped the quiz".to_owned()
                } else {
                    "stopped the poll".to_owned()
                }
            }
            ChatEventAction::MemberJoinedByInviteLink {
                invite_link_name, ..
            } => {
                if invite_link_name.is_empty() {
                    "joined via an invite link".to_owned()
                } else {
                    format!("joined via invite link {}", quote(invite_link_name))
                }
            }
            ChatEventAction::MemberJoinedByRequest {
                approver_user_id, ..
            } => {
                if *approver_user_id == 0 {
                    "joined via an approved join request".to_owned()
                } else {
                    format!("join request approved by {}", user_name(*approver_user_id))
                }
            }
            ChatEventAction::MemberInvited { user_id, .. } => {
                format!("invited {}", user_name(*user_id))
            }
            ChatEventAction::MemberPromoted {
                user_id,
                old_status,
                new_status,
            } => {
                let name = user_name(*user_id);
                match (old_status, new_status) {
                    (_, ChannelMemberStatus::Creator) => {
                        format!("transferred ownership to {name}")
                    }
                    (_, ChannelMemberStatus::Administrator) => {
                        format!("promoted {name} to administrator")
                    }
                    (ChannelMemberStatus::Administrator, ChannelMemberStatus::Member) => {
                        format!("removed {name} as administrator")
                    }
                    _ => format!("changed {name}'s role"),
                }
            }
            ChatEventAction::MemberRestricted {
                member_id,
                old_status,
                new_status,
            } => {
                let name = sender_name(member_id);
                match (old_status, new_status) {
                    (_, ChannelMemberStatus::Banned) => format!("banned {name}"),
                    (_, ChannelMemberStatus::Restricted) => format!("restricted {name}"),
                    (ChannelMemberStatus::Banned, _) => format!("unbanned {name}"),
                    (ChannelMemberStatus::Restricted, _) => {
                        format!("lifted restrictions on {name}")
                    }
                    _ => format!("changed {name}'s restrictions"),
                }
            }
            ChatEventAction::DescriptionChanged {
                new_description, ..
            } => {
                if new_description.is_empty() {
                    "cleared the description".to_owned()
                } else {
                    format!("changed the description to {}", quote(new_description))
                }
            }
            ChatEventAction::PhotoChanged => "changed the chat photo".to_owned(),
            ChatEventAction::TitleChanged { new_title, .. } => {
                format!("changed the title to {}", quote(new_title))
            }
            ChatEventAction::InviteLinkEdited {
                old_name, new_name, ..
            } => {
                let name = if new_name.is_empty() {
                    old_name
                } else {
                    new_name
                };
                if name.is_empty() {
                    "edited an invite link".to_owned()
                } else {
                    format!("edited invite link {}", quote(name))
                }
            }
            ChatEventAction::InviteLinkRevoked { name, .. } => {
                if name.is_empty() {
                    "revoked an invite link".to_owned()
                } else {
                    format!("revoked invite link {}", quote(name))
                }
            }
            ChatEventAction::InviteLinkDeleted { name, .. } => {
                if name.is_empty() {
                    "deleted an invite link".to_owned()
                } else {
                    format!("deleted invite link {}", quote(name))
                }
            }
            ChatEventAction::Unsupported { .. } => "performed an action".to_owned(),
        }
    }
}
