//! Agent access, the Activity section: what agents did for the account
//! Settings edits (`quill::agent::activity`), filtered by agent, chat and
//! action. Each row opens the chat or message it touched, and offers Undo
//! where that is honest: deleting messages an agent sent (only those; the
//! log holds the ids of the messages its own sends made) and taking back a
//! reaction it added. Both go through the normal delete and reaction paths,
//! which act on the open account, so another account's log shows neither
//! Open nor Undo.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::agent_access::empty_line;
use super::rows::group;
use crate::ui::agent_labels::{agent_label, now_unix, tool_label};
use crate::ui::agent_state::{ACTIVITY_PAGE, active_account, visible_activity};
use quill::ids::AccountKey;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::activity::{self, Action};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::local_time::{CivilTime, chat_list_stamp, civil_local};
use quill::state::ReactionChoice;

/// Tools whose log entries name messages the agent created.
const SENDS: &[&str] = &[
    "send_message",
    "send_file",
    "send_poll",
    "send_sticker",
    "forward_messages",
];

/// What Undo would do for one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Undo {
    /// Delete the messages the agent's send made.
    Delete(Vec<i64>),
    /// Take back the reaction the agent added to this message.
    Unreact { message_id: i64, emoji: String },
}

fn passes(filter: &super::agent_page::ActivityFilter, action: &Action) -> bool {
    filter
        .agent
        .as_ref()
        .is_none_or(|agent| agent == &action.agent)
        && filter.chat.is_none_or(|chat| chat == action.chat_id)
        && filter.tool.as_ref().is_none_or(|tool| tool == &action.tool)
}

/// The distinct values of one field, in log order.
fn distinct<T: PartialEq + Clone>(log: &[Action], field: impl Fn(&Action) -> T) -> Vec<T> {
    let mut seen: Vec<T> = Vec::new();
    for action in log {
        let value = field(action);
        if !seen.contains(&value) {
            seen.push(value);
        }
    }
    seen
}

impl QuillApp {
    /// What Undo would do for `action`, from what Quill knows now: the
    /// messages still exist, or the reaction is still the user's. Only for
    /// the open account: Undo acts through its session.
    fn agent_undo_for(&self, action: &Action) -> Option<Undo> {
        if !action.ok || action.declined || action.undone || !self.agent_settings_on_open_account()
        {
            return None;
        }
        let session = self.session()?;
        let chat = ChatId(action.chat_id);
        if SENDS.contains(&action.tool.as_str()) {
            let alive: Vec<i64> = action
                .message_ids
                .iter()
                .copied()
                .filter(|id| !session.message_deleted(chat, MessageId(*id)))
                .collect();
            return (!alive.is_empty()).then_some(Undo::Delete(alive));
        }
        if action.tool == "react" {
            let message_id = *action.message_ids.first()?;
            let emoji = action.summary.trim().to_string();
            let chosen = session
                .histories
                .get(&action.chat_id)
                .and_then(|history| history.messages.get(&message_id))
                .is_some_and(|message| {
                    message.chosen_reaction(&ReactionChoice::Emoji(emoji.clone()))
                });
            return chosen.then_some(Undo::Unreact { message_id, emoji });
        }
        None
    }

