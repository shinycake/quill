//! The Recent actions filter box (tdesktop `history_admin_log_filter.cpp`
//! and `InnerWidget::showFilter`): "All actions" over a box per category in
//! three sections, "Show Actions by All Admins" over a box per admin with
//! their photo, and Cancel / Save. The choices stay in a draft until Save.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialogs::group_admin_settings::GroupSettingsView;
use super::message_text::kit_avatar_element;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::event_log_filter::{FilterCategory, FilterDraft, FilterSection};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::telegram::envelope::MessageSender;

/// Side of an administrator's photo in the filter.
const ADMIN_AVATAR: f32 = 28.;

/// The open filter box: the choices so far and the administrators offered.
pub(super) struct EventLogFilterBox {
    draft: FilterDraft,
    admins: Vec<i64>,
    is_channel: bool,
}

fn category_label(category: FilterCategory, is_channel: bool) -> &'static str {
    match category {
        FilterCategory::AdminRights => tr(keys::ADMIN_LOG_FILTER_ADMINS_NEW),
        FilterCategory::TagChanges => tr(keys::ADMIN_LOG_FILTER_EDIT_RANK),
        FilterCategory::Restrictions => tr(keys::ADMIN_LOG_FILTER_RESTRICTIONS),
        FilterCategory::NewMembers if is_channel => tr(keys::ADMIN_LOG_FILTER_SUBSCRIBERS_NEW),
        FilterCategory::NewMembers => tr(keys::ADMIN_LOG_FILTER_MEMBERS_NEW),
        FilterCategory::MembersLeaving if is_channel => {
            tr(keys::ADMIN_LOG_FILTER_SUBSCRIBERS_REMOVED)
        }
        FilterCategory::MembersLeaving => tr(keys::ADMIN_LOG_FILTER_MEMBERS_REMOVED),
        FilterCategory::Info if is_channel => tr(keys::ADMIN_LOG_FILTER_INFO_CHANNEL),
        FilterCategory::Info => tr(keys::ADMIN_LOG_FILTER_INFO_GROUP),
        FilterCategory::InviteLinks => tr(keys::ADMIN_LOG_FILTER_INVITE_LINKS),
        FilterCategory::VideoChats if is_channel => tr(keys::ADMIN_LOG_FILTER_VOICE_CHATS_CHANNEL),
        FilterCategory::VideoChats => tr(keys::ADMIN_LOG_FILTER_VOICE_CHATS),
        FilterCategory::SubscriptionRenewals => tr(keys::ADMIN_LOG_FILTER_SUB_EXTEND),
        FilterCategory::Topics => tr(keys::ADMIN_LOG_FILTER_TOPICS),
        FilterCategory::Deleted => tr(keys::ADMIN_LOG_FILTER_MESSAGES_DELETED),
        FilterCategory::Edited => tr(keys::ADMIN_LOG_FILTER_MESSAGES_EDITED),
        FilterCategory::Pinned => tr(keys::ADMIN_LOG_FILTER_MESSAGES_PINNED),
    }
}

