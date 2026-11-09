//! The subsection tabs of a channel's direct messages, for its admins
//! (tdesktop `SubsectionTabs` for `amMonoforumAdmin`): "All", then one tab
//! per person writing, with their userpic and short name. Right-click a
//! tab for its actions (tdesktop `Filler::fillContextMenuActions` for a
//! sublist): mark as read or unread, ban from the channel, delete the chat.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::direct_messages_state::{DirectConfirm, DirectConfirmKind};
use super::subsection_tabs::{Tab, TabsSnapshot};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::{Session, TopicBadge};
use quill::telegram::envelope::{DirectMessagesTopic, MessageSender};

/// How close to the end of the tabs (in px) the next page is asked for.
const PAGE_AHEAD: f32 = 120.;

pub(super) struct DirectTab {
    pub id: i64,
    pub short_name: String,
    pub sender: MessageSender,
    pub badge: TopicBadge,
}

pub(super) struct DirectTabs {
    pub active: Option<i64>,
    pub tabs: Vec<DirectTab>,
}

impl DirectTabs {
    /// `Some` only for a channel's direct messages its admin has open.
    pub(super) fn of(session: &Session, chat_id: ChatId) -> Option<Self> {
        if !session.is_direct_messages_admin(chat_id) {
            return None;
        }
        let tabs = session
            .threads
            .direct
            .ordered(chat_id.0)
            .into_iter()
            .map(|topic| DirectTab {
                id: topic.id,
                short_name: session.direct_topic_short_name(topic),
                sender: topic.sender,
                badge: badge(topic),
            })
            .collect();
        Some(Self {
            active: session.open_direct_topic(chat_id),
            tabs,
        })
    }

    /// The strip child to keep in view: 0 is "All".
    pub(super) fn selected_child(&self) -> Option<usize> {
        match self.active {
            None => Some(0),
            Some(id) => self
                .tabs
                .iter()
                .position(|tab| tab.id == id)
                .map(|at| at.saturating_add(1)),
        }
    }
}

/// tdesktop `chatListBadgesState`: the unread count, or a dot for a chat
/// marked unread by hand.
fn badge(topic: &DirectMessagesTopic) -> TopicBadge {
    if topic.unread_count > 0 {
        TopicBadge::Count(topic.unread_count)
    } else if topic.is_marked_as_unread {
        TopicBadge::Dot
    } else {
        TopicBadge::None
    }
}

