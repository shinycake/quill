//! Transfer ownership and "owner leaves" dialogs. Mirrors tdesktop's
//! `boxes/select_future_owner_box.cpp` (leaving as owner: who inherits,
//! appoint someone else) and `boxes/peers/channel_ownership_transfer.cpp`
//! (security check, then the 2-step verification password). The password is
//! typed into a masked field and sent only to TDLib; it is cleared from the
//! field as soon as it is sent and never stored or logged.

use super::app::QuillApp;
use super::dialogs::{OwnershipDialog, OwnershipStage};
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputContentType};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::moderation::{GroupFlavor, OwnerCandidate, owner_choices};
use quill::state::{MemberListFilter, OwnerLookup, SupergroupMembersFetch, transfer_block_reason};
use quill::telegram::envelope::{CanTransferOwnershipResult, MessageSender, ParsedChatMember};
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    /// Chats the transfer applies to: basic groups, supergroups and
    /// channels the viewer owns.
    fn can_offer_ownership(&self, chat_id: ChatId) -> bool {
        self.group_flavor(chat_id).is_some()
            && self.session().is_some_and(|s| s.chat_is_owner(chat_id))
    }

    /// Group info "Transfer ownership": pick the new owner, then confirm
    /// with the password.
    pub(super) fn open_transfer_ownership(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_offer_ownership(chat_id) {
            return;
        }
        self.open_ownership_dialog(
            chat_id,
            OwnershipStage::Pick { leave_after: false },
            window,
            cx,
        );
    }

    /// "Leave group" as the owner: tdesktop shows who inherits first.
    /// Returns false when the viewer is not an owner, so the plain leave
    /// confirmation applies.
    pub(super) fn open_owner_leave(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_offer_ownership(chat_id) {
            return false;
        }
        self.open_ownership_dialog(chat_id, OwnershipStage::Leave, window, cx);
        true
    }

    fn open_ownership_dialog(
        &mut self,
        chat_id: ChatId,
        stage: OwnershipStage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ownership_dialog = Some(OwnershipDialog::new(window, cx, chat_id, stage));
        if let Some(live) = self.live.as_mut() {
            let session = &mut live.driver.session;
            session.ownership.transfer_error = None;
            session.ownership.transferred = None;
            // Asked once per dialog so a stale answer never decides.
            session.ownership.owner_after_leaving.remove(&chat_id.0);
            let _ = live.driver.check_can_transfer_ownership();
            let _ = live.driver.fetch_chat_owner_after_leaving(chat_id);
            let _ = live.driver.refresh_supergroup_members(
                chat_id,
                MemberListFilter::Administrators,
                "",
            );
            let _ = live
                .driver
                .refresh_supergroup_members(chat_id, MemberListFilter::Recent, "");
            let _ = live.driver.fetch_basic_group_members(chat_id);
        }
        cx.notify();
    }

    pub(super) fn close_ownership_dialog(&mut self, cx: &mut Context<Self>) {
        self.ownership_dialog = None;
        cx.notify();
    }

    /// The people the new owner may be: admins, then members.
    fn ownership_candidates(&self, chat_id: ChatId) -> Vec<(i64, bool)> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let mut pool: Vec<ParsedChatMember> = Vec::new();
        for filter in [MemberListFilter::Administrators, MemberListFilter::Recent] {
            if let Some(SupergroupMembersFetch::Loaded { members, .. }) =
                session.supergroup_members.get(&(chat_id.0, filter))
            {
                pool.extend(members.iter().cloned());
            }
        }
        if let Some(SupergroupMembersFetch::Loaded { members, .. }) =
            session.basic_group_members.get(&chat_id.0)
        {
            pool.extend(members.iter().cloned());
        }
        let candidates: Vec<OwnerCandidate> = pool
            .iter()
            .filter_map(|member| match member.member_id {
                MessageSender::User { user_id } => Some(OwnerCandidate {
                    user_id,
                    status: member.status,
                    is_bot: session.is_bot_user(user_id),
                }),
                MessageSender::Chat { .. } => None,
            })
            .collect();
        owner_choices(&candidates, session.my_user_id)
    }

    fn set_ownership_stage(&mut self, stage: OwnershipStage, cx: &mut Context<Self>) {
        if let Some(dialog) = self.ownership_dialog.as_mut() {
            dialog.stage = stage;
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.session.ownership.transfer_error = None;
        }
        cx.notify();
    }

    /// "Change owner": send the password with the transfer. The field is
    /// emptied the moment the request is handed to TDLib.
    fn submit_ownership_transfer(
        &mut self,
        user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((chat_id, input)) = self
            .ownership_dialog
            .as_ref()
            .map(|d| (d.chat_id, d.password_input.clone()))
        else {
            return;
        };
        let password = input.read(cx).value().to_string();
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .transfer_chat_ownership(chat_id, user_id, &password)
        });
        input.update(cx, |input, cx| input.set_value("", window, cx));
        drop(password);
        self.status_note = match sent {
            Some(Ok(Some(_))) => "transferring ownership…".into(),
            Some(_) => "ownership can't be transferred right now".into(),
            None => "transferring ownership needs a live connection (demo)".into(),
        };
        cx.notify();
    }

    /// Leave after appointing someone, or the plain "Leave" of the owner
    /// box (the future owner takes over per TDLib's own rule).
    fn leave_after_ownership(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.leave_channel(chat_id, cx);
        self.close_ownership_dialog(cx);
        self.close_kit_dialog_if_done(DialogKind::Ownership, window, cx);
    }

    /// Drained from the poll loop: a finished transfer closes the dialog
    /// and, for "appoint and leave", leaves the chat.
    pub(super) fn finish_ownership_transfer_ui(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(done) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.ownership.transferred.take())
        else {
            return false;
        };
        let leave_after = matches!(
            self.ownership_dialog.as_ref().map(|d| d.stage),
            Some(OwnershipStage::Confirm {
                leave_after: true,
                ..
            })
        );
        let name = self.contact_display_name(done.user_id);
        let place = if self.group_flavor(ChatId(done.chat_id)) == Some(GroupFlavor::Channel) {
            "channel"
        } else {
            "group"
        };
        self.status_note = format!("{name} is now the owner of the {place}.");
        self.ownership_dialog = None;
        if leave_after {
            self.leave_channel(ChatId(done.chat_id), cx);
        }
        cx.notify();
        true
    }

    pub(super) fn build_ownership_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Ownership, |this, _, cx| {
                this.close_ownership_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some((chat_id, stage, password_input)) = this
                .ownership_dialog
                .as_ref()
                .map(|d| (d.chat_id, d.stage, d.password_input.clone()))
            else {
                return dialog
                    .title(crate::ui::shell::dialog_title("Transfer ownership"))
                    .on_close(on_close);
            };
            let flavor = this.group_flavor(chat_id).unwrap_or(GroupFlavor::Supergroup);
            let (place, legacy) = match flavor {
                GroupFlavor::Channel => ("channel", false),
                GroupFlavor::Supergroup => ("group", false),
                GroupFlavor::BasicGroup => ("group", true),
            };
            let chat_title = this
                .session()
                .and_then(|s| s.chats.get(&chat_id.0))
                .map(|c| c.title.clone())
                .unwrap_or_default();
            let ownership = this
                .session()
                .map(|s| s.ownership.clone())
                .unwrap_or_default();
            let muted = cx.theme().muted_foreground;
            let mut body = div().flex().flex_col().gap_3();
            let mut footer = div().flex().justify_end().gap_2();
            let title: String;
            match stage {
                OwnershipStage::Leave => {
                    title = format!("Leave {}?", if place == "channel" { "Channel" } else { "Group" });
                    let line = match ownership.owner_after_leaving.get(&chat_id.0) {
                        Some(OwnerLookup::Loaded(user_id)) => {
                            let name = this.contact_display_name(*user_id);
                            let when = if legacy { "immediately" } else { "in 1 week" };
                            format!(
                                "If you leave, {name} will become the new owner of {chat_title} {when}."
                            )
                        }
                        Some(OwnerLookup::Failed(message)) => message.clone(),
                        _ => "Finding out who becomes the next owner…".to_string(),
                    };
                    body = body.child(div().text_sm().text_color(muted).child(line));
                    footer = footer
                        .child(
                            Button::new("own-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_ownership_dialog(cx);
                                    this.close_kit_dialog_if_done(DialogKind::Ownership, window, cx);
                                })),
                        )
                        .child(
                            Button::new("own-appoint")
                                .label("Appoint Another Owner")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_ownership_stage(
                                        OwnershipStage::Pick { leave_after: true },
                                        cx,
                                    );
                                    this.close_kit_dialog_if_done(DialogKind::Ownership, window, cx);
                                })),
                        )
                        .child(
                            Button::new("own-leave")
                                .label(format!("Leave {}", if place == "channel" { "Channel" } else { "Group" }))
                                .danger()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.leave_after_ownership(chat_id, window, cx);
                                })),
                        );
                }
                OwnershipStage::Pick { leave_after } => {
                    title = if leave_after {
                        "Appoint New Owner".to_string()
                    } else {
                        format!("Transfer {place} ownership")
                    };
                    let choices = this.ownership_candidates(chat_id);
                    let mut list = div()
                        .id("own-candidates")
                        .flex()
                        .flex_col()
                        .gap_1()
                        .max_h(px(260.))
                        .overflow_y_scroll();
                    if choices.is_empty() {
                        list = list.child(div().text_sm().text_color(muted).child(
                            "There are no eligible participants to appoint as owner.",
                        ));
                    }
                    for (user_id, is_admin) in choices.into_iter().take(100) {
                        let name = this.contact_display_name(user_id);
                        list = list.child(
                            div()
                                .id(("own-candidate", user_id as u64))
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|style| style.bg(cx.theme().accent))
                                .child(div().flex_1().text_sm().child(name))
                                .child(div().text_xs().text_color(muted).child(if is_admin {
                                    "admin"
                                } else {
                                    "member"
                                }))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_ownership_stage(
                                        OwnershipStage::Confirm {
                                            user_id,
                                            leave_after,
                                        },
                                        cx,
                                    );
                                    this.close_kit_dialog_if_done(DialogKind::Ownership, window, cx);
                                })),
                        );
                    }
                    body = body.child(list);
                    footer = footer.child(
                        Button::new("own-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_ownership_dialog(cx);
                                this.close_kit_dialog_if_done(DialogKind::Ownership, window, cx);
                            })),
                    );
                }
                OwnershipStage::Confirm { user_id, leave_after } => {
                    title = "Change owner".to_string();
                    let name = this.contact_display_name(user_id);
                    let back = OwnershipStage::Pick { leave_after };
                    let blocked = ownership
                        .can_transfer
                        .and_then(transfer_block_reason)
                        .or_else(|| ownership.check_error.clone());
                    let ready = ownership.can_transfer == Some(CanTransferOwnershipResult::Ok);
                    let in_flight = ownership.transfer_in_flight.is_some();
                    if let Some(reason) = blocked {
                        body = body
                            .child(div().text_sm().font_semibold().child("Security check"))
                            .child(div().text_sm().text_color(muted).child(format!(
                                "You can transfer this {place} to {name} only if you have enabled Two-Step Verification more than 7 days ago and logged in on this device more than 24 hours ago."
                            )))
                            .child(div().text_sm().text_color(cx.theme().danger).child(reason));
                    } else if !ready {
                        body = body.child(div().text_sm().text_color(muted).child("Checking…"));
                    } else {
                        body = body
                            .child(div().text_sm().text_color(muted).child(format!(
                                "This will transfer the full owner rights for {chat_title} to {name}. The new owner will be free to remove any of your admin privileges or even ban you."
                            )))
                            .child(div().text_sm().text_color(muted).child(
                                "Please enter your Two-Step Verification password to complete the transfer.",
                            ))
                            .child(
                                Input::new(&password_input)
                                    .aria_label("Two-step verification password")
                                    .content_type(InputContentType::Password),
                            );
                    }
                    if let Some(error) = ownership.transfer_error.clone() {
                        body = body.child(div().text_sm().text_color(cx.theme().danger).child(error));
                    }
                    footer = footer
                        .child(
                            Button::new("own-back")
                                .label("Back")
                                .ghost()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_ownership_stage(back, cx);
                                    this.close_kit_dialog_if_done(DialogKind::Ownership, window, cx);
                                })),
                        )
                        .when(ready, |footer| {
                            footer.child(
                                Button::new("own-submit")
                                    .label(if in_flight { "Transferring…" } else { "Change owner" })
                                    .danger()
                                    .disabled(in_flight)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.submit_ownership_transfer(user_id, window, cx);
                                        this.close_kit_dialog_if_done(DialogKind::Ownership, window, cx);
                                    })),
                            )
                        });
                }
            }
            let body = body.into_any_element();
            dialog
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// Transfer ownership / the owner's leave box.
    Ownership => DialogSpec::new(
        4700,
        |app| app.ownership_dialog.is_some(),
        QuillApp::build_ownership_dialog,
    ),
}
