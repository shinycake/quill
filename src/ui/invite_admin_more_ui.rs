//! Invite-link admin, part two: another admin's links, one link's pending
//! join requests, and the link QR code (tdesktop `edit_peer_invite_links.cpp`
//! "Links created by other admins", `edit_peer_invite_link.cpp` requests
//! list and QR box).

use super::app::QuillApp;
use super::auth_ui::render_qr_image;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::local_time::{civil_local, day_label, now_unix};
use quill::state::{InviteLinkFetch, InviteLinkList};

/// "requested to join October 6" for a pending request row.
pub(crate) fn link_request_label(date: i32) -> String {
    if date <= 0 {
        return "requested to join".into();
    }
    let at = civil_local(i64::from(date));
    let now = civil_local(now_unix());
    format!("requested to join {}", day_label(&at, &now))
}

/// "3 pending" style count for the Requests button.
pub(crate) fn requests_button_label(count: i32) -> String {
    format!("Requests ({count})")
}

/// Uses line of a link row: "4 uses" or "4/10 uses".
pub(crate) fn link_uses_label(member_count: i32, member_limit: i32) -> String {
    if member_limit > 0 {
        format!("{member_count}/{member_limit} uses")
    } else {
        format!("{member_count} uses")
    }
}

impl QuillApp {
    pub(super) fn open_admin_links(&mut self, chat_id: ChatId, admin: i64, cx: &mut Context<Self>) {
        self.connection.status_note = match self.live.as_mut() {
            Some(live) => match live.driver.open_admin_invite_links(chat_id, admin) {
                Ok(_) => String::new(),
                Err(_) => "could not load invite links".into(),
            },
            None => "invite links need a live connection (demo)".into(),
        };
        cx.notify();
    }

