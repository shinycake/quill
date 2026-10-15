//! `ChooseJoinAsBox` for a new video chat or live stream (tdesktop
//! calls/group/calls_choose_join_as.cpp:186-266, `Context::Create`): the
//! about line, one row per identity you may appear as (photo, name, and
//! "personal account" or the member count), the selected one marked, and
//! for a group or channel you administer the "You can also schedule ..."
//! link. Continue creates the call as the selected identity.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::group_call_create::or_schedule_link;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::radio::Radio;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::earn::EarnPeer;
use quill::ids::ChatId;
use quill::lang::{keys, tr, trn};
use quill::telegram::envelope::{ChatKind, MessageSender};

/// Element id suffix of a row.
fn row_key(sender: MessageSender) -> String {
    match sender {
        MessageSender::User { user_id } => format!("u{user_id}"),
        MessageSender::Chat { chat_id } => format!("c{chat_id}"),
    }
}

impl QuillApp {
    /// The box's body (the title and the Continue / Cancel footer come
    /// from the shared create dialog).
    pub(super) fn join_as_body(
        &self,
        chat_id: ChatId,
        channel: bool,
        options: &[MessageSender],
        selected: MessageSender,
        cx: &mut Context<QuillApp>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let me = session.my_user_id;
        let mut rows = div().flex().flex_col().gap_1();
        for sender in options.iter().copied() {
            let key = row_key(sender);
            let checked = sender == selected;
            let name = self.group_call_participant_name(&sender);
            let (peer, status) = match sender {
                MessageSender::User { user_id } => (
                    EarnPeer::User(user_id),
                    (Some(user_id) == me)
                        .then(|| tr(keys::GROUP_CALL_JOIN_AS_PERSONAL).to_string()),
                ),
                MessageSender::Chat { chat_id } => (
                    EarnPeer::Chat(chat_id),
                    session
                        .chats
                        .get(&chat_id)
                        .and_then(|chat| match chat.kind {
                            ChatKind::Supergroup {
                                supergroup_id,
                                is_channel,
                            } => session
                                .groups
                                .supergroup_member_counts
                                .get(&supergroup_id)
                                .map(|count| {
                                    if is_channel {
                                        trn(keys::CHAT_STATUS_SUBSCRIBERS, *count)
                                    } else {
                                        trn(keys::CHAT_STATUS_MEMBERS, *count)
                                    }
                                }),
                            _ => None,
                        }),
                ),
            };
            let select = cx.listener(move |this, _, _, cx| this.select_create_join_as(sender, cx));
            let row_id = format!("group-call-join-as-{key}");
            let selector = row_id.clone();
            rows =
                rows.child(
                    div()
                        .id(SharedString::from(row_id))
                        .debug_selector(move || selector.clone())
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_2()
                        .py_1p5()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|style| style.bg(cx.theme().secondary))
                        .on_click(select)
                        .child(self.earn_avatar(session, peer, 36.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .truncate()
                                        .child(name),
                                )
                                .when_some(status, |this, status| {
                                    this.child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(status),
                                    )
                                }),
                        )
                        .child(
                            Radio::new(SharedString::from(format!(
                                "group-call-join-as-radio-{key}"
                            )))
                            .checked(checked)
                            .accessibility_label(self.group_call_participant_name(&sender))
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.select_create_join_as(sender, cx),
                            )),
                        ),
                );
        }
        let admin = matches!(
            session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) && session.chat_can_manage_video_chats(chat_id);
        div()
            .flex()
            .flex_col()
            .gap_3()
            .text_sm()
            .child(
                div()
                    .id("group-call-join-as-about")
                    .debug_selector(|| "group-call-join-as-about".into())
                    .whitespace_normal()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::GROUP_CALL_JOIN_AS_ABOUT)),
            )
            .child(rows)
            .when(admin, |this| {
                this.child(or_schedule_link(chat_id, channel, cx))
            })
            .into_any_element()
    }
}
