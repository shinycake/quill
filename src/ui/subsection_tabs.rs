//! Subsection tabs — Telegram Desktop's topic tabs for bots with topics and
//! forums with tabs (`history_view_subsection_tabs.cpp`,
//! `subsection_tabs_slider.cpp`, `chat.style` `chatTabs*`).
//!
//! Three layouts, cycled by the toggle at the strip's start
//! (Top → Bottom → Left, saved per chat):
//! - Top: a horizontal strip under the chat header;
//! - Bottom: the same strip just above the composer;
//! - Left: a narrow column of round topic icons with wrapped names.
//!
//! "All" (the whole chat) comes first, then the topics in topic order.
//! Clicking a tab opens that topic through the existing topic path;
//! right-clicking one opens the topic's actions.

use super::GroupConfirmAction;
use super::app::QuillApp;
use super::chat_theme::{accent, accent_strong, text_on_fill};
use super::pressable::PressableDiv;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::TopicBadge;
use quill::subsection_tabs::SubsectionTabsMode;
use quill::telegram::envelope::ForumTopic;

/// `chatTabsSlider.height` / `chatTabsToggle.height`.
const STRIP_HEIGHT: f32 = 36.;
/// `chatTabsToggle.width` is 64px in Telegram Desktop; the icon sits
/// centered, so a tighter hit target keeps the same look.
const TOGGLE_WIDTH: f32 = 48.;
/// `chatTabsVertical.width` (64px) plus breathing room for the bar.
const COLUMN_WIDTH: f32 = 68.;
/// `chatTabsVertical.userpicSize`.
const ICON_SIZE: f32 = 28.;
/// `chatTabsVertical.nameWidth`.
const NAME_WIDTH: f32 = 54.;
/// `chatTabsOutline*.stroke` 8px drawn half outside → a 4px visible bar.
const BAR_STROKE: f32 = 4.;

/// What the tabs need from the session, snapshotted once per render.
pub(super) struct TabsSnapshot {
    pub chat_id: ChatId,
    pub mode: SubsectionTabsMode,
    /// The open topic; `None` = "All".
    pub active: Option<i32>,
    pub topics: Vec<ForumTopic>,
    /// A bot chat (no close/reopen; delete only when users create topics).
    pub bot: Option<quill::state::BotTopics>,
    /// Each topic's badge (`Session::topic_badge`), by topic id.
    pub badges: std::collections::HashMap<i32, TopicBadge>,
}

/// Which tab a click or menu targets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    All,
    Topic(i32),
}

