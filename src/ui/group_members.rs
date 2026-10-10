//! member-management dialog.

use super::app::QuillApp;
use super::shell::DialogKind;
use super::shell::QuillShell;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::{ContactRow, MemberListFilter, SupergroupMembersFetch};
use quill::telegram::envelope::{ChannelMemberStatus, ChatKind, MessageSender};
use std::cell::RefCell;
use std::rc::Rc;
impl QuillApp {
    /// Slice G1: open the member-management dialog and kick off the
    /// member fetch for the current tab.
    pub(super) fn open_member_dialog(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_basic_group = self.session().is_some_and(|session| {
            session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| matches!(chat.kind, ChatKind::BasicGroup { .. }))
        });
        self.member_dialog = Some(MemberDialog::new(window, cx, chat_id, is_basic_group));
        // Slice G1: a fresh dialog open clears the last action error —
        // stale failures from a previous open must not linger.
        if let Some(live) = self.live.as_mut() {
            live.driver.session.member_action_error.remove(&chat_id.0);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.member_action_error.remove(&chat_id.0);
        }
        cx.notify();
        self.refresh_member_dialog(cx);
    }

    pub(super) fn close_member_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.member_dialog.take() {
            let chat_id = dialog.chat_id;
            if let Some(live) = self.live.as_mut() {
                live.driver.session.add_members_failed.remove(&chat_id.0);
            } else if let Some(session) = self.demo_session.as_mut() {
                session.add_members_failed.remove(&chat_id.0);
            }
        }
        cx.notify();
    }

    /// Slice G1: (re)fetch the member page the dialog's current tab
    /// shows. Basic groups read `getBasicGroupFullInfo`; supergroups
    /// read the matching `getSupergroupMembers` filter.
    pub(super) fn refresh_member_dialog(&mut self, cx: &mut Context<Self>) {
        let (chat_id, is_basic_group, tab, query) = match self.member_dialog.as_ref() {
            Some(dialog) => (
                dialog.chat_id,
                dialog.is_basic_group,
                dialog.tab,
                dialog.search_input.read(cx).value().trim().to_string(),
            ),
            None => return,
        };
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let result = if is_basic_group {
            live.driver.fetch_basic_group_members(chat_id).map(|_| ())
        } else if tab == MemberTab::All && !query.is_empty() {
            live.driver
                .refresh_supergroup_members(chat_id, MemberListFilter::Search, &query)
                .map(|_| ())
        } else {
            let filter = tab.filter().unwrap_or(MemberListFilter::Recent);
            live.driver
                .refresh_supergroup_members(chat_id, filter, &query)
                .map(|_| ())
        };
        if result.is_err() {
            self.status_note = "could not load members".into();
        }
        cx.notify();
    }

    pub(super) fn member_dialog_tab(&mut self, tab: MemberTab, cx: &mut Context<Self>) {
        if let Some(dialog) = self.member_dialog.as_mut() {
            dialog.tab = tab;
        }
        self.refresh_member_dialog(cx);
    }

    pub(super) fn toggle_member_add_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(dialog) = self.member_dialog.as_mut() {
            if let Some(position) = dialog.add_selected.iter().position(|id| *id == user_id) {
                dialog.add_selected.remove(position);
            } else {
                dialog.add_selected.push(user_id);
            }
            cx.notify();
        }
    }

    /// Slice G1: add the picked contacts (`addChatMember` per user for
    /// basic groups, one bulk `addChatMembers` for supergroups — the
    /// driver picks). Failures surface via `FailedToAddMembers` in
    /// `status_note` through the session's `add_members_failed` map.
    pub(super) fn submit_member_add(&mut self, cx: &mut Context<Self>) {
        let (chat_id, user_ids) = match self.member_dialog.as_mut() {
            Some(dialog) => (dialog.chat_id, std::mem::take(&mut dialog.add_selected)),
            None => return,
        };
        if user_ids.is_empty() {
            self.status_note = "pick at least one contact to add".into();
            cx.notify();
            return;
        }
        let note = match self.live.as_mut() {
            Some(live) => match live.driver.add_chat_members(chat_id, &user_ids) {
                Ok(_) => "adding members".into(),
                Err(_) => "could not add members".into(),
            },
            None => "adding members needs a live connection (demo)".into(),
        };
        self.status_note = note;
        // `updateChatMember` drops the cached member list and marks the
        // chat stale; `poll_live` refetches the open dialog's page.
        self.refresh_member_dialog(cx);
    }

    /// kit Phase 2 (redo): member dialog hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_member_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::Member, |this, _, cx| {
            this.close_member_dialog(cx);
        });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(dialog_state) = this.member_dialog.as_ref() else {
                return dialog
                    .title(crate::ui::shell::dialog_title("Manage members"))
                    .on_close(on_close);
            };
            let chat_id = dialog_state.chat_id;
            let is_basic_group = dialog_state.is_basic_group;
            let tab = dialog_state.tab;
            let add_open = dialog_state.add_open;
            let add_query = dialog_state.add_search.read(cx).value();
            let add_selected = dialog_state.add_selected.clone();
            let can_restrict = this
                .session()
                .is_some_and(|session| session.chat_can_restrict_members(chat_id));
            let can_add = this
                .session()
                .is_some_and(|session| session.chat_can_add_members(chat_id));
            let fetch = this.member_dialog_fetch(cx);

            let mut body = div().flex().flex_col().gap_2();
            // kit Phase 3: the member tab strip as a kit segmented
            // `TabBar` — the visible tabs depend on group type and the
            // viewer's restrict permission, as before.
            let visible_tabs: Vec<MemberTab> = [
                MemberTab::All,
                MemberTab::Administrators,
                MemberTab::Restricted,
                MemberTab::Banned,
            ]
            .into_iter()
            .filter(|member_tab| {
                if is_basic_group && *member_tab != MemberTab::All {
                    return false;
                }
                if !can_restrict && matches!(member_tab, MemberTab::Restricted | MemberTab::Banned)
                {
                    return false;
                }
                true
            })
            .collect();
            let selected_tab = visible_tabs.iter().position(|t| *t == tab).unwrap_or(0);
            let member_weak = cx.weak_entity();
            let mut member_bar = TabBar::new("g1-member-tabs").segmented();
            for member_tab in &visible_tabs {
                member_bar = member_bar.child(Tab::new().label(member_tab.label()));
            }
            body = body.child(member_bar.selected_index(selected_tab).on_click(
                move |ix, window, cx| {
                    if let Some(member_tab) = visible_tabs.get(*ix).copied() {
                        let _ = member_weak.update(cx, |this, cx| {
                            this.member_dialog_tab(member_tab, cx);
                            this.close_kit_dialog_if_done(DialogKind::Member, window, cx);
                        });
                    }
                },
            ));
            if let Some(error) = this
                .session()
                .and_then(|session| session.member_action_error.get(&chat_id.0))
                .cloned()
            {
                body = body.child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .p_2()
                        .rounded_md()
                        .bg(danger_bg_deep())
                        .child(div().text_sm().text_color(danger_pale()).child(error))
                        .child(
                            Button::new("g1-member-action-error-dismiss")
                                .label("Dismiss")
                                .ghost()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.dismiss_member_action_error(chat_id, cx);
                                    this.close_kit_dialog_if_done(DialogKind::Member, window, cx);
                                })),
                        ),
                );
            }
            // Search (supergroups only — basic groups list everyone).
            if !is_basic_group {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().flex_1().child(
                                Textarea::new(&dialog_state.search_input)
                                    .aria_label("Search members")
                                    .h(px(40.)),
                            ),
                        )
                        .child(
                            Button::new("g1-member-search")
                                .label("Search")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.refresh_member_dialog(cx);
                                    this.close_kit_dialog_if_done(DialogKind::Member, window, cx);
                                })),
                        ),
                );
            }
            // Member rows.
            let mut list = div()
                .id("g1-member-list")
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(240.))
                .overflow_y_scroll();
            match fetch {
                None | Some(SupergroupMembersFetch::Loading) => {
                    list = list.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Loading members…"),
                    );
                }
                Some(SupergroupMembersFetch::Failed(message)) => {
                    list = list.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(message),
                            )
                            .child(
                                Button::new("g1-member-retry")
                                    .label("Retry")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.refresh_member_dialog(cx);
                                        this.close_kit_dialog_if_done(
                                            DialogKind::Member,
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    );
                }
                Some(SupergroupMembersFetch::Loaded { members, .. }) => {
                    if members.is_empty() {
                        list = list.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("No members"),
                        );
                    }
                    for member in members.iter().take(200) {
                        list = list.child(this.member_row(chat_id, member, cx));
                    }
                }
            }
            body = body.child(list);
            // Add-members section.
            if can_add && tab == MemberTab::All {
                let toggle_label = if add_open {
                    "▾ Add members"
                } else {
                    "▸ Add members"
                };
                body = body.child(
                    Button::new("g1-add-toggle")
                        .label(toggle_label)
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(dialog) = this.member_dialog.as_mut() {
                                dialog.add_open = !dialog.add_open;
                                cx.notify();
                            }
                            this.close_kit_dialog_if_done(DialogKind::Member, window, cx);
                        })),
                );
                if add_open {
                    if let Some(failed) = this
                        .session()
                        .and_then(|session| session.add_members_failed.get(&chat_id.0))
                        .copied()
                        .filter(|count| *count > 0)
                    {
                        body =
                            body.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div().flex_1().text_xs().text_color(danger()).child(
                                            format!("{failed} member(s) could not be added"),
                                        ),
                                    )
                                    .child(
                                        Button::new("g1-add-failed-dismiss")
                                            .label("Dismiss")
                                            .ghost()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                if let Some(live) = this.live.as_mut() {
                                                    live.driver
                                                        .session
                                                        .add_members_failed
                                                        .remove(&chat_id.0);
                                                }
                                                cx.notify();
                                                this.close_kit_dialog_if_done(
                                                    DialogKind::Member,
                                                    window,
                                                    cx,
                                                );
                                            })),
                                    ),
                            );
                    }
                    body = body.child(
                        div().flex_1().child(
                            Textarea::new(&dialog_state.add_search)
                                .aria_label("Search people to add")
                                .h(px(40.)),
                        ),
                    );
                    let mut add_list = div()
                        .id("g1-add-contacts")
                        .flex()
                        .flex_col()
                        .gap_1()
                        .max_h(px(160.))
                        .overflow_y_scroll();
                    let rows = this.g1_contact_rows(&add_query, cx);
                    for row in rows.iter().take(50) {
                        let is_selected = add_selected.contains(&row.user_id);
                        add_list = add_list.child(this.g1_contact_checkbox(
                            "g1-add".to_string(),
                            row,
                            is_selected,
                            row.user_id,
                            cx,
                        ));
                    }
                    body = body.child(add_list);
                    body = body.child(
                        Button::new("g1-add-submit")
                            .label(if add_selected.is_empty() {
                                "Add members".to_string()
                            } else {
                                format!("Add {} member(s)", add_selected.len())
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_member_add(cx);
                                this.close_kit_dialog_if_done(DialogKind::Member, window, cx);
                            })),
                    );
                }
            }
            let body = body.into_any_element();
            dialog
                .title(crate::ui::shell::dialog_title(if is_basic_group {
                    "Group members"
                } else {
                    "Manage members"
                }))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    /// Slice G1: contact rows filtered by a dialog search query.
    pub(super) fn g1_contact_rows(&self, query: &str, _cx: &mut Context<Self>) -> Vec<ContactRow> {
        let query = query.trim().to_lowercase();
        self.session()
            .map(|session| {
                session
                    .contact_rows()
                    .into_iter()
                    .filter(|row| query.is_empty() || row.name.to_lowercase().contains(&query))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Slice G1: one contact row with a checkbox, shared by the
    /// create-chat and add-members pickers.
    pub(super) fn g1_contact_checkbox(
        &self,
        id_prefix: String,
        row: &ContactRow,
        selected: bool,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = row.name.clone();
        div()
            .id(format!("{id_prefix}-contact-{user_id}"))
            .flex()
            .items_center()
            .gap_2()
            .child(
                // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                // The staged picks flip, so fire only when the requested
                // value differs from the rendered one.
                Checkbox::new(format!("{id_prefix}-toggle-{user_id}"))
                    .checked(selected)
                    .label(name)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        if on != selected {
                            this.toggle_g1_contact_pick(&id_prefix, user_id, cx);
                        }
                    })),
            )
            .into_any_element()
    }

    /// Slice G1: route a contact checkbox toggle to the open dialog
    /// that owns `id_prefix`.
    pub(super) fn toggle_g1_contact_pick(
        &mut self,
        id_prefix: &str,
        user_id: i64,
        cx: &mut Context<Self>,
    ) {
        match id_prefix {
            "g1-create" => self.toggle_create_chat_user(user_id, cx),
            "g1-add" => self.toggle_member_add_user(user_id, cx),
            // Phase 9.3: the story composer's "Selected users" picker.
            "story-composer" => self.toggle_story_composer_user(user_id, cx),
            // Phase 9.5: the viewer privacy editor's "Selected users" picker.
            "story-privacy" => self.toggle_story_privacy_user(user_id, cx),
            // B14: the story viewer's close-friends editor.
            "story-close-friends" => self.toggle_close_friend(user_id, cx),
            _ => {}
        }
    }

    /// Slice G1: the member page the dialog's current tab shows.
    pub(super) fn member_dialog_fetch(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<SupergroupMembersFetch> {
        let dialog = self.member_dialog.as_ref()?;
        let session = self.session()?;
        if dialog.is_basic_group {
            session.basic_group_members.get(&dialog.chat_id.0).cloned()
        } else {
            let query = dialog.search_input.read(cx).value();
            let filter = match dialog.tab.filter() {
                Some(filter) => filter,
                None if query.trim().is_empty() => MemberListFilter::Recent,
                None => MemberListFilter::Search,
            };
            // Slice G1 fix-up: a Restricted/Banned tab left open after
            // the viewer lost restrict rights has no fetchable page —
            // fall back to Recent instead of spinning forever.
            let filter = if !session.chat_can_restrict_members(dialog.chat_id)
                && matches!(
                    filter,
                    MemberListFilter::Restricted | MemberListFilter::Banned
                ) {
                MemberListFilter::Recent
            } else {
                filter
            };
            session
                .supergroup_members
                .get(&(dialog.chat_id.0, filter))
                .cloned()
        }
    }

    pub(super) fn member_status_label(status: ChannelMemberStatus) -> &'static str {
        match status {
            ChannelMemberStatus::Creator => "owner",
            ChannelMemberStatus::Administrator => "admin",
            ChannelMemberStatus::Member => "member",
            ChannelMemberStatus::Restricted => "restricted",
            ChannelMemberStatus::Banned => "banned",
            ChannelMemberStatus::Left => "left",
            ChannelMemberStatus::Unknown => "",
        }
    }

    /// Slice G1: one member row — name, status, and the tab's actions
    /// (restrict/ban on All, edit/unrestrict on Restricted, unban on
    /// Banned; admins are read-only here — promotion lives in the
    /// admin dialog). Actions only render for user senders and when
    /// the viewer may restrict members.
    pub(super) fn member_row(
        &self,
        chat_id: ChatId,
        member: &quill::telegram::envelope::ParsedChatMember,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let user_id = match member.member_id {
            MessageSender::User { user_id } => Some(user_id),
            MessageSender::Chat { .. } => None,
        };
        let name = user_id
            .and_then(|id| {
                self.session()
                    .and_then(|session| session.user(id))
                    .map(|user| user.display_name())
            })
            .unwrap_or_else(|| match member.member_id {
                MessageSender::User { user_id } => format!("User {user_id}"),
                MessageSender::Chat { chat_id } => format!("Channel {chat_id}"),
            });
        let status_label = Self::member_status_label(member.status);
        // Slice G1: show the admin custom title (`chatMember.tag`,
        // schema 1.8.67 line 2526) next to the status when set.
        let tag_label = (!member.tag.is_empty()).then(|| format!("❝{}❞", member.tag));
        let row = div()
            .id(format!("g1-member-row-{}", user_id.unwrap_or_default()))
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .child(super::bidi_line::one_line_plain(name)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status_label),
            )
            .when_some(tag_label, |this, label| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().accent_foreground)
                        .child(label),
                )
            });
        // The member menu (tdesktop `rowContextMenu`): right-click the row
        // or use the "more" button. Which items appear is decided by
        // `quill::moderation::member_menu_actions` from the viewer's rights.
        let build = self.member_menu_build(chat_id, member, cx);
        match build {
            Some(build) => {
                let for_button = build.clone();
                row.child(
                    Button::new(format!("g1-member-more-{}", user_id.unwrap_or_default()))
                        .icon(gpui_kit::assets::IconName::EllipsisVertical)
                        .ghost()
                        .xsmall()
                        .accessibility_label("Member actions")
                        .dropdown_menu(move |menu, window, cx| for_button(menu, window, cx)),
                )
                .context_menu(move |menu, window, cx| build(menu, window, cx))
                .into_any_element()
            }
            None => row.into_any_element(),
        }
    }
}

crate::ui::shell::register_dialogs! {
    Member => DialogSpec::new(
        4300,
        |app| app.member_dialog.is_some(),
        QuillApp::build_member_dialog,
    ),
}