    /// The Activity section: filters, rows, Show more and Clear.
    pub(super) fn agent_activity_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let account = self.agent_settings_account();
        let log = visible_activity(&account);
        if log.is_empty() {
            return group(
                None,
                [empty_line(
                    "agent-activity-empty",
                    tr(keys::Q_AGENT_ACTIVITY_EMPTY),
                    cx,
                )],
            )
            .into_any_element();
        }
        let filter = self.agent_ui.page.filter.clone();
        let shown_log: Vec<&Action> = log
            .iter()
            .filter(|action| passes(&filter, action))
            .collect();
        let now = civil_local(now_unix());
        let shown = self.agent_ui.activity_shown.max(ACTIVITY_PAGE);
        let more = shown_log.len() > shown;
        let mut rows: Vec<AnyElement> = if shown_log.is_empty() {
            vec![empty_line(
                "agent-activity-filtered",
                tr(keys::Q_AGENT_PAGE_FILTER_EMPTY),
                cx,
            )]
        } else {
            shown_log
                .iter()
                .take(shown)
                .enumerate()
                .map(|(index, action)| self.agent_activity_row(index, action, &now, cx))
                .collect()
        };
        rows.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .child(div().when(more, |this| {
                    this.child(
                        Button::new("agent-activity-more")
                            .label(tr(keys::Q_AGENT_ACTIVITY_MORE))
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.agent_ui.activity_shown += ACTIVITY_PAGE;
                                cx.notify();
                            })),
                    )
                }))
                .child(
                    Button::new("agent-activity-clear")
                        .label(tr(keys::Q_AGENT_ACTIVITY_CLEAR))
                        .ghost()
                        .small()
                        .tooltip(tr(keys::Q_AGENT_ACTIVITY_CLEAR_HINT))
                        .on_click(cx.listener(|this, _, _, cx| {
                            activity::clear();
                            this.agent_ui.activity_shown = ACTIVITY_PAGE;
                            this.agent_ui.page.filter = Default::default();
                            cx.notify();
                        })),
                )
                .into_any_element(),
        );
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.agent_activity_filters(&log, cx))
            .child(group(None, rows))
            .into_any_element()
    }

    /// Three menu buttons: agent, chat and action, each with "All".
    fn agent_activity_filters(&self, log: &[Action], cx: &mut Context<Self>) -> AnyElement {
        let filter = &self.agent_ui.page.filter;
        let owner = cx.entity().downgrade();
        let agents = distinct(log, |a| a.agent.clone());
        let chats = distinct(log, |a| (a.chat_id, a.chat_title.clone()));
        let tools = distinct(log, |a| a.tool.clone());
        let agent_label_now = filter.agent.as_deref().map_or_else(
            || tr(keys::Q_AGENT_PAGE_FILTER_AGENTS).to_string(),
            agent_label,
        );
        let chat_label_now = filter
            .chat
            .and_then(|id| chats.iter().find(|(chat, _)| *chat == id))
            .map_or_else(
                || tr(keys::Q_AGENT_PAGE_FILTER_CHATS).to_string(),
                |(_, t)| t.clone(),
            );
        let tool_label_now = filter.tool.as_deref().map_or_else(
            || tr(keys::Q_AGENT_PAGE_FILTER_TOOLS).to_string(),
            tool_label,
        );
        let agent_menu = {
            let owner = owner.clone();
            let current = filter.agent.clone();
            Button::new("agent-filter-agent")
                .label(agent_label_now)
                .outline()
                .small()
                .dropdown_caret(true)
                .dropdown_menu(move |mut menu, _, _| {
                    let choices = std::iter::once(None).chain(agents.iter().cloned().map(Some));
                    for choice in choices {
                        let owner = owner.clone();
                        let label = choice.as_deref().map_or_else(
                            || tr(keys::Q_AGENT_PAGE_FILTER_AGENTS).to_string(),
                            agent_label,
                        );
                        let checked = choice == current;
                        menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                            move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.agent_ui.page.filter.agent = choice.clone();
                                    cx.notify();
                                });
                            },
                        ));
                    }
                    menu
                })
        };
        let chat_menu = {
            let owner = owner.clone();
            let current = filter.chat;
            Button::new("agent-filter-chat")
                .label(chat_label_now)
                .outline()
                .small()
                .dropdown_caret(true)
                .dropdown_menu(move |mut menu, _, _| {
                    let all = (None, tr(keys::Q_AGENT_PAGE_FILTER_CHATS).to_string());
                    let choices = std::iter::once(all)
                        .chain(chats.iter().map(|(id, title)| (Some(*id), title.clone())));
                    for (choice, label) in choices {
                        let owner = owner.clone();
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .checked(choice == current)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        this.agent_ui.page.filter.chat = choice;
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                })
        };
        let tool_menu = {
            let current = filter.tool.clone();
            Button::new("agent-filter-tool")
                .label(tool_label_now)
                .outline()
                .small()
                .dropdown_caret(true)
                .dropdown_menu(move |mut menu, _, _| {
                    let choices = std::iter::once(None).chain(tools.iter().cloned().map(Some));
                    for choice in choices {
                        let owner = owner.clone();
                        let label = choice.as_deref().map_or_else(
                            || tr(keys::Q_AGENT_PAGE_FILTER_TOOLS).to_string(),
                            tool_label,
                        );
                        let checked = choice == current;
                        menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                            move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.agent_ui.page.filter.tool = choice.clone();
                                    cx.notify();
                                });
                            },
                        ));
                    }
                    menu
                })
        };
        div()
            .id("agent-activity-filters")
            .flex()
            .flex_wrap()
            .gap_2()
            .child(agent_menu)
            .child(chat_menu)
            .child(tool_menu)
            .into_any_element()
    }

    /// One action: what it was and where, who did it and what happened,
    /// the time, and Open and Undo.
    fn agent_activity_row(
        &self,
        index: usize,
        action: &Action,
        now: &CivilTime,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let stamp = chat_list_stamp(&civil_local(action.time), now);
        let what = if action.declined {
            tr(keys::Q_AGENT_ACTIVITY_DECLINED).to_string()
        } else {
            action.summary.clone()
        };
        let detail = if what.is_empty() {
            agent_label(&action.agent)
        } else {
            format!("{}: {what}", agent_label(&action.agent))
        };
        let undo = self.agent_undo_for(action);
        // Open goes to a chat of the open account.
        let can_open = action.chat_id != 0 && self.agent_settings_on_open_account();
        let entry = action.id;
        let shown = action.account.clone();
        let chat = action.chat_id;
        let message = action.message_ids.first().copied();
        let agent = agent_label(&action.agent);
        let chat_title = action.chat_title.clone();
        div()
            .id(SharedString::from(format!("agent-activity-{index}")))
            .debug_selector(move || format!("agent-activity-{index}"))
            .flex()
            .items_center()
            .gap_2()
            .w_full()
            .min_h(px(40.))
            .px_3()
            .py_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(div().text_sm().flex_none().child(tool_label(&action.tool)))
                            .child(
                                div()
                                    .text_sm()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(muted)
                                    .child(action.chat_title.clone()),
                            )
                            .when(!action.ok && !action.declined, |this| {
                                this.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "agent-activity-{index}-failed"
                                        )))
                                        .debug_selector(move || {
                                            format!("agent-activity-{index}-failed")
                                        })
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_xs()
                                        .text_color(danger)
                                        .child(Icon::new(IconName::TriangleAlert).xsmall())
                                        .child(tr(keys::Q_AGENT_ACTIVITY_FAILED)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .text_xs()
                            .text_color(muted)
                            .child(div().min_w_0().truncate().child(detail))
                            .child(div().flex_none().child(stamp)),
                    ),
            )
            .when(action.undone, |this| {
                this.child(
                    div().flex_none().child(
                        Tag::secondary()
                            .small()
                            .child(tr(keys::Q_AGENT_PAGE_UNDONE)),
                    ),
                )
            })
            .when_some(undo, |this, undo| {
                this.child(
                    Button::new(SharedString::from(format!("agent-activity-{index}-undo")))
                        .label(tr(keys::Q_AGENT_PAGE_UNDO))
                        .icon(IconName::Undo2)
                        .ghost()
                        .small()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.undo_agent_action(
                                &shown,
                                entry,
                                chat,
                                &undo,
                                &agent,
                                &chat_title,
                                window,
                                cx,
                            );
                        })),
                )
            })
            .when(can_open, |this| {
                this.child(
                    Button::new(SharedString::from(format!("agent-activity-{index}-open")))
                        .icon(IconName::ArrowUpRight)
                        .ghost()
                        .small()
                        .accessibility_label(tr(keys::Q_AGENT_PAGE_OPEN))
                        .tooltip(tr(keys::Q_AGENT_PAGE_OPEN))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_agent_action(chat, message, window, cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// Open: out of Settings, into the chat, at the message when there is
    /// one.
    fn open_agent_action(
        &mut self,
        chat_id: i64,
        message_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.agent_settings_on_open_account() {
            return;
        }
        self.close_settings();
        match message_id {
            Some(id) => self.go_to_message_in(ChatId(chat_id), MessageId(id), window, cx),
            None => self.select_listed_chat(ChatId(chat_id), window, cx),
        }
        cx.notify();
    }

    /// Undo: the normal delete box for the agent's own messages, or the
    /// normal reaction toggle. The entry is marked once the change is
    /// sent.
    #[allow(clippy::too_many_arguments)]
    fn undo_agent_action(
        &mut self,
        shown: &AccountKey,
        entry: u64,
        chat_id: i64,
        undo: &Undo,
        agent: &str,
        chat_title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if quill::agent::ui_driver::guard::refuse_if_injecting("undo what an agent did") {
            return;
        }
        // The entry's own account, not the page's: a click on a frame
        // drawn before an account switch must not act on the new account.
        if *shown != active_account() {
            return;
        }
        let account = shown.clone();
        match undo {
            Undo::Unreact { message_id, emoji } => {
                self.toggle_reaction(
                    ChatId(chat_id),
                    MessageId(*message_id),
                    ReactionChoice::Emoji(emoji.clone()),
                    cx,
                );
                activity::mark_undone(&account, entry);
            }
            Undo::Delete(ids) => {
                let ids: Vec<MessageId> = ids.iter().copied().map(MessageId).collect();
                let question = tr_with(
                    keys::Q_AGENT_PAGE_UNDO_DELETE,
                    &[("agent", agent), ("chat", chat_title)],
                );
                let app = cx.entity().downgrade();
                window.open_alert_dialog(cx, move |alert, _, _| {
                    let (app, ids, account) = (app.clone(), ids.clone(), account.clone());
                    alert
                        .description(question.clone())
                        .ok_text(tr(keys::BOX_DELETE))
                        .cancel_text(tr(keys::CANCEL))
                        .show_cancel(true)
                        .on_ok(move |_, _, cx| {
                            let ids = ids.clone();
                            let account = account.clone();
                            let _ = app.update(cx, |this, cx| {
                                // The open account may have changed while
                                // the box was up.
                                if active_account() != account {
                                    return;
                                }
                                this.delete_selection(ChatId(chat_id), &ids, true, cx);
                                activity::mark_undone(&account, entry);
                                cx.notify();
                            });
                            true
                        })
                });
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_activity_tests.rs"]
mod tests;
