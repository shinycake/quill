//! The video chat's "Share Invite Link" box (tdesktop `ShareInviteLinkBox`,
//! calls/group/calls_group_settings.cpp) and the members list's invite row
//! routing (`Members::setupAddMember`, `Panel` `addMembersRequests`).
//!
//! A public chat's video chat has a listener link and, for whoever can
//! manage the call, a speaker link. The box opens once both answers are in
//! (`quill::calls::share_links`): "Users with this link can speak" picks
//! which link "Copy Speaker Link" / "Copy Listener Link" copies, and a chat
//! in "Share to" gets the link in its message box, unsent, as every other
//! link Quill shares (`DeepLinkAction::ShareDraft`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::share_links::{InviteRowAction, InviteRowFacts, copy_label, invite_row_action};
use quill::ids::ChatId;
use quill::send_rights::{SendKind, SendRights};
use quill::state::{ActiveGroupCall, ChatSummary, DeepLinkAction};
use quill::telegram::envelope::{ChatKind, MessageSender};

/// Chats offered under "Share to", as in the link share chooser.
const SHARE_CHOICES: usize = 14;

/// tdesktop `lng_group_invite_copied`.
pub(super) const LINK_COPIED: &str = "Invite link copied to clipboard.";

impl QuillApp {
    /// The chat a video chat belongs to.
    fn group_call_chat(&self, call: &ActiveGroupCall) -> Option<&ChatSummary> {
        let session = self.session()?;
        session.chats.values().find(|chat| {
            chat.video_chat
                .as_ref()
                .is_some_and(|video_chat| video_chat.group_call_id == call.id)
        })
    }

