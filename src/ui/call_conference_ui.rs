//! Conference calls in the call window (docs/decisions/tgcalls-conference-calls.md):
//! - "Add People" in an established 1:1 call opens tdesktop's invite box
//!   (`Group::PrepareInviteBox`): the other side is already in, contacts
//!   are picked, or "Create Call Link" makes the conference with only its
//!   link; either way the call moves into a new conference call.
//! - An incoming conference invitation rings in the call window as
//!   tdesktop's `conferenceInvite()` call does: "Telegram Group Call",
//!   who else joined, Decline or Join.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::call_panel::{CallPhase, CallSnap};
use super::link_info_boxes::finish;
use super::screen_chooser::bold_marked;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_base::{Disableable, StyledExt};
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::state::Session;
use quill::telegram::envelope::CallState;

/// Contacts offered in the box, as the group call invite picker offers.
const CONTACTS_SHOWN: usize = 30;

/// The call window's key for a ringing invitation: never a call id (those
/// are positive), and stable for the invitation's message.
pub(super) fn ringing_invite_key(session: &Session) -> Option<i32> {
    let invite = session
        .calls
        .conference_invite
        .as_ref()
        .filter(|invite| !invite.busy)?;
    let server_id = (invite.message_id.0 >> 20) & 0x3fff_ffff;
    Some(-i32::try_from(server_id).unwrap_or(0) - 1)
}

/// tdesktop `lng_confcall_already_joined_*`: up to three names, then a
/// count.
fn joined_line(names: &[String]) -> Option<String> {
    let name = |index: usize| names.get(index).map(String::as_str).unwrap_or_default();
    Some(match names.len() {
        0 => return None,
        1 => tr_with(keys::CONFCALL_ALREADY_JOINED_ONE, &[("user", name(0))]),
        2 => tr_with(
            keys::CONFCALL_ALREADY_JOINED_TWO,
            &[("user", name(0)), ("other", name(1))],
        ),
        3 => tr_with(
            keys::CONFCALL_ALREADY_JOINED_THREE,
            &[("user", name(0)), ("other", name(1)), ("third", name(2))],
        ),
        count => trn_with(
            keys::CONFCALL_ALREADY_JOINED_MANY,
            count - 2,
            &[("user", name(0)), ("other", name(1))],
        ),
    })
}

impl QuillApp {
    fn user_name(&self, user_id: i64) -> String {
        self.session()
            .and_then(|session| session.user(user_id))
            .map(|user| user.display_name())
            .unwrap_or_else(|| tr(keys::Q_OPS_TELEGRAM_USER).into())
    }

    /// The call window for the ringing invitation.
    pub(super) fn conference_invite_snapshot(&mut self) -> Option<CallSnap> {
        let session = self.session()?;
        let invite = session
            .calls
            .conference_invite
            .clone()
            .filter(|invite| !invite.busy)?;
        let photo = session.user_photo_path(invite.user_id).and_then(|path| {
            quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
        });
        let others: Vec<String> = invite
            .others
            .iter()
            .map(|user_id| self.user_name(*user_id))
            .collect();
        let name = self.user_name(invite.user_id);
        Some(CallSnap {
            user_id: invite.user_id,
            is_video: invite.is_video,
            joined_line: joined_line(&others).or_else(|| {
                Some(tr_with(
                    keys::CONFCALL_JOIN_TEXT_INVITER,
                    &[("user", &name)],
                ))
            }),
            name,
            photo,
            phase: CallPhase::ConferenceInvite,
            status: tr(keys::CALL_STATUS_GROUP_INVITE).into(),
            connecting: false,
            muted: false,
            camera_on: false,
            screen_sharing: false,
            can_share_screen: false,
            emojis: Vec::new(),
            remote_muted: false,
            remote_battery_low: false,
            signal_bars: None,
            video: false,
            preview: false,
            can_add_people: false,
        })
    }

    /// Join the ringing invitation.
    pub(super) fn accept_conference_invite(&mut self, cx: &mut Context<Self>) {
        let Some(invite) = self
            .session()
            .and_then(|session| session.calls.conference_invite.clone())
        else {
            return;
        };
        self.accept_group_call_invitation(invite.chat_id, invite.message_id, cx);
    }

