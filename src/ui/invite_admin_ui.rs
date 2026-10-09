//! B8: invite-link admin blocks under the info panel's "Invite links"
//! section — who joined through a link, other admins' link counts, and
//! the revoked-links list with delete (tdesktop `edit_peer_invite_links.cpp`
//! and `edit_peer_invite_link.cpp`).

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::local_time::{civil_local, day_label, now_unix};
use quill::state::{InviteLinkCountsFetch, InviteLinkFetch};
use quill::telegram::envelope::StarSubscriptionPricing;

/// "250 Stars / month" for a subscription link's pricing.
pub(crate) fn subscription_price_label(pricing: &StarSubscriptionPricing) -> String {
    format!("{} Stars / month", pricing.star_count)
}

/// "3 invite links" / "1 invite link", with revoked links appended
/// (tdesktop `lng_group_invite_other_count`).
pub(crate) fn invite_link_count_label(active: i32, revoked: i32) -> String {
    let base = if active == 1 {
        "1 invite link".to_string()
    } else {
        format!("{active} invite links")
    };
    if revoked > 0 {
        format!("{base}, {revoked} revoked")
    } else {
        base
    }
}

/// "joined today" / "joined 12 Sep" for a member row.
pub(crate) fn joined_label(date: i32, via_folder: bool) -> String {
    if via_folder {
        return "joined via a folder invite link".into();
    }
    if date <= 0 {
        return "joined".into();
    }
    let at = civil_local(i64::from(date));
    let now = civil_local(now_unix());
    format!("joined {}", day_label(&at, &now))
}

impl QuillApp {
    fn invite_block_title(&self, text: &str, cx: &mut Context<Self>) -> Div {
        div()
            .text_xs()
            .font_semibold()
            .text_color(cx.theme().muted_foreground)
            .child(text.to_owned())
    }

    /// Toggle the "who joined" details of one link.
    pub(super) fn toggle_invite_link_details(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .invite_link_details
            .as_ref()
            .is_some_and(|(chat, link)| *chat == chat_id && link == invite_link);
        if open {
            self.invite_link_details = None;
            if let Some(live) = self.live.as_mut() {
                live.driver.close_chat_invite_link_members(chat_id);
            }
        } else {
            self.invite_link_details = Some((chat_id, invite_link.to_owned()));
            if let Some(live) = self.live.as_mut() {
                let _ = live
                    .driver
                    .open_chat_invite_link_members(chat_id, invite_link);
            }
        }
        cx.notify();
    }

