//! Methods moved out of `group_admin_panels.rs` to keep files under 1000 lines.

use super::*;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Phase D3a: invite-link management section for the channel/group
    /// info panel. Shown only to admins who may manage links
    /// (`ChatSummary::can_invite_users`). Honest states: loading /
    /// failed-with-retry / loaded list. Each row shows the link name (or
    /// "Primary link" / "Invite link" fallback), uses, expiry, a
    /// join-request badge when `pending_join_request_count` > 0, and
    /// Copy + Revoke buttons. Revoking is the delete path (TDLib 1.8.67
    /// has no `deleteChatInviteLink`).
    pub(in crate::ui) fn invite_links_section(
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
            .and_then(|session| session.groups.invite_links.get(&chat_id.0))
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
                        .child(tr(keys::ADMIN_LOG_FILTER_INVITE_LINKS)),
                )
                .child(div().flex_1())
                .child(
                    Button::new("invite-links-refresh")
                        .icon(gpui_kit::assets::IconName::RotateCcw)
                        .ghost()
                        .xsmall()
                        .tooltip(tr(keys::Q_PEOPLE_REFRESH))
                        .accessibility_label(tr(keys::Q_PEOPLE_REFRESH))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.refresh_invite_links(chat_id, cx);
                        })),
                )
                .child(
                    Button::new("invite-link-create")
                        .icon(gpui_kit::assets::IconName::Plus)
                        .xsmall()
                        .tooltip(tr(keys::Q_PEOPLE_CREATE_INVITE_LINK))
                        .accessibility_label(tr(keys::Q_PEOPLE_CREATE_INVITE_LINK))
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
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(gpui_kit::component::spinner::Spinner::new().small())
                        .child(tr(keys::Q_PEOPLE_LOADING_INVITE_LINKS)),
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
                                .label(tr(keys::Q_PEOPLE_RETRY))
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
                            .child(tr(keys::Q_PEOPLE_NO_INVITE_LINKS)),
                    );
                } else {
                    for (index, link) in list.links.into_iter().enumerate() {
                        let name = if !link.name.is_empty() {
                            link.name.clone()
                        } else if link.is_primary {
                            tr(keys::Q_PEOPLE_PRIMARY_LINK).into()
                        } else {
                            tr(keys::CREATE_GROUP_INVITE_LINK).into()
                        };
                        let uses = if link.member_limit > 0 {
                            tr_with(
                                keys::Q_PEOPLE_USES,
                                &[
                                    ("member_count", &link.member_count.to_string()),
                                    ("member_limit", &link.member_limit.to_string()),
                                ],
                            )
                        } else {
                            format!("{} uses", link.member_count)
                        };
                        let expiry = Self::invite_link_expiry(link.expiration_date);
                        let copy_link = link.invite_link.clone();
                        let revoke_link = link.invite_link.clone();
                        let details_link = link.invite_link.clone();
                        let rename_link = link.invite_link.clone();
                        let rename_name = link.name.clone();
                        let joined_count = link.member_count;
                        let subscription = link.subscription_pricing.clone();
                        let members_block =
                            self.invite_link_members_block(chat_id, &link.invite_link, cx);
                        let requests_block =
                            self.link_requests_block(chat_id, &link.invite_link, cx);
                        let requests_link = link.invite_link.clone();
                        let qr_link = link.invite_link.clone();
                        let pending_requests = link.pending_join_request_count;
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
                        if let Some(pricing) = &subscription {
                            row = row.child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(super::invite_admin_ui::subscription_price_label(
                                        pricing,
                                    )),
                            );
                        }
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
                                .flex_wrap()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new(format!("invite-link-copy-{index}"))
                                        .label(tr(keys::GROUP_INVITE_CONTEXT_COPY))
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.copy_invite_link(&copy_link, cx);
                                        })),
                                )
                                .when(joined_count > 0, |actions| {
                                    actions.child(
                                        Button::new(format!("invite-link-joined-{index}"))
                                            .label(tr_with(
                                                keys::Q_PEOPLE_JOINED,
                                                &[("joined_count", &joined_count.to_string())],
                                            ))
                                            .ghost()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.toggle_invite_link_details(
                                                    chat_id,
                                                    &details_link,
                                                    cx,
                                                );
                                            })),
                                    )
                                })
                                .when(pending_requests > 0, |actions| {
                                    actions.child(
                                        Button::new(format!("invite-link-requests-{index}"))
                                            .label(
                                                super::invite_admin_more_ui::requests_button_label(
                                                    pending_requests,
                                                ),
                                            )
                                            .ghost()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.toggle_link_requests(
                                                    chat_id,
                                                    &requests_link,
                                                    cx,
                                                );
                                            })),
                                    )
                                })
                                .child(
                                    Button::new(format!("invite-link-qr-{index}"))
                                        .label(tr(keys::GROUP_INVITE_CONTEXT_QR))
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.open_invite_link_qr(&qr_link, cx);
                                        })),
                                )
                                .when(subscription.is_some(), |actions| {
                                    actions.child(
                                        Button::new(format!("invite-link-rename-{index}"))
                                            .label(tr(keys::Q_PEOPLE_RENAME))
                                            .ghost()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.open_subscription_link_rename(
                                                    chat_id,
                                                    &rename_link,
                                                    &rename_name,
                                                    window,
                                                    cx,
                                                );
                                            })),
                                    )
                                })
                                .child(
                                    Button::new(format!("invite-link-revoke-{index}"))
                                        .label(tr(keys::GROUP_INVITE_CONTEXT_REVOKE))
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.revoke_invite_link(chat_id, &revoke_link, cx);
                                        })),
                                ),
                        );
                        if let Some(block) = members_block {
                            row = row.child(block);
                        }
                        if let Some(block) = requests_block {
                            row = row.child(block);
                        }
                        section = section.child(row);
                    }
                }
            }
        }
        if let Some(counts) = self.invite_link_counts_block(chat_id, cx) {
            section = section.child(counts);
        }
        section = section.child(self.revoked_invite_links_block(chat_id, cx));
        section.into_any_element()
    }

    /// Phase D3a: join-request section. Approve/Decline per request;
    /// shows the requester's name (falls back to "User <id>") and bio.
    pub(in crate::ui) fn join_requests_section(
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
            .and_then(|session| session.groups.join_requests.get(&chat_id.0))
            .cloned();
        let pending_count = match &fetch {
            Some(JoinRequestFetch::Loaded(list)) => list.total_count,
            _ => self
                .session()
                .and_then(|session| {
                    session
                        .groups
                        .pending_join_request_counts
                        .get(&chat_id.0)
                        .copied()
                })
                .unwrap_or(0),
        };
        let mut header = div().flex().items_center().w_full().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_PEOPLE_JOIN_REQUESTS)),
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
                .tooltip(tr(keys::Q_PEOPLE_REFRESH))
                .accessibility_label(tr(keys::Q_PEOPLE_REFRESH))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.refresh_join_requests(chat_id, cx);
                })),
        );
        let mut section = div().flex().flex_col().w_full().gap_1().child(header);
        match fetch {
            None | Some(JoinRequestFetch::Loading) => {
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(gpui_kit::component::spinner::Spinner::new().small())
                        .child(tr(keys::Q_PEOPLE_LOADING_JOIN_REQUESTS)),
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
                                .label(tr(keys::Q_PEOPLE_RETRY))
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
                            .child(tr(keys::Q_PEOPLE_NO_PENDING_JOIN_REQUESTS)),
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
                            .unwrap_or_else(|| {
                                tr_with(keys::Q_PEOPLE_USER, &[("user_id", &user_id.to_string())])
                            });
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
                                        .label(tr(keys::Q_PEOPLE_APPROVE))
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.process_join_request(chat_id, user_id, true, cx);
                                        })),
                                )
                                .child(
                                    Button::new(format!("join-request-decline-{user_id}"))
                                        .label(tr(keys::Q_PEOPLE_DECLINE))
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
    pub(in crate::ui) fn administrators_section(
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
            .and_then(|session| session.groups.admin_lists.get(&chat_id.0))
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
                        .child(tr(keys::Q_PEOPLE_ADMINISTRATORS)),
                )
                .child(div().flex_1())
                .child(
                    Button::new("admin-list-refresh")
                        .icon(gpui_kit::assets::IconName::RotateCcw)
                        .ghost()
                        .xsmall()
                        .tooltip(tr(keys::Q_PEOPLE_REFRESH))
                        .accessibility_label(tr(keys::Q_PEOPLE_REFRESH))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.refresh_administrators(chat_id, cx);
                        })),
                )
                .child(
                    Button::new("admin-promote-open")
                        .icon(gpui_kit::assets::IconName::Plus)
                        .xsmall()
                        .tooltip(tr(keys::CHANNEL_ADD_ADMIN))
                        .accessibility_label(tr(keys::CHANNEL_ADD_ADMIN))
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
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(gpui_kit::component::spinner::Spinner::new().small())
                        .child(tr(keys::Q_PEOPLE_LOADING_ADMINISTRATORS)),
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
                                .label(tr(keys::Q_PEOPLE_RETRY))
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
                            .child(tr(keys::Q_PEOPLE_NO_ADMINISTRATORS)),
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
    pub(in crate::ui) fn admin_row(
        &self,
        chat_id: ChatId,
        entry: &ChatAdministratorEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .session()
            .and_then(|session| session.user(entry.user_id))
            .map(|user| user.display_name())
            .unwrap_or_else(|| {
                tr_with(
                    keys::Q_PEOPLE_USER,
                    &[("user_id", &entry.user_id.to_string())],
                )
            });
        let role = if entry.is_owner {
            tr(keys::Q_PEOPLE_OWNER).to_string()
        } else if entry.custom_title.is_empty() {
            tr(keys::Q_PEOPLE_ADMIN).to_string()
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
                        .label(tr(keys::GROUP_INVITE_CONTEXT_EDIT))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_rights_editor(chat_id, user_id, cx);
                        })),
                )
                .child(
                    Button::new(format!("admin-demote-{user_id}"))
                        .label(tr(keys::BOX_REMOVE))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_demote_confirm(chat_id, user_id, cx);
                        })),
                );
        }
        row.into_any_element()
    }
}