pub(super) fn sender_userpic(session: &Session, sender: MessageSender, size: f32) -> AnyElement {
    let name = session.sender_label(sender);
    let photo = match sender {
        MessageSender::User { user_id } => session.user_photo_path(user_id),
        MessageSender::Chat { chat_id } => session.chat_photo_path(ChatId(chat_id)),
    };
    super::message_text::kit_avatar_element(&name, photo.map(std::path::Path::new), px(size))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum DirectAction {
    MarkRead,
    MarkUnread,
    Ban { channel: ChatId, user_id: i64 },
    Unban { channel: ChatId, user_id: i64 },
    Delete,
}

impl DirectAction {
    fn label(self) -> (&'static str, IconName) {
        match self {
            Self::MarkRead => (tr(keys::CONTEXT_MARK_READ), IconName::CheckCheck),
            Self::MarkUnread => (tr(keys::CONTEXT_MARK_UNREAD), IconName::MessageCircle),
            Self::Ban { .. } => (tr(keys::CONTEXT_MONOFORUM_BAN), IconName::UserX),
            Self::Unban { .. } => (tr(keys::CONTEXT_MONOFORUM_UNBAN), IconName::UserCheck),
            Self::Delete => (tr(keys::PROFILE_DELETE_CONVERSATION), IconName::Trash),
        }
    }
}

/// A person's tab menu, in tdesktop's order (`fillContextMenuActions`):
/// mark read or unread once there are messages, ban or unban while the
/// viewer may restrict the channel's members, then Delete chat.
pub(super) fn direct_menu_actions(
    session: &Session,
    chat_id: ChatId,
    topic_id: i64,
) -> Vec<DirectAction> {
    let Some(topic) = session.direct_topic(chat_id, topic_id) else {
        return Vec::new();
    };
    let mut actions = Vec::new();
    if topic.last_message_id != 0 {
        actions.push(if topic.is_unread() {
            DirectAction::MarkRead
        } else {
            DirectAction::MarkUnread
        });
    }
    if let Some((channel, user_id)) = direct_ban_target(session, chat_id, topic) {
        actions.push(
            if session.direct_member_banned(channel, user_id) == Some(true) {
                DirectAction::Unban { channel, user_id }
            } else {
                DirectAction::Ban { channel, user_id }
            },
        );
    }
    actions.push(DirectAction::Delete);
    actions
}

impl QuillApp {
    pub(super) fn direct_tab_userpic(
        &self,
        tabs: &TabsSnapshot,
        tab: Tab,
        size: f32,
    ) -> Option<AnyElement> {
        let Tab::Direct(id) = tab else {
            return None;
        };
        let sender = tabs
            .direct
            .as_ref()?
            .tabs
            .iter()
            .find(|t| t.id == id)?
            .sender;
        Some(sender_userpic(self.session()?, sender, size))
    }

    /// Scrolling near the end of the tabs loads the next people (tdesktop
    /// `SubsectionTabs` asks `monoforum->loadMore()`).
    pub(super) fn direct_tabs_paging(
        &self,
        tabs: &TabsSnapshot,
        list: Stateful<Div>,
        vertical: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if tabs.direct.is_none() {
            return list;
        }
        let chat_id = tabs.chat_id;
        let handle = if vertical {
            self.history.subsection_tabs.column.clone()
        } else {
            self.history.subsection_tabs.strip.clone()
        };
        list.on_scroll(cx.listener(move |this, _, _, cx| {
            let max = handle.max_offset();
            let offset = handle.offset();
            let (left, max) = if vertical {
                (f32::from(max.y) + f32::from(offset.y), f32::from(max.y))
            } else {
                (f32::from(max.x) + f32::from(offset.x), f32::from(max.x))
            };
            if max > 0. && left <= PAGE_AHEAD {
                this.load_more_direct_topics_ui(chat_id, cx);
            }
        }))
    }

    fn load_more_direct_topics_ui(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && matches!(live.driver.load_more_direct_topics(chat_id), Ok(Some(_)))
        {
            cx.notify();
        }
    }

    pub(super) fn select_direct_topic_ui(
        &mut self,
        chat_id: ChatId,
        topic: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        if self.session().and_then(|s| s.open_direct_topic(chat_id)) == topic {
            return;
        }
        self.flush_leaving_draft(cx);
        self.history.scroll_memory.forget((chat_id.0, None, 0));
        if let Some(live) = self.live.as_mut() {
            if live.driver.select_direct_topic(chat_id, topic).is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_OPEN_TOPIC)));
            }
        } else if let Some(session) = self.demo_session.as_mut()
            && session.select_direct_topic(chat_id, topic)
        {
            self.demo_refill_direct_history(chat_id);
        }
        self.request_draft_restore();
        cx.notify();
    }

    pub(super) fn with_direct_menu(
        &self,
        chat_id: ChatId,
        topic_id: i64,
        element: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return element.into_any_element();
        };
        let Some(topic) = session.direct_topic(chat_id, topic_id) else {
            return element.into_any_element();
        };
        let ban = direct_ban_target(session, chat_id, topic);
        let owner = cx.entity().downgrade();
        let element = match ban {
            Some((channel, user_id)) => element.on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| {
                    this.ask_direct_member(channel, user_id, cx);
                }),
            ),
            None => element,
        };
        element
            .context_menu(move |menu, _, cx| {
                let actions = owner
                    .upgrade()
                    .and_then(|app| {
                        Some(direct_menu_actions(
                            app.read(cx).session()?,
                            chat_id,
                            topic_id,
                        ))
                    })
                    .unwrap_or_default();
                actions.into_iter().fold(menu, |menu, action| {
                    let owner = owner.clone();
                    let (label, icon) = action.label();
                    let menu = if action == DirectAction::Delete {
                        menu.separator()
                    } else {
                        menu
                    };
                    menu.item(PopupMenuItem::new(label).icon(icon).on_click(
                        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            let _ = owner.update(cx, |this, cx| {
                                this.direct_topic_action(chat_id, topic_id, action, cx);
                            });
                        },
                    ))
                })
            })
            .into_any_element()
    }

    fn ask_direct_member(&mut self, channel: ChatId, user_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && matches!(
                live.driver.fetch_direct_member(channel, user_id),
                Ok(Some(_))
            )
        {
            cx.notify();
        }
    }

    fn direct_topic_action(
        &mut self,
        chat_id: ChatId,
        topic_id: i64,
        action: DirectAction,
        cx: &mut Context<Self>,
    ) {
        let name = self
            .session()
            .and_then(|s| {
                s.direct_topic(chat_id, topic_id)
                    .map(|t| s.direct_topic_name(t))
            })
            .unwrap_or_default();
        let confirm = |kind| DirectConfirm {
            chat_id,
            topic_id,
            name: name.clone(),
            kind,
        };
        match action {
            DirectAction::Delete => {
                self.direct_messages.confirm = Some(confirm(DirectConfirmKind::Delete));
                self.open_box(crate::ui::shell::DialogKind::DirectConfirm);
            }
            DirectAction::Ban { channel, user_id } => {
                self.direct_messages.confirm =
                    Some(confirm(DirectConfirmKind::Ban { channel, user_id }));
                self.open_box(crate::ui::shell::DialogKind::DirectConfirm);
            }
            DirectAction::Unban { channel, user_id } => {
                self.run_direct_live(|driver| {
                    driver
                        .set_direct_member_banned(channel, user_id, false)
                        .map(|_| ())
                });
            }
            DirectAction::MarkRead | DirectAction::MarkUnread => {
                let read = action == DirectAction::MarkRead;
                self.run_direct_live(|driver| {
                    driver
                        .set_direct_topic_read(chat_id, topic_id, read)
                        .map(|_| ())
                });
            }
        }
        cx.notify();
    }

    pub(super) fn run_direct_live(
        &mut self,
        run: impl FnOnce(
            &mut quill::connect::ConnectDriver<quill::connect::LiveSender>,
        ) -> Result<(), quill::connect::ConnectSendError>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_MISC_TOPICS_NEED_A_LIVE_CONNECTION,
            )));
            return;
        };
        if run(&mut live.driver).is_err() {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MISC_COULD_NOT_UPDATE_TOPIC)));
        }
    }
}

/// The channel and user a conversation's ban row acts on: a user writing,
/// in a channel where the viewer may restrict members (tdesktop
/// `canRestrictParticipant`).
pub(super) fn direct_ban_target(
    session: &Session,
    chat_id: ChatId,
    topic: &DirectMessagesTopic,
) -> Option<(ChatId, i64)> {
    let user_id = topic.user_id()?;
    let channel = session.chat_direct_messages_chat(chat_id)?;
    session
        .chat_can_restrict_members(channel)
        .then_some((channel, user_id))
}