    /// What tdesktop reads to decide the invite row and the share link.
    pub(super) fn group_call_link_facts(&self, call: &ActiveGroupCall) -> InviteRowFacts {
        let chat = self.group_call_chat(call);
        let has_username = chat.is_some_and(|chat| match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => self
                .session()
                .and_then(|session| session.supergroup_username(supergroup_id))
                .is_some_and(|name| !name.is_empty()),
            _ => false,
        });
        let can_send_other = match (self.session(), chat) {
            (Some(session), Some(chat)) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as i64);
                SendRights::for_chat(session, chat, now).allows(SendKind::Stickers)
            }
            _ => false,
        };
        let joined_as_self = match call.participants.iter().find(|p| p.is_current_user) {
            Some(me) => matches!(me.participant_id, MessageSender::User { .. }),
            None => !matches!(call.join_as, Some(MessageSender::Chat { .. })),
        };
        InviteRowFacts {
            is_video_chat: call.is_video_chat,
            is_broadcast: chat.is_some_and(|chat| chat.kind.is_channel()),
            has_username,
            can_send_other,
            joined_as_self,
        }
    }

    /// "Share Invite Link" is offered for video chats in public chats:
    /// TDLib's `getVideoChatInviteLink` works only there.
    pub(super) fn group_call_can_share_link(&self, call: &ActiveGroupCall) -> bool {
        call.is_video_chat && self.group_call_link_facts(call).has_username
    }

    /// The invite row's action, or `None` when there is no row.
    pub(super) fn group_call_invite_row(&self, call: &ActiveGroupCall) -> Option<InviteRowAction> {
        invite_row_action(self.group_call_link_facts(call))
    }

    /// Bring the main window forward, past the call windows (its passcode
    /// screen, or the chat a link was shared to).
    fn raise_main_window(&self, cx: &mut Context<Self>) {
        let call_windows = [self.group_call.window, self.calls.window];
        for window in cx.windows() {
            if !call_windows.contains(&Some(window)) {
                let _ = window.update(cx, |_, window, _| window.activate_window());
            }
        }
    }

    /// While the passcode lock is on, the invite picker and the share box
    /// wait for the unlock in the main window (tdesktop
    /// `ShowUnlockPasscodeBox` before `addMembers` and the share action).
    pub(super) fn group_call_needs_unlock(&self, cx: &mut Context<Self>) -> bool {
        if !self.account.passcode.locked {
            return false;
        }
        self.raise_main_window(cx);
        true
    }

    /// A click on "Invite Members".
    pub(super) fn group_call_invite_row_clicked(&mut self, cx: &mut Context<Self>) {
        let Some(call) = self
            .session()
            .and_then(|session| session.calls.active_group_call.clone())
        else {
            return;
        };
        match self.group_call_invite_row(&call) {
            Some(InviteRowAction::Invite) => self.open_group_call_invite(cx),
            Some(InviteRowAction::ShareLink) => self.open_group_call_share_links(cx),
            None => {}
        }
    }

    /// "Share Invite Link": ask for the links; the box shows once both
    /// are in (tdesktop `ShareInviteLinkAction`).
    pub(super) fn open_group_call_share_links(&mut self, cx: &mut Context<Self>) {
        let asked = match self.live.as_mut() {
            Some(live) => live.driver.request_video_chat_share_links().is_ok(),
            // Screenshot demos and tests carry their links in the session.
            None => self.demo_session.is_some(),
        };
        if asked {
            self.group_call_needs_unlock(cx);
            self.group_call.share_open = true;
            self.group_call.share_speaker = true;
            self.group_call.share_copied = false;
        } else {
            self.connection.status_note = "Couldn't get the invite link.".into();
        }
        cx.notify();
    }

    pub(super) fn close_group_call_share_links(&mut self, cx: &mut Context<Self>) {
        self.group_call.share_open = false;
        cx.notify();
    }

    fn group_call_share_link(&self) -> Option<String> {
        let call = self.session()?.calls.active_group_call.as_ref()?;
        call.share_links
            .current(self.group_call.share_speaker)
            .map(str::to_string)
    }

    /// "Copy … Link": the box stays open (tdesktop `ShareBox::copyLink`).
    pub(super) fn copy_group_call_share_link(&mut self, cx: &mut Context<Self>) {
        if let Some(link) = self.group_call_share_link() {
            cx.write_to_clipboard(ClipboardItem::new_string(link));
            self.group_call.share_copied = true;
        }
        cx.notify();
    }

    /// A chat in "Share to": open it in the main window with the link in
    /// its message box.
    pub(super) fn share_group_call_link_to(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(text) = self.group_call_share_link() else {
            return;
        };
        self.links.pending_deep_link_open = Some((chat_id, DeepLinkAction::ShareDraft { text }));
        self.group_call.share_open = false;
        self.raise_main_window(cx);
        cx.notify();
    }

    /// The box over the call window, once both links are known.
    pub(super) fn group_call_share_overlay(
        &self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.group_call.share_open || !call.share_links.ready() || self.account.passcode.locked
        {
            return None;
        }
        let links = &call.share_links;
        let speaker_checked = self.group_call.share_speaker;
        let link = links
            .current(speaker_checked)
            .unwrap_or_default()
            .to_string();
        let choices: Vec<(ChatId, String, Option<std::path::PathBuf>)> = self
            .session()
            .map(|session| {
                session
                    .forward_destinations("")
                    .into_iter()
                    .take(SHARE_CHOICES)
                    .map(|chat| {
                        let photo = session.chat_photo_path(chat.id).and_then(|path| {
                            quill::local_path::sandboxed_display_path(
                                path,
                                &self.media_display_roots(),
                            )
                        });
                        (chat.id, chat.title.clone(), photo)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let muted = cx.theme().muted_foreground;
        let mut list = div()
            .id("group-call-share-chats")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(220.))
            .overflow_y_scroll();
        if choices.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("No chats loaded yet."),
            );
        }
        for (id, title, photo) in choices {
            list = list.child(
                div()
                    .id(("group-call-share-chat", id.0 as u64))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(cx.theme().muted))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.share_group_call_link_to(id, cx);
                    }))
                    .child(chat_avatar(&title, photo.as_deref(), 28.))
                    .child(div().flex_1().min_w_0().truncate().text_sm().child(title)),
            );
        }
        let card = div()
            .id("group-call-share-box")
            .w(px(340.))
            .max_h(relative(0.9))
            .rounded(px(12.))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Share to"))
            .child(list)
            .child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().muted.opacity(0.5))
                    .text_sm()
                    .truncate()
                    .child(link),
            )
            .when(links.speaker_link().is_some(), |card| {
                card.child(
                    Checkbox::new("group-call-share-speaker")
                        .label("Users with this link can speak")
                        .checked(speaker_checked)
                        .on_click(cx.listener(|this, &on, _, cx| {
                            this.group_call.share_speaker = on;
                            this.group_call.share_copied = false;
                            cx.notify();
                        })),
                )
            })
            .when(self.group_call.share_copied, |card| {
                card.child(
                    div()
                        .id("group-call-share-copied")
                        .text_xs()
                        .text_color(muted)
                        .child(LINK_COPIED),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("group-call-share-cancel")
                            .label("Cancel")
                            .ghost()
                            .small()
                            .on_click(
                                cx.listener(|this, _, _, cx| this.close_group_call_share_links(cx)),
                            ),
                    )
                    .child(
                        Button::new("group-call-share-copy")
                            .label(copy_label(links, speaker_checked))
                            .primary()
                            .small()
                            .on_click(
                                cx.listener(|this, _, _, cx| this.copy_group_call_share_link(cx)),
                            ),
                    ),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .bg(hsla(0., 0., 0., 0.5))
                .flex()
                .items_center()
                .justify_center()
                .child(card)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_call_share_tests.rs"]
mod tests;
