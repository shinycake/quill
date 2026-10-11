//! Methods moved out of `group_moderation.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// Phase D3b: the promote member picker — search field, member rows
    /// (click to select; already-admin members are excluded), rights
    /// checkboxes, Promote / Cancel.
    pub(in crate::ui) fn promote_dialog_panel(
        &self,
        chat_id: ChatId,
        search_input: &Entity<TextareaState>,
        rights: &ChatAdminRights,
        selected_user: Option<i64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let admin_ids: std::collections::HashSet<i64> = self
            .session()
            .and_then(|session| session.groups.admin_lists.get(&chat_id.0))
            .and_then(|fetch| match fetch {
                AdminListFetch::Loaded(list) => Some(list),
                _ => None,
            })
            .map(|list| list.iter().map(|entry| entry.user_id).collect())
            .unwrap_or_default();
        let members = self.session().and_then(|session| {
            // Slice G1: the promote picker browses Recent / Search
            // pages; show whichever matches the current query.
            let query = search_input.read(cx).value();
            let filter = if query.trim().is_empty() {
                MemberListFilter::Recent
            } else {
                MemberListFilter::Search
            };
            session
                .groups
                .supergroup_members
                .get(&(chat_id.0, filter))
                .cloned()
        });
        let mut panel = div()
            .id("admin-promote-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(text_bright())
                    .child("Promote member"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .child(Textarea::new(search_input).aria_label("Search").h(px(40.))),
                    )
                    .child(
                        Button::new("admin-promote-search")
                            .label("Search")
                            .ghost()
                            .text_color(text_bright())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.search_promote_members(cx);
                            })),
                    ),
            );
        match members {
            None | Some(SupergroupMembersFetch::Loading) => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading members…"),
                );
            }
            Some(SupergroupMembersFetch::Failed(message)) => {
                panel = panel.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(message),
                        )
                        .child(
                            Button::new("admin-promote-retry")
                                .label("Retry")
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.search_promote_members(cx);
                                })),
                        ),
                );
            }
            Some(SupergroupMembersFetch::Loaded { members, .. }) => {
                // Phase 6: kit RadioGroup (was: buttons with a ●/○ prefix).
                // Controlled: the chosen index writes the value.
                let candidates: Vec<(i64, String)> = members
                    .iter()
                    .filter_map(|member| match member.member_id {
                        MessageSender::User { user_id } => Some(user_id),
                        _ => None,
                    })
                    .filter(|user_id| !admin_ids.contains(user_id))
                    .take(30)
                    .map(|user_id| {
                        let name = self
                            .session()
                            .and_then(|session| session.user(user_id))
                            .map(|user| user.display_name())
                            .unwrap_or_else(|| format!("User {user_id}"));
                        (user_id, name)
                    })
                    .collect();
                if candidates.is_empty() {
                    panel = panel.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No members found."),
                    );
                } else {
                    let selected_ix = candidates
                        .iter()
                        .position(|(user_id, _)| selected_user == Some(*user_id));
                    panel = panel.child(
                        RadioGroup::vertical("admin-promote-member")
                            .selected_index(selected_ix)
                            .children(candidates.iter().map(|(user_id, name)| {
                                Radio::new(format!("admin-promote-member-{user_id}"))
                                    .label(name.clone())
                            }))
                            .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                                let user_id = candidates[ix].0;
                                if let Some(dialog) = this.admin.admin_dialog.as_mut()
                                    && let AdminDialogKind::Promote { selected_user, .. } =
                                        &mut dialog.kind
                                {
                                    *selected_user = Some(user_id);
                                }
                                cx.notify();
                            })),
                    );
                }
            }
        }
        let summary = admin_rights_summary(rights);
        panel = panel
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Rights ({summary})")),
            )
            .child(self.rights_checkboxes(rights, "admin-right", cx))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("admin-promote-submit")
                            .label("Promote")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_admin_dialog(cx);
                            })),
                    )
                    .child(
                        Button::new("admin-promote-cancel")
                            .label("Cancel")
                            .ghost()
                            .text_color(text_bright())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_admin_dialog(cx);
                            })),
                    ),
            );
        panel.into_any_element()
    }

    /// Phase D3b: the rights editor for one administrator. While the
    /// `getChatMember` lookup is in flight the dialog shows a loading
    /// row; once rights land (cached or staged in the dialog) the 18
    /// checkboxes bind to the dialog's staged copy and Save sends
    /// `setChatMemberStatus`.
    pub(in crate::ui) fn rights_editor_panel(
        &self,
        chat_id: ChatId,
        user_id: i64,
        staged: Option<ChatAdminRights>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .session()
            .and_then(|session| session.user(user_id))
            .map(|user| user.display_name())
            .unwrap_or_else(|| format!("User {user_id}"));
        let cached = self
            .session()
            .and_then(|session| session.groups.admin_rights.get(&(chat_id.0, user_id)))
            .and_then(|fetch| match fetch {
                AdminRightsFetch::Loaded(rights) => Some(*rights),
                _ => None,
            });
        let rights = staged.or(cached);
        let mut panel = div()
            .id("admin-rights-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(text_bright())
                    .child(format!("Edit rights — {name}")),
            );
        match rights {
            None => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading rights…"),
                );
            }
            Some(rights) => {
                panel = panel
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(admin_rights_summary(&rights)),
                    )
                    .child(self.rights_checkboxes(&rights, "admin-edit-right", cx));
            }
        }
        panel = panel.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("admin-rights-save")
                        .label("Save")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.submit_admin_dialog(cx);
                        })),
                )
                .child(
                    Button::new("admin-rights-cancel")
                        .label("Cancel")
                        .ghost()
                        .text_color(text_bright())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_admin_dialog(cx);
                        })),
                ),
        );
        panel.into_any_element()
    }

    /// Phase D3b: demote confirmation for one administrator.
    pub(in crate::ui) fn demote_confirm_panel(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .session()
            .and_then(|session| session.user(user_id))
            .map(|user| user.display_name())
            .unwrap_or_else(|| format!("User {user_id}"));
        div()
            .id("admin-demote-dialog")
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(text_bright())
                    .child(format!("Demote {name}?")),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("They will become a regular member and lose all admin rights."),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(Button::new("admin-demote-submit").label("Demote").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.submit_admin_dialog(cx);
                        }),
                    ))
                    .child(
                        Button::new("admin-demote-cancel")
                            .label("Cancel")
                            .ghost()
                            .text_color(text_bright())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_admin_dialog(cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}