fn section_label(section: FilterSection, is_channel: bool) -> &'static str {
    match (section, is_channel) {
        (FilterSection::Members, false) => tr(keys::ADMIN_LOG_FILTER_ACTIONS_MEMBER_SECTION),
        (FilterSection::Members, true) => tr(keys::ADMIN_LOG_FILTER_ACTIONS_SUBSCRIBER_SECTION),
        (FilterSection::Settings, false) => tr(keys::ADMIN_LOG_FILTER_ACTIONS_SETTINGS_SECTION),
        (FilterSection::Settings, true) => {
            tr(keys::ADMIN_LOG_FILTER_ACTIONS_CHANNEL_SETTINGS_SECTION)
        }
        (FilterSection::Messages, _) => tr(keys::ADMIN_LOG_FILTER_ACTIONS_MESSAGES_SECTION),
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
impl EventLogFilterBox {
    pub(super) fn draft(&self) -> &FilterDraft {
        &self.draft
    }
}

impl QuillApp {
    /// The Filter button: open the box over what the chat filters by now.
    pub(super) fn open_event_log_filter(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let stored = session.groups.event_log_filters.get(&chat_id.0).copied();
        let stored_admins = session
            .groups
            .event_log_users
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        let fetch = session.groups.event_logs.get(&chat_id.0).cloned();
        let is_channel = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_channel());
        let admins = self.event_log_admin_ids(chat_id, fetch.as_ref());
        self.admin.event_log_filter = Some(EventLogFilterBox {
            draft: FilterDraft::open(stored, &stored_admins, &admins, is_channel),
            admins,
            is_channel,
        });
        self.set_group_settings_view(GroupSettingsView::RecentActionsFilter, cx);
    }

    /// Cancel: back to the list, nothing changed.
    pub(super) fn close_event_log_filter(&mut self, cx: &mut Context<Self>) {
        self.admin.event_log_filter = None;
        self.set_group_settings_view(GroupSettingsView::RecentActions, cx);
    }

    /// Save: store both lists and load the log again.
    pub(super) fn save_event_log_filter(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(open) = self.admin.event_log_filter.take() else {
            return;
        };
        let choice = open.draft.choice(&open.admins);
        if let Some(live) = self.live.as_mut() {
            live.driver
                .set_chat_event_log_filter(chat_id, choice.categories, choice.admins);
        }
        self.refresh_event_log(chat_id, cx);
        self.set_group_settings_view(GroupSettingsView::RecentActions, cx);
    }

    pub(super) fn edit_event_log_filter(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut FilterDraft, &[i64]),
    ) {
        if let Some(open) = self.admin.event_log_filter.as_mut() {
            edit(&mut open.draft, &open.admins);
        }
        cx.notify();
    }

    /// Cancel and Save for the filter box's footer.
    pub(super) fn event_log_filter_footer(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_save = self
            .admin
            .event_log_filter
            .as_ref()
            .is_some_and(|open| open.draft.can_save());
        div()
            .flex()
            .gap_2()
            .child(
                Button::new("event-log-filter-save")
                    .label(tr(keys::SETTINGS_SAVE))
                    .primary()
                    .disabled(!can_save)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.save_event_log_filter(chat_id, cx);
                    })),
            )
            .child(
                Button::new("event-log-filter-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_event_log_filter(cx))),
            )
            .into_any_element()
    }

    pub(super) fn event_log_filter_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div()
            .id("event-log-filter")
            .flex()
            .flex_col()
            .w_full()
            .gap_2();
        let Some(open) = self.admin.event_log_filter.as_ref() else {
            return body.into_any_element();
        };
        let theme = cx.theme();
        let subtitle = |text: &'static str| {
            div()
                .pt_1()
                .text_sm()
                .font_semibold()
                .text_color(theme.primary)
                .child(text)
        };
        let is_channel = open.is_channel;
        body = body.child(subtitle(tr(keys::ADMIN_LOG_FILTER_ACTIONS_TYPE_SUBTITLE)));
        body = body.child(
            Checkbox::new("event-log-filter-all-actions")
                .label(tr(keys::ADMIN_LOG_FILTER_ALL_ACTIONS))
                .checked(open.draft.all_actions())
                .on_click(cx.listener(|this, &on, _, cx| {
                    this.edit_event_log_filter(cx, |draft, _| draft.set_all_actions(on));
                })),
        );
        for (section, categories) in FilterCategory::sections(is_channel) {
            let mut group = div().flex().flex_col().gap_2().pl_6();
            group = group.child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(theme.muted_foreground)
                    .child(section_label(section, is_channel)),
            );
            for category in categories {
                group = group.child(
                    Checkbox::new(("event-log-filter-category", category as u64))
                        .label(category_label(category, is_channel))
                        .checked(open.draft.category(category))
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            this.edit_event_log_filter(cx, |draft, _| {
                                draft.toggle_category(category, on);
                            });
                        })),
                );
            }
            body = body.child(group);
        }
        body = body
            .child(div().h_px().w_full().bg(theme.border))
            .child(subtitle(tr(keys::ADMIN_LOG_FILTER_ACTIONS_ADMINS_SUBTITLE)))
            .child(
                Checkbox::new("event-log-filter-all-admins")
                    .label(tr(keys::ADMIN_LOG_FILTER_ACTIONS_ADMINS_SECTION))
                    .checked(open.draft.all_admins(&open.admins))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.edit_event_log_filter(cx, |draft, admins| {
                            draft.set_all_admins(on, admins);
                        });
                    })),
            );
        let mut admins = div().flex().flex_col().gap_2().pl_6();
        for user_id in open.admins.iter().copied() {
            let name = self.group_call_participant_name(&MessageSender::User { user_id });
            let photo = self
                .session()
                .and_then(|session| session.user_photo_path(user_id))
                .map(std::path::PathBuf::from);
            admins = admins.child(
                Checkbox::new(("event-log-filter-admin", user_id as u64))
                    .checked(open.draft.admin(user_id))
                    .items_center()
                    .accessibility_label(name.clone())
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(kit_avatar_element(
                                &name,
                                photo.as_deref(),
                                px(ADMIN_AVATAR),
                            ))
                            .child(super::bidi_line::one_line_plain(name)),
                    )
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        this.edit_event_log_filter(cx, |draft, _| draft.toggle_admin(user_id, on));
                    })),
            );
        }
        body.child(admins).into_any_element()
    }
}