impl QuillApp {
    /// The open chat's tabs, when `SubsectionTabs::UsedFor` holds.
    pub(super) fn subsection_tabs_snapshot(&self) -> Option<TabsSnapshot> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        if !session.subsection_tabs_used_for(chat_id) {
            return None;
        }
        Some(TabsSnapshot {
            chat_id,
            mode: session.subsection_tabs_mode(chat_id),
            active: session.open_topic,
            topics: session
                .ordered_forum_topics(chat_id)
                .into_iter()
                .filter(|t| !t.is_hidden)
                .collect(),
            bot: session.bot_topics(chat_id),
            badges: session
                .ordered_forum_topics(chat_id)
                .iter()
                .map(|t| (t.forum_topic_id, session.topic_badge(chat_id, t)))
                .collect(),
        })
    }

    /// The horizontal strip for `at` (Top or Bottom), if the open chat
    /// uses tabs in that layout.
    pub(super) fn subsection_tabs_strip(
        &self,
        at: SubsectionTabsMode,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tabs = self.subsection_tabs_snapshot().filter(|t| t.mode == at)?;
        let bottom = at == SubsectionTabsMode::Bottom;
        let mut row = div()
            .id("subsection-tabs-scroll")
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_center()
            .overflow_x_scroll()
            .child(self.horizontal_tab(&tabs, Tab::All, cx));
        for topic in &tabs.topics {
            row = row.child(self.horizontal_tab(&tabs, Tab::Topic(topic.forum_topic_id), cx));
        }
        Some(
            div()
                .id(if bottom {
                    "subsection-tabs-bottom"
                } else {
                    "subsection-tabs-top"
                })
                .flex()
                .flex_none()
                .items_center()
                .w_full()
                .h(px(STRIP_HEIGHT))
                .bg(cx.theme().background)
                .border_color(cx.theme().border)
                .map(|this| {
                    if bottom {
                        this.border_t_1()
                    } else {
                        this.border_b_1()
                    }
                })
                .role(Role::TabList)
                .aria_label("Topics")
                .child(self.tabs_toggle(&tabs, cx))
                .child(row)
                .into_any_element(),
        )
    }

    /// The Left layout's column, if the open chat uses it.
    pub(super) fn subsection_tabs_column(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tabs = self
            .subsection_tabs_snapshot()
            .filter(|t| t.mode == SubsectionTabsMode::Left)?;
        let mut list = div()
            .id("subsection-tabs-column-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .child(self.vertical_tab(&tabs, Tab::All, cx));
        for topic in &tabs.topics {
            list = list.child(self.vertical_tab(&tabs, Tab::Topic(topic.forum_topic_id), cx));
        }
        Some(
            div()
                .id("subsection-tabs-left")
                .flex()
                .flex_col()
                .flex_none()
                .w(px(COLUMN_WIDTH))
                .h_full()
                .min_h_0()
                .bg(cx.theme().background)
                .border_r_1()
                .border_color(cx.theme().border)
                .role(Role::TabList)
                .aria_label("Topics")
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .justify_center()
                        .w_full()
                        .h(px(STRIP_HEIGHT))
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(self.tabs_toggle(&tabs, cx)),
                )
                .child(list)
                .into_any_element(),
        )
    }

    /// The layout toggle (`chatTabsToggle`): its glyph shows the current
    /// layout; a click moves to the next one.
    fn tabs_toggle(&self, tabs: &TabsSnapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let chat_id = tabs.chat_id;
        let icon = match tabs.mode {
            SubsectionTabsMode::Top => IconName::PanelTop,
            SubsectionTabsMode::Bottom => IconName::PanelBottom,
            SubsectionTabsMode::Left => IconName::PanelLeft,
        };
        div()
            .id("subsection-tabs-toggle")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(TOGGLE_WIDTH))
            .h(px(STRIP_HEIGHT))
            .cursor_pointer()
            .text_color(cx.theme().muted_foreground)
            .hover(|s| s.text_color(cx.theme().foreground))
            .role(Role::Button)
            .aria_label("Change tab layout")
            .tab_index(0)
            .tooltip(|window, cx| {
                gpui_kit::component::tooltip::Tooltip::new("Change tab layout").build(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.cycle_subsection_tabs_ui(chat_id, cx);
            }))
            .child(Icon::new(icon).size(px(20.)))
    }

    /// One horizontal tab: label, unread pill, accent underline when active.
    fn horizontal_tab(&self, tabs: &TabsSnapshot, tab: Tab, cx: &mut Context<Self>) -> AnyElement {
        let (label, unread, muted) = tab_label(tabs, tab);
        let active = is_active(tabs, tab);
        let text = if active {
            Hsla::from(accent())
        } else {
            cx.theme().muted_foreground
        };
        let element = div()
            .id(tab_element_id("subsection-tab", tab))
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap_1p5()
            .h_full()
            .px(px(10.))
            .cursor_pointer()
            .pressable(cx.theme())
            .role(Role::Tab)
            .aria_selected(active)
            .aria_label(label.clone())
            .tab_index(0)
            .text_sm()
            .font_semibold()
            .text_color(text)
            .on_click(cx.listener(move |this, _, _, cx| this.select_subsection_tab(tab, cx)))
            .child(div().whitespace_nowrap().child(label))
            .when(unread != TopicBadge::None, |this| {
                this.child(badge_pill(unread, muted, cx))
            })
            .when(active, |this| {
                this.child(
                    div()
                        .absolute()
                        .left(px(6.))
                        .right(px(6.))
                        .bottom_0()
                        .h(px(3.))
                        .rounded_t(px(2.))
                        .bg(accent()),
                )
            });
        self.with_topic_menu(tabs, tab, element, cx)
    }

    /// One Left-layout item: round icon with the unread badge on its
    /// corner, the name wrapped below (≤ 3 lines), a bar when active.
    fn vertical_tab(&self, tabs: &TabsSnapshot, tab: Tab, cx: &mut Context<Self>) -> AnyElement {
        let (label, unread, muted) = tab_label(tabs, tab);
        let active = is_active(tabs, tab);
        let text = if active {
            Hsla::from(accent())
        } else {
            cx.theme().muted_foreground
        };
        let icon = match tab {
            Tab::All => div()
                .size(px(ICON_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .text_color(text)
                .child(Icon::new(IconName::MessagesSquare).size(px(22.)))
                .into_any_element(),
            Tab::Topic(id) => tabs
                .topics
                .iter()
                .find(|t| t.forum_topic_id == id)
                .map(|t| topic_icon(t, ICON_SIZE, cx))
                .unwrap_or_else(|| div().size(px(ICON_SIZE)).into_any_element()),
        };
        let element = div()
            .id(tab_element_id("subsection-vtab", tab))
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .items_center()
            .w_full()
            .pt(px(8.))
            .pb(px(6.))
            .gap(px(4.))
            .cursor_pointer()
            .pressable(cx.theme())
            .role(Role::Tab)
            .aria_selected(active)
            .aria_label(label.clone())
            .tab_index(0)
            .on_click(cx.listener(move |this, _, _, cx| this.select_subsection_tab(tab, cx)))
            .child(
                div()
                    .relative()
                    .child(icon)
                    .when(unread != TopicBadge::None, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(-6.))
                                .left(px(ICON_SIZE - 12.))
                                .child(badge_pill(unread, muted, cx)),
                        )
                    }),
            )
            .child(
                div()
                    .w(px(NAME_WIDTH))
                    .text_size(px(10.))
                    .line_height(px(12.))
                    .text_center()
                    .line_clamp(3)
                    .text_ellipsis()
                    .text_color(text)
                    .child(label),
            )
            .when(active, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(8.))
                        .bottom(px(8.))
                        .w(px(BAR_STROKE))
                        .rounded_r(px(BAR_STROKE / 2.))
                        .bg(accent()),
                )
            });
        self.with_topic_menu(tabs, tab, element, cx)
    }

    /// Right-click a topic tab: the topic's dialog-entry actions
    /// (`Window::FillDialogsEntryMenu`, section `SubsectionTabsMenu`) —
    /// mark as read, pin/unpin, mute/unmute, close/reopen (forums), delete
    /// where permitted. "All" has no menu.
    fn with_topic_menu(
        &self,
        tabs: &TabsSnapshot,
        tab: Tab,
        element: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Tab::Topic(topic_id) = tab else {
            return element.into_any_element();
        };
        let Some(topic) = tabs.topics.iter().find(|t| t.forum_topic_id == topic_id) else {
            return element.into_any_element();
        };
        let chat_id = tabs.chat_id;
        let unread = topic.last_message_id != 0
            && tabs
                .badges
                .get(&topic_id)
                .copied()
                .unwrap_or(TopicBadge::None)
                != TopicBadge::None;
        let pinned = topic.is_pinned;
        let muted = topic.notification_settings.is_muted();
        let closed = topic.is_closed;
        let general = topic.is_general;
        let (can_close, can_delete) = match tabs.bot {
            Some(bot) => (false, bot.allows_users_to_create_topics),
            None => {
                let manage = self
                    .session()
                    .is_some_and(|s| s.chat_can_manage_topics(chat_id));
                (manage && !general, manage && !general)
            }
        };
        let can_pin = tabs.bot.is_some()
            || self
                .session()
                .is_some_and(|s| s.chat_can_manage_topics(chat_id));
        let owner = cx.entity().downgrade();
        let extras = self.topic_extras(chat_id, topic);
        element
            .context_menu(move |menu, _, _| {
                let extras_owner = owner.clone();
                let act = |action: TopicMenuAction| {
                    let owner = owner.clone();
                    move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        let _ = owner.update(cx, |this, cx| {
                            this.subsection_topic_action(chat_id, topic_id, action, cx);
                        });
                    }
                };
                let mut menu = menu;
                menu = menu.item(
                    PopupMenuItem::new("Topic info")
                        .icon(IconName::Info)
                        .on_click(act(TopicMenuAction::Info)),
                );
                if unread {
                    menu = menu.item(
                        PopupMenuItem::new("Mark as read")
                            .icon(IconName::CheckCheck)
                            .on_click(act(TopicMenuAction::MarkRead)),
                    );
                }
                if can_pin {
                    menu = menu.item(
                        PopupMenuItem::new(if pinned { "Unpin" } else { "Pin" })
                            .icon(if pinned {
                                IconName::PinOff
                            } else {
                                IconName::Pin
                            })
                            .on_click(act(if pinned {
                                TopicMenuAction::Unpin
                            } else {
                                TopicMenuAction::Pin
                            })),
                    );
                }
                menu = menu.item(
                    PopupMenuItem::new(if muted { "Unmute" } else { "Mute" })
                        .icon(if muted {
                            IconName::Bell
                        } else {
                            IconName::BellOff
                        })
                        .on_click(act(if muted {
                            TopicMenuAction::Unmute
                        } else {
                            TopicMenuAction::Mute
                        })),
                );
                if can_close {
                    menu = menu.item(
                        PopupMenuItem::new(if closed {
                            "Reopen topic"
                        } else {
                            "Close topic"
                        })
                        .icon(IconName::Lock)
                        .on_click(act(if closed {
                            TopicMenuAction::Reopen
                        } else {
                            TopicMenuAction::Close
                        })),
                    );
                }
                if extras.any() {
                    menu = menu.separator();
                    menu = super::forum_extras::add_topic_extras(
                        menu,
                        extras,
                        extras_owner,
                        chat_id,
                        topic_id,
                    );
                }
                if can_delete {
                    menu = menu.separator().item(
                        PopupMenuItem::new("Delete")
                            .icon(IconName::Trash)
                            .on_click(act(TopicMenuAction::Delete)),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn select_subsection_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        match tab {
            Tab::All => {
                if self.session().is_some_and(|s| s.open_topic.is_some()) {
                    self.deselect_topic_ui(cx);
                }
            }
            Tab::Topic(id) => {
                if self.session().and_then(|s| s.open_topic) != Some(id) {
                    self.select_topic_ui(id, cx);
                }
            }
        }
    }

    fn cycle_subsection_tabs_ui(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cycle_subsection_tabs_mode(chat_id);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.cycle_subsection_tabs_mode(chat_id);
        }
        cx.notify();
    }

    fn subsection_topic_action(
        &mut self,
        chat_id: ChatId,
        topic_id: i32,
        action: TopicMenuAction,
        cx: &mut Context<Self>,
    ) {
        use super::ForumTopicAction;
        let forum = match action {
            TopicMenuAction::Pin => Some(ForumTopicAction::Pin),
            TopicMenuAction::Unpin => Some(ForumTopicAction::Unpin),
            TopicMenuAction::Close => Some(ForumTopicAction::Close),
            TopicMenuAction::Reopen => Some(ForumTopicAction::Reopen),
            _ => None,
        };
        if let Some(forum) = forum {
            self.forum_topic_action(chat_id, topic_id, forum, cx);
            return;
        }
        if action == TopicMenuAction::Info {
            if self.session().and_then(|s| s.open_topic) != Some(topic_id) {
                self.select_topic_ui(topic_id, cx);
            }
            self.topic_info_open = true;
            cx.notify();
            return;
        }
        if action == TopicMenuAction::Delete {
            self.open_group_confirm(
                chat_id,
                GroupConfirmAction::DeleteForumTopic {
                    forum_topic_id: topic_id,
                },
                cx,
            );
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.status_note = "topics need a live connection (demo)".into();
            cx.notify();
            return;
        };
        let result = match action {
            TopicMenuAction::MarkRead => live.driver.mark_forum_topic_read(chat_id, topic_id),
            TopicMenuAction::Mute => live.driver.set_forum_topic_muted(chat_id, topic_id, true),
            TopicMenuAction::Unmute => live.driver.set_forum_topic_muted(chat_id, topic_id, false),
            _ => Ok(None),
        };
        if result.is_err() {
            self.status_note = "could not update topic".into();
        }
        cx.notify();
    }
}

/// `ReadyBotTopics*` fixtures: a bot with topics (`userTypeBot.has_topics`)
/// whose `getForumTopics` answer seeds four topics, opened with the tabs in
/// `mode`. Top and Bottom show "All" (the whole chat); Left opens a topic so
/// the active bar sits on a topic icon. Injected through the live reducer.
pub(super) fn apply_ready_bot_topics(
    session: &mut quill::state::Session,
    sink: &std::sync::Arc<quill::diagnostics::MemorySink>,
    seq: &std::sync::atomic::AtomicU64,
    mode: SubsectionTabsMode,
) {
    use quill::diagnostics::DiagnosticSink;
    use quill::state::RequestPurpose;
    use quill::telegram::client::copy_and_parse;
    const BOT: i64 = 41;
    let dyn_sink: std::sync::Arc<dyn DiagnosticSink> = sink.clone();
    let apply = |session: &mut quill::state::Session, json: &str| {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    apply(
        session,
        &format!(
            r#"{{"@type":"updateUser","user":{{"id":{BOT},"first_name":"Atlas","last_name":"Assistant","type":{{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":true,"allows_users_to_create_topics":true,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}}}}"#
        ),
    );
    apply(
        session,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{BOT},"title":"Atlas Assistant","type":{{"@type":"chatTypePrivate","user_id":{BOT}}},"unread_count":5}}}}"#
        ),
    );
    apply(
        session,
        &format!(
            r#"{{"@type":"updateChatPosition","chat_id":{BOT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"99","is_pinned":false}}}}"#
        ),
    );
    let message = |id: i64, topic: i32, outgoing: bool, date: i64, text: &str| {
        format!(
            r#"{{"id":{id},"chat_id":{BOT},"date":{date},"is_outgoing":{outgoing},"topic_id":{{"@type":"messageTopicForum","forum_topic_id":{topic}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
        )
    };
    let base = 1_760_000_000_i64;
    let messages = [
        message(
            501,
            2,
            true,
            base,
            "Plan a three-day trip to Lisbon in May.",
        ),
        message(
            502,
            2,
            false,
            base + 60,
            "Day 1: Alfama and the castle. Day 2: Belém. Day 3: Sintra by train.",
        ),
        message(503, 3, true, base + 120, "Make the release notes shorter."),
        message(
            504,
            3,
            false,
            base + 180,
            "Done: five bullets, one line each.",
        ),
        message(
            505,
            4,
            false,
            base + 240,
            "Suggest a recipe with chickpeas and spinach.",
        ),
        message(506, 5, false, base + 300, "Your weekly summary is ready."),
    ];
    // Older back-and-forth so "All" scrolls, as a real bot chat does.
    for i in 0..30_i64 {
        let text = if i % 2 == 0 {
            "Another draft, please."
        } else {
            "Here is a tighter version."
        };
        let row = message(400 + i, 3, i % 2 == 0, base - 3_600 + i * 60, text);
        apply(
            session,
            &format!(r#"{{"@type":"updateNewMessage","message":{row}}}"#),
        );
    }
    for row in &messages {
        apply(
            session,
            &format!(r#"{{"@type":"updateNewMessage","message":{row}}}"#),
        );
    }
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(BOT)));
    let topic = |id: i32, name: &str, color: u32, unread: i32, pinned: bool, last: &str| {
        format!(
            r#"{{"info":{{"@type":"forumTopicInfo","chat_id":{BOT},"forum_topic_id":{id},"name":"{name}","icon":{{"@type":"forumTopicIcon","color":{color},"custom_emoji_id":"0"}},"creation_date":1,"creator_id":{{"@type":"messageSenderUser","user_id":{BOT}}},"is_general":false,"is_outgoing":true,"is_closed":false,"is_hidden":false,"is_name_implicit":false}},"last_message":{last},"order":"{order}","is_pinned":{pinned},"unread_count":{unread},"last_read_inbox_message_id":500,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}}"#,
            order = 100 - id,
        )
    };
    let topics = [
        topic(2, "Lisbon trip", 0x6FB9F0, 0, true, &messages[1]),
        topic(3, "Release notes", 0xFFD67E, 0, false, &messages[3]),
        topic(4, "Suggest recipes", 0x8EEE98, 2, false, &messages[4]),
        topic(5, "Weekly summary", 0xCB86DB, 3, false, &messages[5]),
        topic(
            6,
            "Make the new model the default",
            0xFF93B2,
            0,
            false,
            "null",
        ),
    ]
    .join(",");
    apply(
        session,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":5,"topics":[{topics}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            extra.0
        ),
    );
    session.open_chat(ChatId(BOT));
    while session.subsection_tabs_mode(ChatId(BOT)) != mode {
        session.cycle_subsection_tabs_mode(ChatId(BOT));
    }
    if mode == SubsectionTabsMode::Left {
        session.select_topic(ChatId(BOT), 2);
        let extra =
            session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(BOT)), 2);
        apply(
            session,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":0,"messages":[{},{}]}}"#,
                extra.0, messages[1], messages[0]
            ),
        );
    }
}