    fn close_admin_links(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_admin_invite_links(chat_id);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.admin_invite_links.remove(&chat_id.0);
        }
        cx.notify();
    }

    fn confirm_delete_admin_revoked(
        &mut self,
        chat_id: ChatId,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            alert
                .title(format!("Delete revoked links by {name}?"))
                .description("This can't be undone.")
                .ok_text("Delete")
                .cancel_text("Cancel")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| {
                        this.connection.status_note = match this.live.as_mut() {
                            Some(live) => match live.driver.delete_all_revoked_admin_links(chat_id)
                            {
                                Ok(_) => "deleting revoked links…".into(),
                                Err(_) => "could not delete revoked links".into(),
                            },
                            None => "invite links need a live connection (demo)".into(),
                        };
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn admin_link_rows(
        &self,
        id: &'static str,
        list: &InviteLinkList,
        empty: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        let mut rows = div().flex().flex_col().w_full().gap_1();
        if list.links.is_empty() {
            return rows.child(div().text_xs().text_color(muted).child(empty));
        }
        for (index, link) in list.links.iter().enumerate() {
            let copy = link.invite_link.clone();
            let name = if link.name.is_empty() {
                link.invite_link.clone()
            } else {
                link.name.clone()
            };
            rows = rows.child(
                div()
                    .id((id, index as u64))
                    .flex()
                    .items_center()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().truncate().child(name))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(link_uses_label(link.member_count, link.member_limit)),
                            ),
                    )
                    .child(
                        Button::new((id, 1000 + index as u64))
                            .label("Copy")
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy_invite_link(&copy, cx);
                            })),
                    ),
            );
        }
        rows
    }

    /// Another admin's active and revoked links, shown in place of the
    /// per-admin counts while open.
    pub(super) fn admin_links_block(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.session()?.admin_invite_links.get(&chat_id.0)?.clone();
        let admin = state.creator_user_id;
        let name = self.contact_display_name(admin);
        let muted = cx.theme().muted_foreground;
        let mut block = div().flex().flex_col().w_full().gap_1().pt_1().child(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap_1()
                .child(
                    Button::new("admin-links-back")
                        .icon(gpui_kit::assets::IconName::ChevronLeft)
                        .ghost()
                        .xsmall()
                        .tooltip("Back")
                        .accessibility_label("Back")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_admin_links(chat_id, cx);
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(format!("Links by {name}")),
                ),
        );
        block = match &state.active {
            InviteLinkFetch::Loading => {
                block.child(div().text_xs().text_color(muted).child("Loading…"))
            }
            InviteLinkFetch::Failed(message) => {
                block.child(div().text_xs().text_color(muted).child(message.clone()))
            }
            InviteLinkFetch::Loaded(list) => {
                block.child(self.admin_link_rows("admin-link", list, "No active links.", cx))
            }
        };
        if let InviteLinkFetch::Loaded(list) = &state.revoked
            && !list.links.is_empty()
        {
            block = block
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w_full()
                        .gap_1()
                        .pt_1()
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .text_color(muted)
                                .child("Revoked links"),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("admin-links-delete-revoked")
                                .label("Delete all")
                                .ghost()
                                .small()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let name = this.contact_display_name(admin);
                                    this.confirm_delete_admin_revoked(chat_id, name, window, cx);
                                })),
                        ),
                )
                .child(self.admin_link_rows("admin-revoked-link", list, "", cx));
        }
        Some(block.into_any_element())
    }

    /// Open or close the pending-request list of one link.
    pub(super) fn toggle_link_requests(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .session()
            .and_then(|s| s.link_join_requests.get(&chat_id.0))
            .is_some_and(|state| state.invite_link == invite_link);
        if let Some(live) = self.live.as_mut() {
            if open {
                live.driver.close_link_join_requests(chat_id);
            } else {
                let _ = live.driver.open_link_join_requests(chat_id, invite_link);
            }
        } else if open && let Some(session) = self.demo_session.as_mut() {
            session.link_join_requests.remove(&chat_id.0);
        }
        cx.notify();
    }

    fn process_link_requests(&mut self, chat_id: ChatId, approve: bool, cx: &mut Context<Self>) {
        self.connection.status_note = match self.live.as_mut() {
            Some(live) => match live.driver.process_link_join_requests(chat_id, approve) {
                Ok(_) => String::new(),
                Err(_) => "could not process join requests".into(),
            },
            None => "join requests need a live connection (demo)".into(),
        };
        cx.notify();
    }

    fn load_more_link_requests(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_link_join_requests(chat_id);
        }
        cx.notify();
    }

    /// The pending requests of `invite_link`, under its row while open.
    pub(super) fn link_requests_block(
        &self,
        chat_id: ChatId,
        invite_link: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self
            .session()?
            .link_join_requests
            .get(&chat_id.0)
            .filter(|state| state.invite_link == invite_link)?
            .clone();
        let muted = cx.theme().muted_foreground;
        let mut block = div()
            .id(format!("link-requests-{}", invite_link.len()))
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .pl_2();
        if state.requests.is_empty() && state.loading {
            return Some(
                block
                    .child(div().text_xs().text_color(muted).child("Loading…"))
                    .into_any_element(),
            );
        }
        if state.requests.is_empty() {
            return Some(
                block
                    .child(
                        div().text_xs().text_color(muted).child(
                            state
                                .error
                                .clone()
                                .unwrap_or_else(|| "No pending requests".into()),
                        ),
                    )
                    .into_any_element(),
            );
        }
        block = block.child(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(format!("{} pending", state.total_count)),
                )
                .child(
                    Button::new("link-requests-approve-all")
                        .label("Add all")
                        .ghost()
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.process_link_requests(chat_id, true, cx);
                        })),
                )
                .child(
                    Button::new("link-requests-dismiss-all")
                        .label("Dismiss all")
                        .ghost()
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.process_link_requests(chat_id, false, cx);
                        })),
                ),
        );
        for request in &state.requests {
            let uid = request.user_id;
            block = block.child(
                div()
                    .id(("link-request", uid as u64))
                    .flex()
                    .items_center()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .truncate()
                                    .child(self.contact_display_name(uid)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(link_request_label(request.date)),
                            ),
                    )
                    .child(
                        Button::new(("link-request-add", uid as u64))
                            .label("Add")
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.process_join_request(chat_id, uid, true, cx);
                            })),
                    )
                    .child(
                        Button::new(("link-request-dismiss", uid as u64))
                            .label("Dismiss")
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.process_join_request(chat_id, uid, false, cx);
                            })),
                    ),
            );
        }
        if let Some(error) = state.error.clone() {
            block = block.child(div().text_xs().text_color(muted).child(error));
        }
        if state.loading {
            block = block.child(div().text_xs().text_color(muted).child("Loading…"));
        } else if (state.requests.len() as i32) < state.total_count {
            block = block.child(
                Button::new("link-requests-more")
                    .label("Show more")
                    .ghost()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.load_more_link_requests(chat_id, cx);
                    })),
            );
        }
        Some(block.into_any_element())
    }

    /// Show or hide the QR code of one link.
    pub(super) fn toggle_invite_link_qr(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .admin
            .invite_link_qr
            .as_ref()
            .is_some_and(|(chat, link, _)| *chat == chat_id && link == invite_link);
        self.admin.invite_link_qr = if open {
            None
        } else {
            render_qr_image(invite_link).map(|image| (chat_id, invite_link.to_owned(), image))
        };
        cx.notify();
    }

    /// The QR code under a link's row while it is open.
    pub(super) fn invite_link_qr_block(
        &self,
        chat_id: ChatId,
        invite_link: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (_, _, image) = self
            .admin
            .invite_link_qr
            .as_ref()
            .filter(|(chat, link, _)| *chat == chat_id && link == invite_link)?;
        Some(
            div()
                .id(format!("invite-link-qr-{}", invite_link.len()))
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(
                    div().p_2().rounded_lg().bg(gpui_kit::white()).child(
                        img(ImageSource::from(image.clone()))
                            .size(px(176.))
                            .aspect_square()
                            .object_fit(ObjectFit::Contain),
                    ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Scan to join"),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{link_request_label, link_uses_label, requests_button_label};

    #[test]
    fn labels_read_like_telegram() {
        assert_eq!(link_request_label(0), "requested to join");
        assert!(link_request_label(1_700_000_000).starts_with("requested to join "));
        assert_eq!(requests_button_label(3), "Requests (3)");
        assert_eq!(link_uses_label(4, 0), "4 uses");
        assert_eq!(link_uses_label(4, 10), "4/10 uses");
    }
}