    /// Decline the ringing invitation.
    pub(super) fn decline_conference_invite(&mut self, cx: &mut Context<Self>) {
        let Some(invite) = self
            .session()
            .and_then(|session| session.calls.conference_invite.clone())
        else {
            return;
        };
        self.decline_group_call_invitation(invite.chat_id, invite.message_id, cx);
    }

    /// "Add People" (tdesktop `Panel::addPeople`): only once connected.
    pub(super) fn open_call_add_people(&mut self, cx: &mut Context<Self>) {
        let established = self
            .session()
            .and_then(|session| session.calls.active_call.as_ref())
            .is_some_and(|call| matches!(call.state, CallState::Ready));
        if !established {
            self.connection
                .set_note(StatusNote::info(tr(keys::CALL_ERROR_ADD_NOT_STARTED)));
            cx.notify();
            return;
        }
        self.calls.add_people = Some(Vec::new());
        if let Some(live) = self.live.as_mut()
            && live.driver.session.users_state.contacts.is_none()
        {
            let _ = live.driver.fetch_contacts();
        }
        cx.notify();
    }

    fn toggle_call_add_person(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(picked) = self.calls.add_people.as_mut() {
            if let Some(at) = picked.iter().position(|id| *id == user_id) {
                picked.remove(at);
            } else {
                picked.push(user_id);
            }
        }
        cx.notify();
    }

    /// The box's Invite, or "Create Call Link" with nobody picked: the
    /// call moves into a new conference call.
    fn finish_call_add_people(&mut self, share_link: bool, cx: &mut Context<Self>) {
        let Some(picked) = self.calls.add_people.take() else {
            return;
        };
        let invite = if share_link { Vec::new() } else { picked };
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .upgrade_call_to_conference(invite, share_link)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_AV_COULDNT_JOIN_THE_VOICE_CHAT,
                )));
            }
        } else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_AV_VOICE_CHATS_NEED_A_LIVE)));
        }
        cx.notify();
    }

    /// The "Add People" box over the call window.
    pub(super) fn call_add_people_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let picked = self.calls.add_people.as_ref()?;
        let session = self.session()?;
        let peer = session.calls.active_call.as_ref()?.user_id;
        let contacts: Vec<i64> = session
            .users_state
            .contacts
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter(|user_id| *user_id != peer)
            .take(CONTACTS_SHOWN)
            .collect();
        let mut rows = div()
            .id("call-add-people-rows")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(240.))
            .overflow_y_scroll()
            // The other side is in the call already (tdesktop `alreadyIn`).
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr_with(
                        keys::CONFCALL_INVITE_ALREADY_USER,
                        &[("user", &self.user_name(peer))],
                    )),
            );
        for user_id in contacts {
            let checked = picked.contains(&user_id);
            rows = rows.child(
                Checkbox::new(("call-add-person", user_id as u64))
                    .label(self.user_name(user_id))
                    .checked(checked)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_call_add_person(user_id, cx);
                    })),
            );
        }
        let none_picked = picked.is_empty();
        Some(
            div()
                .id("call-add-people")
                .absolute()
                .inset_0()
                .bg(cx.theme().overlay)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(340.))
                        .rounded(cx.theme().radius_lg)
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().background)
                        .text_color(cx.theme().foreground)
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .text_base()
                                .font_semibold()
                                .child(tr(keys::GROUP_CALL_INVITE_CONF)),
                        )
                        .child(
                            Button::new("call-add-people-link")
                                .label(tr(keys::CONFCALL_CREATE_LINK))
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.finish_call_add_people(true, cx);
                                })),
                        )
                        .child(rows)
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("call-add-people-cancel")
                                        .label(tr(keys::CANCEL))
                                        .ghost()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.calls.add_people = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("call-add-people-invite")
                                        .label(tr(keys::GROUP_CALL_INVITE_CONF))
                                        .primary()
                                        .disabled(none_picked)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.finish_call_add_people(false, cx);
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    /// A conference call's link in the invite panel, with "Copy Link"
    /// (tdesktop's invite box for a conference starts with the link).
    pub(super) fn conference_link_row(
        &self,
        call: &quill::state::ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !call.is_conference() || call.invite_link.is_empty() {
            return None;
        }
        let link = call.invite_link.clone();
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::CONFCALL_LINK_TITLE)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_sm().truncate().child(link.clone()))
                        .child(
                            Button::new("conference-copy-link")
                                .label(tr(keys::GROUP_INVITE_COPY))
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                                    this.connection.set_note(StatusNote::silent(tr(
                                        super::group_call_share::LINK_COPIED,
                                    )));
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The invite panel was closed: a conference's "Share link" is done.
    pub(super) fn close_conference_link(&mut self) {
        if let Some(conference) = self
            .live
            .as_mut()
            .and_then(|live| live.driver.session.calls.active_group_call.as_mut())
            .and_then(|call| call.conference.as_mut())
        {
            conference.link_open = false;
        }
    }
}

