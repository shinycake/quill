//! The community info panel's TDLib 1.8.70 part (tdesktop
//! `SetupCommunityContent`, `SetupCommunityEditChatsList`): "Show as One
//! Chat", the pending requests row, the chats in three sections, and
//! "Add a Chat to Community".
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::pressable::action_row;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use quill::telegram::envelope::ParsedCommunityChat;

/// Which section a community chat sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatSection {
    /// "Chats you are in".
    Joined,
    /// "Chats you can view".
    Viewable,
    /// "Hidden chats": not joined and not viewable, and bots not joined.
    Requestable,
}

/// `CommunityChatJoined` / `IsCommunityChatViewable`.
pub(crate) fn chat_section(chat: &ParsedCommunityChat, joined: bool, bot: bool) -> ChatSection {
    if joined {
        ChatSection::Joined
    } else if !bot && chat.can_view_history {
        ChatSection::Viewable
    } else {
        ChatSection::Requestable
    }
}

impl QuillApp {
    fn community_chat_joined(&self, chat_id: i64) -> bool {
        self.session()
            .and_then(|s| s.chats.get(&chat_id))
            .is_some_and(|chat| chat.in_main_list || chat.in_archive)
    }

    /// A row of a section: open what can be opened, else the join
    /// question or why the chat can't be joined
    /// (`OpenCommunityLinkedPeer`).
    fn open_community_member_chat(
        &mut self,
        chat: ParsedCommunityChat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = self.community_chat_kind(chat.chat_id);
        let section = chat_section(
            &chat,
            self.community_chat_joined(chat.chat_id),
            kind == CommunityChatKind::Bot,
        );
        if section != ChatSection::Requestable
            || (kind == CommunityChatKind::Bot && !chat.is_hidden)
        {
            self.select_listed_chat(quill::ids::ChatId(chat.chat_id), window, cx);
            return;
        }
        let owner = self
            .session()
            .is_some_and(|s| s.chat_is_owner(quill::ids::ChatId(chat.chat_id)));
        if kind == CommunityChatKind::Bot || (chat.is_hidden && !owner) {
            self.connection.set_note(StatusNote::info(by_kind(
                kind,
                keys::COMMUNITY_HIDDEN_NOT_ACCESSIBLE,
                keys::COMMUNITY_HIDDEN_NOT_ACCESSIBLE_CHANNEL,
                keys::COMMUNITY_HIDDEN_NOT_ACCESSIBLE_BOT,
            )));
            cx.notify();
            return;
        }
        self.show_community_box(
            CommunityBox::ConfirmJoin {
                chat_id: chat.chat_id,
            },
            cx,
        );
    }

    pub(super) fn confirm_join_community_chat(&mut self, cx: &mut Context<Self>) {
        let Some(CommunityBox::ConfirmJoin { chat_id, .. }) = self.admin.community.flow.open.take()
        else {
            return;
        };
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.join_channel(quill::ids::ChatId(chat_id))
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
        }
        cx.notify();
    }

    fn community_section_row(
        &self,
        community_id: i64,
        chat: ParsedCommunityChat,
        can_edit: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat_id = chat.chat_id;
        let title = self.community_chat_title(chat_id);
        let kind = self.community_chat_kind(chat_id);
        let muted = cx.theme().muted_foreground;
        let status = if chat.is_hidden {
            Some(tr(keys::COMMUNITY_HIDDEN_STATUS).to_string())
        } else {
            self.session()
                .and_then(|s| s.chats.get(&chat_id).and_then(|c| s.group_member_counts(c)))
                .map(|(members, _)| members)
                .filter(|count| *count > 0)
                .map(|count| member_status(kind, count))
        };
        div()
            .id(("community-info-chat", chat_id as u64))
            .flex()
            .items_center()
            .gap_3()
            .w_full()
            .px_2()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .role(Role::Button)
            .aria_label(title.clone())
            .hover(|row| row.bg(cx.theme().list_hover))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_community_member_chat(chat.clone(), window, cx);
            }))
            .child(self.community_chat_avatar(chat_id, &title, 36.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().truncate().child(title))
                    .when_some(status, |col, status| {
                        col.child(div().text_xs().text_color(muted).child(status))
                    }),
            )
            .when(can_edit, |row| {
                row.child(
                    Button::new(("community-info-chat-remove", chat_id as u64))
                        .icon(IconName::CircleMinus)
                        .ghost()
                        .small()
                        .tooltip(by_kind(
                            kind,
                            keys::COMMUNITY_CHAT_REMOVE_GROUP,
                            keys::COMMUNITY_CHAT_REMOVE_CHANNEL,
                            keys::COMMUNITY_CHAT_REMOVE_BOT,
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.ask_remove_from_community(community_id, chat_id, cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// The panel's community part, under the name and counts.
    pub(in crate::ui) fn community_info_sections(
        &self,
        community_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let Some(community) = session.groups.communities.get(&community_id) else {
            return div().into_any_element();
        };
        let can_edit = community.can_edit_chat_list;
        let collapsed = session.community_chats_collapsed(community_id);
        let info = session.groups.community_full_infos.get(&community_id);
        let requests = info.map_or(0, |info| info.add_chat_request_count);
        let muted = cx.theme().muted_foreground;
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().text_sm().child(tr(keys::COMMUNITY_SHOW_AS_ONE)))
                    .child(
                        Switch::new(("community-show-as-one", community_id as u64))
                            .checked(collapsed)
                            .accessibility_label(tr(keys::COMMUNITY_SHOW_AS_ONE))
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.set_community_collapsed(community_id, on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::COMMUNITY_SHOW_AS_ONE_ABOUT)),
            );
        if can_edit && requests > 0 {
            body = body.child(
                action_row(
                    "community-info-requests",
                    Some(IconName::UserPlus),
                    tr(keys::COMMUNITY_REQUESTS_TITLE),
                    false,
                    cx,
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(requests.to_string()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_community_requests(community_id, cx);
                })),
            );
        }
        if let Some(info) = info {
            for (section, title) in [
                (ChatSection::Joined, keys::COMMUNITY_CHATS_JOINED),
                (ChatSection::Viewable, keys::COMMUNITY_CHATS_VIEWABLE),
                (ChatSection::Requestable, keys::COMMUNITY_CHATS_REQUESTABLE),
            ] {
                let chats: Vec<ParsedCommunityChat> = info
                    .chats
                    .iter()
                    .filter(|chat| {
                        let bot = self.community_chat_kind(chat.chat_id) == CommunityChatKind::Bot;
                        chat_section(chat, self.community_chat_joined(chat.chat_id), bot) == section
                    })
                    .cloned()
                    .collect();
                if chats.is_empty() {
                    continue;
                }
                body = body.child(
                    div()
                        .pt_2()
                        .text_sm()
                        .font_semibold()
                        .text_color(cx.theme().accent_foreground)
                        .child(tr(title)),
                );
                for chat in chats {
                    body = body.child(self.community_section_row(community_id, chat, can_edit, cx));
                }
            }
        }
        if can_edit {
            body = body.child(
                Button::new(("community-info-add-chat", community_id as u64))
                    .label(tr(keys::COMMUNITY_ADD_CHAT))
                    .icon(IconName::Plus)
                    .primary()
                    .w_full()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_choose_community_chat(community_id, window, cx);
                    })),
            );
        }
        body.into_any_element()
    }
}
