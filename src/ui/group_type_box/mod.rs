//! The group and channel Type box (tdesktop `EditPeerTypeBox`), opened
//! from the Edit box's "Group type" / "Channel type" row: Public or
//! Private, the t.me link with its live check, the other usernames, who
//! may join or write, and content protection. Save hands the choice back
//! to the Edit box (`GroupEditBox::type_choice`), whose own Save sends it,
//! as tdesktop's `savedCallback` does. A new supergroup or channel shows
//! the same box as a step of its New Group / New Channel box, cut down to
//! tdesktop's `SetupChannelBox` (Public or Private and the link), and its
//! Save sends at once (`ui::new_chat`).
#![deny(clippy::indexing_slicing)]

mod sections;
mod usernames;

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::group_type::{
    CHECK_DELAY_MS, GroupTypeData, LinkProblem, LinkStatus, USERNAME_MAX, check_outcome,
    join_approval_applies, link_status, local_link_status,
};
use quill::ids::ChatId;
use quill::lang::{keys, tr, trn};
use quill::premium_limits::PremiumLimitKind;
use quill::telegram::envelope::ChatKind;
use std::time::Duration;

/// The open Type box: the draft it edits and its link field.
pub(crate) struct GroupTypeBox {
    pub(crate) draft: GroupTypeData,
    pub(crate) link_input: Entity<InputState>,
    /// Counts keystrokes; a check timer for an older one does nothing.
    version: u64,
    /// Save found the link unusable; the line under the field says why.
    save_refused: bool,
}

/// What the box knows about its chat, read from the session each frame.
pub(super) struct TypeBoxFacts {
    pub(super) chat_id: ChatId,
    pub(super) group: bool,
    pub(super) basic: bool,
    /// Public can't be chosen: the account is at its public links limit,
    /// or the server said this chat can't be public.
    pub(super) public_blocked: bool,
    /// The chat's own editable link.
    pub(super) editable: String,
    /// A discussion group: "Only members" applies.
    pub(super) discussion: bool,
    /// The step of a chat being created (tdesktop `SetupChannelBox`).
    pub(super) setup: bool,
}

impl GroupTypeBox {
    pub(crate) fn new(draft: GroupTypeData, link_input: Entity<InputState>) -> Self {
        Self {
            draft,
            link_input,
            version: 0,
            save_refused: false,
        }
    }

    /// The link as typed: the field is the one place it lives.
    pub(super) fn link_text(&self, cx: &App) -> String {
        self.link_input.read(cx).value().trim().to_string()
    }

    /// The line under the link field.
    pub(super) fn status(&self, app: &QuillApp, facts: &TypeBoxFacts, cx: &App) -> LinkStatus {
        let text = self.link_text(cx);
        let verdict = app
            .session()
            .and_then(|s| s.group_username_verdict(facts.chat_id, &text))
            .map(|result| check_outcome(result, &text, &facts.editable));
        match link_status(&text, &facts.editable, verdict) {
            LinkStatus::Empty if self.save_refused => LinkStatus::Problem(LinkProblem::TooShort),
            status => status,
        }
    }
}

impl QuillApp {
    pub(crate) fn group_type_box(&self) -> Option<&GroupTypeBox> {
        match self.group_edit_box() {
            Some(edit) => edit.type_box.as_ref(),
            None => self.new_chat_type_box(),
        }
    }

    fn group_type_box_mut(&mut self) -> Option<&mut GroupTypeBox> {
        if self.group_edit_box().is_some() {
            return self.group_edit_box_mut()?.type_box.as_mut();
        }
        self.new_chat_type_box_mut()
    }

    /// The Type box open as its own dialog over the Edit box.
    fn group_type_box_dialog_open(&self) -> bool {
        self.group_edit_box()
            .is_some_and(|edit| edit.type_box.is_some())
    }

    /// The chat the Type box edits, and whether it is a creation step.
    fn group_type_chat(&self) -> Option<(ChatId, bool)> {
        match self.group_edit_box() {
            Some(edit) => Some((edit.chat_id, false)),
            None => self.new_chat_type_chat().map(|chat| (chat, true)),
        }
    }

    pub(super) fn group_type_facts(&self) -> Option<TypeBoxFacts> {
        let (chat_id, setup) = self.group_type_chat()?;
        let session = self.session()?;
        let chat = session.chats.get(&chat_id.0)?;
        let current = session.group_type_current(chat_id);
        let state = &session.groups.group_type;
        let public_blocked = !current.public
            && (state.public_limit_reached == Some(true)
                || state.public_unavailable.contains(&chat_id.0));
        Some(TypeBoxFacts {
            chat_id,
            group: !chat.is_channel(),
            basic: matches!(chat.kind, ChatKind::BasicGroup { .. }),
            public_blocked,
            editable: current.username,
            discussion: session.group_admin_controls(chat_id).join_to_send,
            setup,
        })
    }