impl QuillApp {
    /// "Join Group Call" on a conference call's link.
    fn join_conference_link(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.calls.conference_link.take() else {
            return;
        };
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.join_conference_link(link).is_err() {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_AV_COULDNT_JOIN_THE_VOICE_CHAT,
                    )));
                }
            }
            None => self
                .connection
                .set_note(StatusNote::info(tr(keys::Q_AV_VOICE_CHATS_NEED_A_LIVE))),
        }
        cx.notify();
    }

    /// tdesktop's confirmation for a call link (`lng_confcall_join_*`).
    pub(super) fn build_conference_link_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ConferenceLink, |this, _, cx| {
                this.calls.conference_link = None;
                cx.notify();
            });
        app.update(cx, |_, cx| {
            let body = div()
                .id("conference-link-text")
                .text_sm()
                .child(tr(keys::CONFCALL_JOIN_TEXT))
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("conference-link-cancel")
                        .label(tr(keys::CALL_CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.calls.conference_link = None;
                            this.close_kit_dialog_if_done(DialogKind::ConferenceLink, window, cx);
                        })),
                )
                .child(
                    Button::new("conference-link-join")
                        .label(tr(keys::CONFCALL_JOIN_BUTTON))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.join_conference_link(cx);
                            this.close_kit_dialog_if_done(DialogKind::ConferenceLink, window, cx);
                        })),
                )
                .into_any_element();
            finish(
                dialog,
                tr(keys::CONFCALL_JOIN_TITLE),
                body,
                footer,
                on_close,
            )
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// A conference call's link: join it?
    ConferenceLink => DialogSpec::new(
        2961,
        |app| app.calls.conference_link.is_some(),
        QuillApp::build_conference_link_dialog,
    ),
}

/// A conference call's encryption under its title (tdesktop
/// `lng_confcall_e2e_badge_small` and the emoji it shows once verified):
/// the four emoji every participant should see the same.
pub(super) fn e2e_badge(call: &quill::state::ActiveGroupCall) -> Option<AnyElement> {
    if !call.is_conference() {
        return None;
    }
    let emojis = call
        .verification
        .as_ref()
        .map(|state| state.emojis.clone())
        .unwrap_or_default();
    let tip: SharedString = tr(keys::CONFCALL_E2E_ABOUT).into();
    let pill = div()
        .id("conference-e2e")
        .mt(px(4.))
        .px(px(8.))
        .py(px(2.))
        .rounded_full()
        .bg(crate::ui::app_theme::call_chrome::pill())
        .flex()
        .gap(px(4.))
        .text_size(px(12.))
        .text_color(crate::ui::app_theme::call_chrome::text())
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
        });
    let pill = if emojis.is_empty() {
        pill.child(tr(keys::CONFCALL_E2E_BADGE_SMALL))
    } else {
        pill.children(emojis.into_iter().map(|emoji| div().child(emoji)))
    };
    Some(pill.into_any_element())
}

/// tdesktop's `**bold**` count in "already joined", as on its panel.
pub(super) fn joined_line_element(line: &str) -> Div {
    bold_marked(line)
}

#[cfg(test)]
mod tests {
    use super::joined_line;

    #[test]
    fn up_to_three_names_then_a_count() {
        let names = |n: usize| (0..n).map(|i| format!("P{i}")).collect::<Vec<_>>();
        assert_eq!(joined_line(&names(0)), None);
        assert_eq!(
            joined_line(&names(1)).as_deref(),
            Some("P0 already joined this call.")
        );
        assert_eq!(
            joined_line(&names(3)).as_deref(),
            Some("P0, P1 and P2 already joined this call.")
        );
        assert_eq!(
            joined_line(&names(5)).as_deref(),
            Some("P0, P1 and **3** other people already joined this call.")
        );
    }
}