    /// Show or hide the revoked-links list (fetched on first open).
    pub(super) fn toggle_revoked_links(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.revoked_links_open = !self.revoked_links_open;
        if self.revoked_links_open
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.fetch_revoked_chat_invite_links(chat_id);
        }
        cx.notify();
    }

    fn delete_revoked_link(&mut self, chat_id: ChatId, invite_link: &str, cx: &mut Context<Self>) {
        self.status_note = match self.live.as_mut() {
            Some(live) => match live
                .driver
                .delete_revoked_chat_invite_link(chat_id, invite_link)
            {
                Ok(_) => "deleting invite link…".into(),
                Err(_) => "could not delete invite link".into(),
            },
            None => "invite links need a live connection (demo)".into(),
        };
        cx.notify();
    }

    fn confirm_delete_all_revoked(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            alert
                .title("Delete all revoked links")
                .description(
                    "Are you sure you want to delete all revoked links? This action cannot be undone.",
                )
                .ok_text("Delete")
                .cancel_text("Cancel")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| {
                        this.status_note = match this.live.as_mut() {
                            Some(live) => {
                                match live.driver.delete_all_revoked_chat_invite_links(chat_id) {
                                    Ok(_) => "deleting revoked links…".into(),
                                    Err(_) => "could not delete revoked links".into(),
                                }
                            }
                            None => "invite links need a live connection (demo)".into(),
                        };
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn load_more_link_members(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_chat_invite_link_members(chat_id);
        }
        cx.notify();
    }

    /// Members who joined through `invite_link` (paged), shown under the
    /// link's row while its details are open.
    pub(super) fn invite_link_members_block(
        &self,
        chat_id: ChatId,
        invite_link: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self
            .invite_link_details
            .as_ref()
            .is_some_and(|(chat, link)| *chat == chat_id && link == invite_link);
        if !open {
            return None;
        }
        let state = self
            .session()
            .and_then(|s| s.invite_link_members.get(&chat_id.0))
            .filter(|state| state.invite_link == invite_link)
            .cloned();
        let muted = cx.theme().muted_foreground;
        let mut block = div()
            .id(format!("invite-link-members-{}", invite_link.len()))
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .pl_2();
        let Some(state) = state else {
            return Some(
                block
                    .child(div().text_xs().text_color(muted).child("Loading…"))
                    .into_any_element(),
            );
        };
        block = block.child(self.invite_block_title(
            &if state.total_count == 0 && !state.loading {
                "No one joined yet".to_string()
            } else if state.total_count == 1 {
                "1 joined".to_string()
            } else {
                format!("{} joined", state.total_count)
            },
            cx,
        ));
        for member in &state.members {
            let uid = member.user_id;
            let name = self.contact_display_name(uid);
            block = block.child(
                div()
                    .id(("invite-link-member", uid as u64))
                    .flex()
                    .items_center()
                    .w_full()
                    .gap_1()
                    .child(div().flex_1().min_w_0().text_sm().truncate().child(name))
                    .child(div().text_xs().text_color(muted).child(joined_label(
                        member.joined_chat_date,
                        member.via_chat_folder_invite_link,
                    ))),
            );
        }
        if let Some(error) = state.error.clone() {
            block = block.child(div().text_xs().text_color(muted).child(error));
        }
        if state.loading {
            block = block.child(div().text_xs().text_color(muted).child("Loading…"));
        } else if (state.members.len() as i32) < state.total_count {
            block = block.child(
                Button::new("invite-link-members-more")
                    .label("Show more")
                    .ghost()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.load_more_link_members(chat_id, cx);
                    })),
            );
        }
        Some(block.into_any_element())
    }

    /// Per-admin link counts (owner only; the driver gates the fetch).
    pub(super) fn invite_link_counts_block(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Some(InviteLinkCountsFetch::Loaded(counts)) = self
            .session()
            .and_then(|s| s.invite_link_counts.get(&chat_id.0))
            .cloned()
        else {
            return None;
        };
        let me = self.session().and_then(|s| s.my_user_id);
        let others: Vec<_> = counts
            .into_iter()
            .filter(|count| Some(count.user_id) != me)
            .collect();
        if others.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let mut block = div().flex().flex_col().w_full().gap_1().pt_1();
        block = block.child(self.invite_block_title("Links created by other admins", cx));
        for count in others {
            let uid = count.user_id;
            block = block.child(
                div()
                    .id(("invite-link-count", uid as u64))
                    .flex()
                    .items_center()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .child(self.contact_display_name(uid)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(invite_link_count_label(
                                count.invite_link_count,
                                count.revoked_invite_link_count,
                            )),
                    ),
            );
        }
        Some(block.into_any_element())
    }

    /// "Revoked links" toggle plus, when open, the list with per-link
    /// Delete and "Delete all".
    pub(super) fn revoked_invite_links_block(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let open = self.revoked_links_open;
        let mut block = div().flex().flex_col().w_full().gap_1().pt_1().child(
            div()
                .flex()
                .items_center()
                .w_full()
                .gap_1()
                .child(self.invite_block_title("Revoked links", cx))
                .child(div().flex_1())
                .child(
                    Button::new("revoked-links-toggle")
                        .label(if open { "Hide" } else { "Show" })
                        .ghost()
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_revoked_links(chat_id, cx);
                        })),
                ),
        );
        if !open {
            return block.into_any_element();
        }
        let fetch = self
            .session()
            .and_then(|s| s.revoked_invite_links.get(&chat_id.0))
            .cloned();
        match fetch {
            None | Some(InviteLinkFetch::Loading) => {
                block = block.child(div().text_xs().text_color(muted).child("Loading…"));
            }
            Some(InviteLinkFetch::Failed(message)) => {
                block = block.child(div().text_xs().text_color(muted).child(message));
            }
            Some(InviteLinkFetch::Loaded(list)) => {
                if list.links.is_empty() {
                    block =
                        block.child(div().text_xs().text_color(muted).child("No revoked links."));
                } else {
                    block = block.child(
                        Button::new("revoked-links-delete-all")
                            .label("Delete all revoked links")
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.confirm_delete_all_revoked(chat_id, window, cx);
                            })),
                    );
                }
                for (index, link) in list.links.into_iter().enumerate() {
                    let delete_link = link.invite_link.clone();
                    let name = if link.name.is_empty() {
                        link.invite_link.clone()
                    } else {
                        link.name.clone()
                    };
                    block = block.child(
                        div()
                            .id(("revoked-link-row", index as u64))
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
                                            .child(format!("{} uses", link.member_count)),
                                    ),
                            )
                            .child(
                                Button::new(format!("revoked-link-delete-{index}"))
                                    .label("Delete")
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_revoked_link(chat_id, &delete_link, cx);
                                    })),
                            ),
                    );
                }
            }
        }
        block.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{invite_link_count_label, joined_label, subscription_price_label};
    use quill::telegram::envelope::StarSubscriptionPricing;

    #[test]
    fn count_labels_pluralise_and_append_revoked() {
        assert_eq!(invite_link_count_label(1, 0), "1 invite link");
        assert_eq!(invite_link_count_label(3, 0), "3 invite links");
        assert_eq!(invite_link_count_label(2, 4), "2 invite links, 4 revoked");
    }

    #[test]
    fn joined_label_handles_folder_and_missing_dates() {
        assert_eq!(joined_label(0, false), "joined");
        assert_eq!(joined_label(5, true), "joined via a folder invite link");
        assert!(joined_label(1_700_000_000, false).starts_with("joined "));
    }

    #[test]
    fn price_label_reads_stars_per_month() {
        let pricing = StarSubscriptionPricing {
            period: 2_592_000,
            star_count: 250,
        };
        assert_eq!(subscription_price_label(&pricing), "250 Stars / month");
    }
}