    /// The Edit box's type row: open the Type box on the last choice, or
    /// on the chat as it is.
    pub(in crate::ui) fn open_group_type_box(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.group_edit_box() else {
            return;
        };
        let chat_id = edit.chat_id;
        let Some(session) = self.session() else {
            return;
        };
        let draft = edit
            .type_choice
            .clone()
            .unwrap_or_else(|| session.group_type_current(chat_id));
        let supergroup = matches!(
            session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        );
        let link_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::CREATE_GROUP_LINK))
                .validate(|text, _| text.chars().count() <= USERNAME_MAX)
                .default_value(draft.username.clone())
        });
        cx.subscribe(&link_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.group_link_changed(cx);
            }
        })
        .detach();
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_chat_invite_links(chat_id);
            if supergroup {
                let _ = live.driver.fetch_chat_invite_link_counts(chat_id);
            }
            if !draft.public {
                let _ = live.driver.check_public_chats_limit();
            }
        }
        if let Some(edit) = self.group_edit_box_mut() {
            edit.type_box = Some(GroupTypeBox::new(draft, link_input));
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_group_type_box(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.group_edit_box_mut() {
            edit.type_box = None;
        }
        cx.notify();
    }

    /// tdesktop `usernameChanged`: the local rules at once, the server
    /// after a pause.
    pub(in crate::ui) fn group_link_changed(&mut self, cx: &mut Context<Self>) {
        let Some(facts) = self.group_type_facts() else {
            return;
        };
        let Some(type_box) = self.group_type_box_mut() else {
            return;
        };
        let text = type_box.link_text(cx);
        type_box.save_refused = false;
        type_box.version = type_box.version.wrapping_add(1);
        let version = type_box.version;
        if facts.setup {
            self.new_chat_link_edited();
        }
        cx.notify();
        if local_link_status(&text) != LinkStatus::Pending || text == facts.editable {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(CHECK_DELAY_MS))
                .await;
            let _ = this.update(cx, |this, cx| this.check_group_link(version, cx));
        })
        .detach();
    }

    fn check_group_link(&mut self, version: u64, cx: &mut Context<Self>) {
        let Some(type_box) = self.group_type_box() else {
            return;
        };
        if type_box.version != version || !type_box.draft.public {
            return;
        }
        let text = type_box.link_text(cx);
        let Some((chat_id, _)) = self.group_type_chat() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.check_group_username(chat_id, &text);
        }
        cx.notify();
    }

    /// After a link was freed: check the typed link again.
    pub(in crate::ui) fn recheck_group_type_link(&mut self, cx: &mut Context<Self>) {
        let Some(facts) = self.group_type_facts() else {
            return;
        };
        let Some(type_box) = self.group_type_box() else {
            return;
        };
        let text = type_box.link_text(cx);
        if local_link_status(&text) == LinkStatus::Pending && text != facts.editable {
            let version = type_box.version;
            self.check_group_link(version, cx);
        }
    }

    /// The Public / Private choice. At the public links limit Public opens
    /// the limit box with the chats a link can be freed from (tdesktop
    /// `askUsernameRevoke`).
    pub(super) fn choose_group_public(&mut self, public: bool, cx: &mut Context<Self>) {
        let Some(facts) = self.group_type_facts() else {
            return;
        };
        if public && facts.public_blocked {
            let unavailable = self.session().is_some_and(|s| {
                s.groups
                    .group_type
                    .public_unavailable
                    .contains(&facts.chat_id.0)
            });
            if !unavailable {
                if let Some(type_box) = self.group_type_box_mut() {
                    type_box.draft.public = true;
                }
                self.show_premium_limit(PremiumLimitKind::PublicLinks, 0, cx);
            }
            return;
        }
        let Some(type_box) = self.group_type_box_mut() else {
            return;
        };
        type_box.draft.public = public;
        if !join_approval_applies(public, facts.group) {
            type_box.draft.request_to_join = false;
        }
        let pending = public && local_link_status(&type_box.link_text(cx)) == LinkStatus::Pending;
        let version = type_box.version;
        if pending {
            self.check_group_link(version, cx);
        }
        cx.notify();
    }

    pub(super) fn edit_group_type_draft(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut GroupTypeData),
    ) {
        if let Some(type_box) = self.group_type_box_mut() {
            change(&mut type_box.draft);
        }
        cx.notify();
    }

    /// tdesktop `EditPeerTypeBox` Save: a public chat needs a usable link;
    /// a changed join approval asks about the existing invite links first.
    pub(in crate::ui) fn save_group_type_box(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(facts) = self.group_type_facts() else {
            return;
        };
        let Some(type_box) = self.group_type_box() else {
            return;
        };
        let public = type_box.draft.public && !facts.public_blocked;
        if public && !type_box.status(self, &facts, cx).is_good() {
            if let Some(type_box) = self.group_type_box_mut() {
                type_box.save_refused = true;
                type_box
                    .link_input
                    .update(cx, |input, cx| input.focus(window, cx));
            }
            cx.notify();
            return;
        }
        let current = self
            .session()
            .map(|s| s.group_type_current(facts.chat_id))
            .unwrap_or_default();
        let links = self
            .session()
            .map_or(0, |s| s.chat_active_invite_link_count(facts.chat_id));
        let request_changed = type_box.draft.request_to_join != current.request_to_join;
        if request_changed && links > 0 && !facts.basic {
            self.ask_apply_to_invite_links(
                &facts,
                type_box.draft.request_to_join,
                links,
                window,
                cx,
            );
            return;
        }
        self.finish_group_type_box(None, cx);
    }

    /// "Apply to existing invite links" (tdesktop `saveRequestToJoin`).
    fn ask_apply_to_invite_links(
        &mut self,
        facts: &TypeBoxFacts,
        enabled: bool,
        links: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = match (enabled, facts.group) {
            (true, true) => keys::MANAGE_PEER_REQUEST_APPLY_ENABLE_GROUP,
            (true, false) => keys::MANAGE_PEER_REQUEST_APPLY_ENABLE_CHANNEL,
            (false, true) => keys::MANAGE_PEER_REQUEST_APPLY_DISABLE_GROUP,
            (false, false) => keys::MANAGE_PEER_REQUEST_APPLY_DISABLE_CHANNEL,
        };
        let text = trn(key, links).replace("**", "");
        let app = cx.entity().downgrade();
        let skip_app = app.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let app = app.clone();
            let skip_app = skip_app.clone();
            alert
                .title(tr(keys::MANAGE_PEER_REQUEST_APPLY_TITLE))
                .description(text.clone())
                .ok_text(tr(keys::MANAGE_PEER_REQUEST_APPLY_CONFIRM))
                .cancel_text(tr(keys::MANAGE_PEER_REQUEST_APPLY_SKIP))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| this.finish_group_type_box(Some(true), cx));
                    true
                })
                .on_cancel(move |_, _, cx| {
                    let _ =
                        skip_app.update(cx, |this, cx| this.finish_group_type_box(Some(false), cx));
                    true
                })
        });
    }

    /// Hand the choice to the Edit box and close.
    fn finish_group_type_box(&mut self, apply_to_invites: Option<bool>, cx: &mut Context<Self>) {
        let blocked = self.group_type_facts().is_some_and(|f| f.public_blocked);
        if self.group_edit_box().is_none() {
            let Some(type_box) = self.new_chat_type_box() else {
                return;
            };
            let mut draft = type_box.draft.clone();
            if blocked {
                draft.public = false;
            }
            draft.username = type_box.link_text(cx);
            self.save_new_chat_type(draft, cx);
            return;
        }
        let Some(edit) = self.group_edit_box_mut() else {
            return;
        };
        let Some(mut type_box) = edit.type_box.take() else {
            return;
        };
        if blocked {
            type_box.draft.public = false;
        }
        type_box.draft.apply_to_invites = apply_to_invites;
        type_box.draft.username = type_box.link_text(cx);
        edit.type_choice = Some(type_box.draft);
        cx.notify();
    }

    pub(super) fn build_group_type_box_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GroupTypeBox, |this, _, cx| {
                this.close_group_type_box(cx);
            });
        app.update(cx, |this, cx| {
            let group = this.group_type_facts().is_none_or(|facts| facts.group);
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(if group {
                    tr(keys::MANAGE_PEER_GROUP_TYPE)
                } else {
                    tr(keys::MANAGE_PEER_CHANNEL_TYPE)
                }));
            if this.group_type_box().is_none() {
                return dialog.on_close(on_close);
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("group-type-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_group_type_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::GroupTypeBox, window, cx);
                        })),
                )
                .child(
                    Button::new("group-type-save")
                        .label(tr(keys::SETTINGS_SAVE))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_group_type_box(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::GroupTypeBox, window, cx);
                        })),
                );
            let body_app = cx.entity();
            dialog
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, cx| {
                        let body = body_app
                            .update(cx, |this, cx| this.group_type_box_body(cx))
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// The answer the box last got for exactly this text.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(crate) fn group_type_link_status(&self, cx: &App) -> Option<LinkStatus> {
        let facts = self.group_type_facts()?;
        Some(self.group_type_box()?.status(self, &facts, cx))
    }
}

crate::ui::shell::register_dialogs! {
    GroupTypeBox => DialogSpec::new(
        7650,
        |app| app.group_type_box_dialog_open(),
        QuillApp::build_group_type_box_dialog,
    ),
}

mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod ui_tests;
