//! Methods moved out of `group_admin_panels.rs` to keep files under 1000 lines.

use super::*;

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
                        let details_link = link.invite_link.clone();
                        let rename_link = link.invite_link.clone();
                        let rename_name = link.name.clone();
                        let joined_count = link.member_count;
                        let subscription = link.subscription_pricing.clone();
                        let members_block =
                            self.invite_link_members_block(chat_id, &link.invite_link, cx);
                        let requests_block =
                            self.link_requests_block(chat_id, &link.invite_link, cx);
                        let qr_block = self.invite_link_qr_block(chat_id, &link.invite_link, cx);
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
                                        .label("Copy")
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.copy_invite_link(&copy_link, cx);
                                        })),
                                )
                                .when(joined_count > 0, |actions| {
                                    actions.child(
                                        Button::new(format!("invite-link-joined-{index}"))
                                            .label(format!("Joined ({joined_count})"))
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
                                        .label("QR code")
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_invite_link_qr(chat_id, &qr_link, cx);
                                        })),
                                )
                                .when(subscription.is_some(), |actions| {
                                    actions.child(
                                        Button::new(format!("invite-link-rename-{index}"))
                                            .label("Rename")
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
                                        .label("Revoke")
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
                        if let Some(block) = qr_block {
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
