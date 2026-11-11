//! "Add a Chat to Community" (tdesktop `ShowChooseChatToAddBox`): the
//! person's chats that can join (`getSuitableCommunityChats`) and the
//! bots they own, a search field, and "Create a New Chat", whose new
//! group or channel goes straight to the visibility choice.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use quill::new_chat::{NewChatKind, NewChatStep};
use quill::state::{ProfileChatsFetch, ProfileChatsKind};

/// How the chooser's lists stand: still loading, or why one failed (its
/// chats are then missing from the list, which says so with a Retry
/// instead of "No chats found").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CandidateStatus {
    Ready,
    Loading,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) chat_id: i64,
    pub(crate) title: String,
}

impl QuillApp {
    pub(super) fn open_choose_community_chat(
        &mut self,
        community_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live
                .driver
                .fetch_suitable_community_chats()
                .and_then(|_| live.driver.fetch_owned_bots())
        {
            self.set_note(StatusNote::error(user_error(&err)));
        }
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_OPS_SEARCH_CHATS)));
        let typed = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        self.show_community_box(
            CommunityBox::ChooseChat {
                community_id,
                search,
                _typed: typed,
            },
            cx,
        );
    }

    /// The chats the box offers, by title, filtered by `query`: the
    /// suitable chats, then the owned bots (`getOwnedBots`) that are in
    /// no community. A bot's chat is its user id.
    pub(crate) fn community_candidates(&self, query: &str) -> (Vec<Candidate>, CandidateStatus) {
        let Some(session) = self.session() else {
            return (Vec::new(), CandidateStatus::Ready);
        };
        let query = query.trim().to_lowercase();
        let (suitable, mut status) = match session
            .users_state
            .profile_chat_lists
            .get(&(ProfileChatsKind::SuitableCommunityChats, 0))
        {
            Some(ProfileChatsFetch::Loaded { ids, .. }) => (ids.clone(), CandidateStatus::Ready),
            Some(ProfileChatsFetch::Loading(_)) => (Vec::new(), CandidateStatus::Loading),
            Some(ProfileChatsFetch::Failed(error)) => {
                (Vec::new(), CandidateStatus::Failed(error.clone()))
            }
            None => (Vec::new(), CandidateStatus::Ready),
        };
        if status == CandidateStatus::Ready
            && let Some(error) = session.communities.owned_bots.error()
        {
            status = CandidateStatus::Failed(error.to_string());
        }
        let mut chats: Vec<Candidate> = suitable
            .into_iter()
            .filter_map(|chat_id| {
                let chat = session.chats.get(&chat_id)?;
                Some(Candidate {
                    chat_id,
                    title: chat.title.clone(),
                })
            })
            .collect();
        chats.sort_by_key(|c| c.title.to_lowercase());
        let owned_bots: &[i64] = session
            .communities
            .owned_bots
            .data()
            .map_or(&[], Vec::as_slice);
        let mut bots: Vec<Candidate> = owned_bots
            .iter()
            .filter_map(|id| session.users.get(id))
            .filter(|user| user.is_bot)
            .filter(|user| session.chat_community_id(user.id).is_none())
            .map(|user| Candidate {
                chat_id: user.id,
                title: user.display_name(),
            })
            .collect();
        bots.sort_by_key(|c| c.title.to_lowercase());
        chats.extend(bots);
        chats.retain(|c| query.is_empty() || c.title.to_lowercase().contains(&query));
        (chats, status)
    }

    /// The chooser's Retry: both lists asked again.
    fn retry_community_candidates(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live
                .driver
                .fetch_suitable_community_chats()
                .and_then(|_| live.driver.fetch_owned_bots())
        {
            self.set_note(StatusNote::error(user_error(&err)));
        }
        cx.notify();
    }

    /// "Create a New Chat" → New Group / New Channel; the new chat then
    /// goes to the visibility box.
    fn create_chat_for_community(
        &mut self,
        community_id: i64,
        kind: NewChatKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The chooser closes the way every community box does (a Pending
        // Requests answer in its undo time goes), and the wait takes over.
        self.flush_and_close_community_box(cx);
        self.admin.community.flow.new_chat_for = Some(community_id);
        self.open_new_chat(kind, window, cx);
    }

    /// The chat made for a community exists: its own steps end there and
    /// the visibility box opens (GroupInfoBox's `done` callback).
    pub(super) fn drain_new_community_chat(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(community_id) = self.admin.community.flow.new_chat_for else {
            return false;
        };
        let Some(view) = self.new_chat_view() else {
            self.admin.community.flow.new_chat_for = None;
            return true;
        };
        let chat = match view.step {
            NewChatStep::Type(chat) | NewChatStep::Done(chat) => chat,
            _ => return false,
        };
        self.admin.community.flow.new_chat_for = None;
        self.close_new_chat(cx);
        self.choose_community_visibility(community_id, chat.0, None, cx);
        true
    }

    fn build_community_choose_chat_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::CommunityChooseChat,
            |this, _, cx| this.flush_and_close_community_box(cx),
        );
        let owner = app.clone();
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).w(px(400.));
            let Some(CommunityBox::ChooseChat {
                community_id,
                search,
                ..
            }) = this.admin.community.flow.open.as_ref()
            else {
                return dialog.on_close(on_close);
            };
            let community_id = *community_id;
            let search = search.clone();
            let query = search.read(cx).value().to_string();
            let (candidates, status) = this.community_candidates(&query);
            let muted = cx.theme().muted_foreground;
            let mut list = div()
                .id("community-choose-list")
                .flex()
                .flex_col()
                .max_h(px(340.))
                .overflow_y_scroll();
            for candidate in candidates.iter() {
                let chat_id = candidate.chat_id;
                list = list.child(
                    div()
                        .id(("community-choose", chat_id as u64))
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_2()
                        .py_1p5()
                        .rounded_md()
                        .cursor_pointer()
                        .role(Role::Button)
                        .aria_label(candidate.title.clone())
                        .hover(|row| row.bg(cx.theme().list_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            // An owned bot may have no chat yet; TDLib
                            // adds chats, so make it before the add.
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.ensure_private_chat(chat_id)
                            {
                                this.set_note(StatusNote::error(user_error(&err)));
                            }
                            this.choose_community_visibility(
                                community_id,
                                chat_id,
                                Some(Back::ChooseChat { community_id }),
                                cx,
                            );
                        }))
                        .child(this.community_chat_avatar(chat_id, &candidate.title, 36.))
                        .child(div().min_w_0().truncate().child(candidate.title.clone())),
                );
            }
            if let CandidateStatus::Failed(error) = &status {
                list = list.child(
                    div()
                        .py_2()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap_2()
                        .child(div().text_sm().text_color(muted).child(error.clone()))
                        .child(
                            Button::new("community-choose-retry")
                                .label(tr(keys::Q_CHAT_RETRY))
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.retry_community_candidates(cx);
                                })),
                        ),
                );
            } else if candidates.is_empty() {
                list = list.child(if status == CandidateStatus::Loading {
                    div()
                        .py_4()
                        .flex()
                        .justify_center()
                        .child(gpui_kit::component::spinner::Spinner::new())
                        .into_any_element()
                } else {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(keys::Q_OPS_NO_CHATS_FOUND))
                        .into_any_element()
                });
            }
            let create = Button::new("community-choose-create")
                .label(tr(keys::COMMUNITY_CREATE_CHAT))
                .icon(IconName::Plus)
                .ghost()
                .dropdown_menu(move |menu, _, _| {
                    let (group, channel) = (owner.clone(), owner.clone());
                    menu.item(PopupMenuItem::new(tr(keys::CREATE_GROUP_TITLE)).on_click(
                        move |_, window, cx| {
                            group.update(cx, |this, cx| {
                                this.create_chat_for_community(
                                    community_id,
                                    NewChatKind::Group,
                                    window,
                                    cx,
                                )
                            });
                        },
                    ))
                    .item(
                        PopupMenuItem::new(tr(keys::CREATE_CHANNEL_TITLE)).on_click(
                            move |_, window, cx| {
                                channel.update(cx, |this, cx| {
                                    this.create_chat_for_community(
                                        community_id,
                                        NewChatKind::Channel,
                                        window,
                                        cx,
                                    )
                                });
                            },
                        ),
                    )
                });
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(Input::new(&search).cleanable(true))
                .child(div().flex().child(create))
                .child(list);
            let footer = div().flex().justify_end().child(
                Button::new("community-choose-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.flush_and_close_community_box(cx))),
            );
            dialog
                .title(crate::ui::shell::dialog_title(tr(
                    keys::COMMUNITY_ADD_CHAT_TITLE,
                )))
                .content(super::boxes::once(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// `MessageGroupOwnerBox`.
    fn build_community_hidden_chat_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::CommunityHiddenChat,
            |this, _, cx| this.back_to_community_requests(cx),
        );
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).w(px(340.));
            let Some(CommunityBox::HiddenChat {
                chat_id,
                owner_user_id,
                ..
            }) = this.admin.community.flow.open
            else {
                return dialog.on_close(on_close);
            };
            let title = this.community_chat_title(chat_id);
            let kind = this.community_chat_kind(chat_id);
            let muted = cx.theme().muted_foreground;
            let members = (kind != CommunityChatKind::Bot).then(|| {
                let count = this
                    .session()
                    .and_then(|s| s.chats.get(&chat_id).and_then(|c| s.group_member_counts(c)))
                    .map_or(0, |(members, _)| members);
                member_status(kind, count)
            });
            let body = div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(this.community_chat_avatar(chat_id, &title, 72.))
                .child(div().text_lg().font_semibold().child(title))
                .children(members.map(|m| div().text_sm().text_color(muted).child(m)))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_start()
                        .gap_2()
                        .text_sm()
                        .child(
                            Icon::new(IconName::EyeOff)
                                .size(px(16.))
                                .flex_none()
                                .mt(px(2.))
                                .text_color(muted),
                        )
                        .child(div().flex_1().min_w_0().child(by_kind(
                            kind,
                            keys::COMMUNITY_REQUEST_INVITE_ONLY,
                            keys::COMMUNITY_REQUEST_INVITE_ONLY_CHANNEL,
                            keys::COMMUNITY_REQUEST_INVITE_ONLY_BOT,
                        ))),
                )
                .child(
                    Button::new("community-hidden-message-owner")
                        .label(by_kind(
                            kind,
                            keys::COMMUNITY_REQUEST_MESSAGE_OWNER,
                            keys::COMMUNITY_REQUEST_MESSAGE_OWNER_CHANNEL,
                            keys::COMMUNITY_REQUEST_MESSAGE_OWNER_BOT,
                        ))
                        .primary()
                        .w_full()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.flush_and_close_community_box(cx);
                            this.open_user_chat(owner_user_id, window, cx);
                        })),
                )
                .child(
                    Button::new("community-hidden-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .w_full()
                        .on_click(
                            cx.listener(|this, _, _, cx| this.back_to_community_requests(cx)),
                        ),
                );
            dialog
                .content(super::boxes::once(body.into_any_element()))
                .on_close(on_close)
        })
    }

    fn back_to_community_requests(&mut self, cx: &mut Context<Self>) {
        match self.admin.community.flow.open {
            Some(CommunityBox::HiddenChat { community_id, .. }) => {
                self.show_community_box(
                    CommunityBox::Requests {
                        community_id,
                        undo: None,
                    },
                    cx,
                );
            }
            _ => self.flush_and_close_community_box(cx),
        }
    }
}

crate::ui::shell::register_dialogs! {
    /// "Add a Chat to Community".
    CommunityChooseChat => DialogSpec::new(
        5415,
        |app| matches!(app.admin.community.flow.open, Some(CommunityBox::ChooseChat { .. })),
        QuillApp::build_community_choose_chat_dialog,
    )
    .close(|app, _, cx| app.flush_and_close_community_box(cx)),
    /// A hidden suggested chat and its owner.
    CommunityHiddenChat => DialogSpec::new(
        5416,
        |app| matches!(app.admin.community.flow.open, Some(CommunityBox::HiddenChat { .. })),
        QuillApp::build_community_hidden_chat_dialog,
    )
    .close(|app, _, cx| app.flush_and_close_community_box(cx)),
}