/// The Left layout: the tab column beside the history (or the history
/// alone when the chat doesn't use the Left layout).
pub(super) fn with_left_column(column: Option<AnyElement>, history: AnyElement) -> AnyElement {
    match column {
        None => history,
        Some(column) => div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(column)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(history),
            )
            .into_any_element(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TopicMenuAction {
    Info,
    MarkRead,
    Pin,
    Unpin,
    Mute,
    Unmute,
    Close,
    Reopen,
    Delete,
}

fn is_active(tabs: &TabsSnapshot, tab: Tab) -> bool {
    match tab {
        Tab::All => tabs.active.is_none(),
        Tab::Topic(id) => tabs.active == Some(id),
    }
}

/// `(label, badge, muted)` for a tab.
fn tab_label(tabs: &TabsSnapshot, tab: Tab) -> (String, TopicBadge, bool) {
    match tab {
        Tab::All => ("All".to_string(), TopicBadge::None, false),
        Tab::Topic(id) => tabs
            .topics
            .iter()
            .find(|t| t.forum_topic_id == id)
            .map(|t| {
                let name = if t.name.is_empty() {
                    format!("Topic {id}")
                } else {
                    t.name.clone()
                };
                let badge = tabs.badges.get(&id).copied().unwrap_or(TopicBadge::None);
                (name, badge, t.notification_settings.is_muted())
            })
            .unwrap_or_else(|| (format!("Topic {id}"), TopicBadge::None, false)),
    }
}

fn tab_element_id(prefix: &'static str, tab: Tab) -> ElementId {
    match tab {
        Tab::All => ElementId::from(SharedString::from(format!("{prefix}-all"))),
        Tab::Topic(id) => ElementId::from((prefix, id as u64)),
    }
}

/// A tab's unread mark: the counter, or a small dot when the count isn't
/// known (`TopicBadge::Dot`).
fn badge_pill(badge: TopicBadge, muted: bool, cx: &App) -> AnyElement {
    match badge {
        TopicBadge::Count(count) => count_pill(count, muted, cx).into_any_element(),
        TopicBadge::Dot => {
            let bg = if muted {
                cx.theme().muted_foreground.opacity(0.55)
            } else {
                Hsla::from(accent_strong())
            };
            div()
                .flex_none()
                .size(px(9.))
                .rounded_full()
                .bg(bg)
                .into_any_element()
        }
        TopicBadge::None => div().into_any_element(),
    }
}

/// The unread counter (`chatTabs` badges): accent, or muted gray for a
/// muted topic.
fn count_pill(count: i32, muted: bool, cx: &App) -> impl IntoElement {
    let bg = if muted {
        cx.theme().muted_foreground.opacity(0.55)
    } else {
        Hsla::from(accent_strong())
    };
    div()
        .flex_none()
        .h(px(18.))
        .min_w(px(18.))
        .px(px(5.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .text_color(text_on_fill())
        .text_size(px(11.))
        .font_semibold()
        .child(super::chat_row::compact_count(count))
}

/// The round topic icon: `forumTopicIcon.color` with the name's first
/// letter (Telegram Desktop's `TopicIconEmojiEntity`); the General topic
/// gets a muted "#". Custom-emoji icons fall back to the letter.
pub(super) fn topic_icon(topic: &ForumTopic, size: f32, cx: &App) -> AnyElement {
    if topic.is_general {
        return div()
            .size(px(size))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(size * 0.7))
            .font_semibold()
            .text_color(cx.theme().muted_foreground)
            .child("#")
            .into_any_element();
    }
    let color: Hsla = if topic.icon_color > 0 {
        rgb(topic.icon_color as u32).into()
    } else {
        accent().into()
    };
    let letter: String = topic
        .name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "#".to_string());
    div()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(color)
        .text_color(gpui_kit::white())
        .text_size(px(size * 0.5))
        .font_semibold()
        .child(letter)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Tab, TabsSnapshot, is_active, tab_label};
    use quill::ids::ChatId;
    use quill::subsection_tabs::SubsectionTabsMode;

    fn snapshot(active: Option<i32>) -> TabsSnapshot {
        TabsSnapshot {
            chat_id: ChatId(21),
            mode: SubsectionTabsMode::Top,
            active,
            topics: Vec::new(),
            bot: None,
            badges: Default::default(),
        }
    }

    #[test]
    fn all_is_active_without_a_topic() {
        assert!(is_active(&snapshot(None), Tab::All));
        assert!(!is_active(&snapshot(Some(3)), Tab::All));
        assert!(is_active(&snapshot(Some(3)), Tab::Topic(3)));
        assert_eq!(tab_label(&snapshot(None), Tab::All).0, "All");
    }
}
