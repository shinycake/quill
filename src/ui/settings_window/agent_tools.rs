//! Agent access, "What agents can do" and "Always asks you": every tool
//! from the MCP registry with its live state, grouped Read, Write, Raw
//! TDLib calls and Window control, then what Quill always asks about and
//! what no agent can do. States, reasons and fixes come from the access
//! summary; a group has a switch only where Quill has one (raw calls,
//! window control).
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::agent_access::action_row;
use super::agent_page::Spot;
use super::rows::{group, switch_row};
use gpui_kit::component::button::*;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::agent::access;
use quill::agent::summary::{
    AccessSummary, AskKind, Asks, Fix, NEVER, Never, OffReason, Section, Tool, ToolState,
};
use quill::agent::tools::ToolGroup;
use quill::lang::{Key, keys, tr};

/// A tool's name as people read it: `list_chats` is "List chats".
pub(super) fn tool_name(tool: &str) -> &'static str {
    tool_words(tool).map_or("", |(name, _)| tr(name))
}

fn tool_hint(tool: &str) -> &'static str {
    tool_words(tool).map_or("", |(_, hint)| tr(hint))
}

fn tool_words(tool: &str) -> Option<(Key, Key)> {
    Some(match tool {
        "quill_status" => (
            keys::Q_AGENT_PAGE_TOOL_QUILL_STATUS,
            keys::Q_AGENT_PAGE_TOOL_QUILL_STATUS_HINT,
        ),
        "list_chats" => (
            keys::Q_AGENT_PAGE_TOOL_LIST_CHATS,
            keys::Q_AGENT_PAGE_TOOL_LIST_CHATS_HINT,
        ),
        "list_folders" => (
            keys::Q_AGENT_PAGE_TOOL_LIST_FOLDERS,
            keys::Q_AGENT_PAGE_TOOL_LIST_FOLDERS_HINT,
        ),
        "list_unread" => (
            keys::Q_AGENT_PAGE_TOOL_LIST_UNREAD,
            keys::Q_AGENT_PAGE_TOOL_LIST_UNREAD_HINT,
        ),
        "list_messages" => (
            keys::Q_AGENT_PAGE_TOOL_LIST_MESSAGES,
            keys::Q_AGENT_PAGE_TOOL_LIST_MESSAGES_HINT,
        ),
        "read_history" => (
            keys::Q_AGENT_PAGE_TOOL_READ_HISTORY,
            keys::Q_AGENT_PAGE_TOOL_READ_HISTORY_HINT,
        ),
        "search_messages" => (
            keys::Q_AGENT_PAGE_TOOL_SEARCH_MESSAGES,
            keys::Q_AGENT_PAGE_TOOL_SEARCH_MESSAGES_HINT,
        ),
        "get_message" => (
            keys::Q_AGENT_PAGE_TOOL_GET_MESSAGE,
            keys::Q_AGENT_PAGE_TOOL_GET_MESSAGE_HINT,
        ),
        "get_chat_info" => (
            keys::Q_AGENT_PAGE_TOOL_GET_CHAT_INFO,
            keys::Q_AGENT_PAGE_TOOL_GET_CHAT_INFO_HINT,
        ),
        "list_members" => (
            keys::Q_AGENT_PAGE_TOOL_LIST_MEMBERS,
            keys::Q_AGENT_PAGE_TOOL_LIST_MEMBERS_HINT,
        ),
        "list_contacts" => (
            keys::Q_AGENT_PAGE_TOOL_LIST_CONTACTS,
            keys::Q_AGENT_PAGE_TOOL_LIST_CONTACTS_HINT,
        ),
        "download_file" => (
            keys::Q_AGENT_PAGE_TOOL_DOWNLOAD_FILE,
            keys::Q_AGENT_PAGE_TOOL_DOWNLOAD_FILE_HINT,
        ),
        "wait_for_message" => (
            keys::Q_AGENT_PAGE_TOOL_WAIT_FOR_MESSAGE,
            keys::Q_AGENT_PAGE_TOOL_WAIT_FOR_MESSAGE_HINT,
        ),
        "request_access" => (
            keys::Q_AGENT_PAGE_TOOL_REQUEST_ACCESS,
            keys::Q_AGENT_PAGE_TOOL_REQUEST_ACCESS_HINT,
        ),
        "send_message" => (
            keys::Q_AGENT_PAGE_TOOL_SEND_MESSAGE,
            keys::Q_AGENT_PAGE_TOOL_SEND_MESSAGE_HINT,
        ),
        "send_file" => (
            keys::Q_AGENT_PAGE_TOOL_SEND_FILE,
            keys::Q_AGENT_PAGE_TOOL_SEND_FILE_HINT,
        ),
        "send_poll" => (
            keys::Q_AGENT_PAGE_TOOL_SEND_POLL,
            keys::Q_AGENT_PAGE_TOOL_SEND_POLL_HINT,
        ),
        "send_sticker" => (
            keys::Q_AGENT_PAGE_TOOL_SEND_STICKER,
            keys::Q_AGENT_PAGE_TOOL_SEND_STICKER_HINT,
        ),
        "forward_messages" => (
            keys::Q_AGENT_PAGE_TOOL_FORWARD_MESSAGES,
            keys::Q_AGENT_PAGE_TOOL_FORWARD_MESSAGES_HINT,
        ),
        "edit_message" => (
            keys::Q_AGENT_PAGE_TOOL_EDIT_MESSAGE,
            keys::Q_AGENT_PAGE_TOOL_EDIT_MESSAGE_HINT,
        ),
        "delete_messages" => (
            keys::Q_AGENT_PAGE_TOOL_DELETE_MESSAGES,
            keys::Q_AGENT_PAGE_TOOL_DELETE_MESSAGES_HINT,
        ),
        "react" => (keys::Q_AGENT_PAGE_TOOL_REACT, keys::Q_AGENT_PAGE_TOOL_REACT_HINT),
        "pin_message" => (
            keys::Q_AGENT_PAGE_TOOL_PIN_MESSAGE,
            keys::Q_AGENT_PAGE_TOOL_PIN_MESSAGE_HINT,
        ),
        "set_draft" => (
            keys::Q_AGENT_PAGE_TOOL_SET_DRAFT,
            keys::Q_AGENT_PAGE_TOOL_SET_DRAFT_HINT,
        ),
        "mark_read" => (
            keys::Q_AGENT_PAGE_TOOL_MARK_READ,
            keys::Q_AGENT_PAGE_TOOL_MARK_READ_HINT,
        ),
        "mute_chat" => (
            keys::Q_AGENT_PAGE_TOOL_MUTE_CHAT,
            keys::Q_AGENT_PAGE_TOOL_MUTE_CHAT_HINT,
        ),
        "archive_chat" => (
            keys::Q_AGENT_PAGE_TOOL_ARCHIVE_CHAT,
            keys::Q_AGENT_PAGE_TOOL_ARCHIVE_CHAT_HINT,
        ),
        "set_chat_folder" => (
            keys::Q_AGENT_PAGE_TOOL_SET_CHAT_FOLDER,
            keys::Q_AGENT_PAGE_TOOL_SET_CHAT_FOLDER_HINT,
        ),
        "td_call" => (keys::Q_AGENT_PAGE_TOOL_TD_CALL, keys::Q_AGENT_PAGE_TOOL_TD_CALL_HINT),
        "read_ui_tree" => (
            keys::Q_AGENT_PAGE_TOOL_READ_UI_TREE,
            keys::Q_AGENT_PAGE_TOOL_READ_UI_TREE_HINT,
        ),
        "take_screenshot" => (
            keys::Q_AGENT_PAGE_TOOL_TAKE_SCREENSHOT,
            keys::Q_AGENT_PAGE_TOOL_TAKE_SCREENSHOT_HINT,
        ),
        "click_element" => (
            keys::Q_AGENT_PAGE_TOOL_CLICK_ELEMENT,
            keys::Q_AGENT_PAGE_TOOL_CLICK_ELEMENT_HINT,
        ),
        "type_text" => (
            keys::Q_AGENT_PAGE_TOOL_TYPE_TEXT,
            keys::Q_AGENT_PAGE_TOOL_TYPE_TEXT_HINT,
        ),
        "press_keys" => (
            keys::Q_AGENT_PAGE_TOOL_PRESS_KEYS,
            keys::Q_AGENT_PAGE_TOOL_PRESS_KEYS_HINT,
        ),
        "scroll_view" => (
            keys::Q_AGENT_PAGE_TOOL_SCROLL_VIEW,
            keys::Q_AGENT_PAGE_TOOL_SCROLL_VIEW_HINT,
        ),
        "hover_element" => (
            keys::Q_AGENT_PAGE_TOOL_HOVER_ELEMENT,
            keys::Q_AGENT_PAGE_TOOL_HOVER_ELEMENT_HINT,
        ),
        "open_menu" => (
            keys::Q_AGENT_PAGE_TOOL_OPEN_MENU,
            keys::Q_AGENT_PAGE_TOOL_OPEN_MENU_HINT,
        ),
        _ => return None,
    })
}

fn group_words(group: ToolGroup) -> (&'static str, &'static str) {
    match group {
        ToolGroup::Read => (
            tr(keys::Q_AGENT_PAGE_GROUP_READ),
            tr(keys::Q_AGENT_PAGE_GROUP_READ_HINT),
        ),
        ToolGroup::Write => (
            tr(keys::Q_AGENT_PAGE_GROUP_WRITE),
            tr(keys::Q_AGENT_PAGE_GROUP_WRITE_HINT),
        ),
        ToolGroup::Raw => (
            tr(keys::Q_AGENT_PAGE_GROUP_RAW),
            tr(keys::Q_AGENT_PAGE_GROUP_RAW_HINT),
        ),
        ToolGroup::Window => (
            tr(keys::Q_AGENT_PAGE_GROUP_WINDOW),
            tr(keys::Q_AGENT_PAGE_GROUP_WINDOW_HINT),
        ),
    }
}

fn off_reason(reason: OffReason) -> &'static str {
    tr(match reason {
        OffReason::AccessOff => keys::Q_AGENT_PAGE_OFF_ACCESS,
        OffReason::ContactsOff => keys::Q_AGENT_PAGE_OFF_CONTACTS,
        OffReason::NothingWritable => keys::Q_AGENT_PAGE_OFF_WRITABLE,
        OffReason::RawCallsOff => keys::Q_AGENT_PAGE_OFF_RAW,
        OffReason::WindowControlOff => keys::Q_AGENT_PAGE_OFF_WINDOW,
        OffReason::Paused => keys::Q_AGENT_PAGE_OFF_PAUSED,
        OffReason::Stopped => keys::Q_AGENT_PAGE_OFF_STOPPED,
        OffReason::NoCapture => keys::Q_AGENT_PAGE_OFF_CAPTURE,
    })
}

fn asks_line(asks: Asks) -> &'static str {
    tr(match asks {
        Asks::EachSend => keys::Q_AGENT_PAGE_ASKS_EACH_SEND,
        Asks::Always => keys::Q_AGENT_PAGE_ASKS_ALWAYS,
        Asks::RiskyCalls => keys::Q_AGENT_PAGE_ASKS_RISKY,
    })
}

fn fix_label(fix: Fix) -> &'static str {
    tr(match fix {
        Fix::TurnOnAccess | Fix::TurnOnRawCalls | Fix::TurnOnWindowControl => {
            keys::Q_AGENT_PAGE_TURN_ON
        }
        Fix::TurnOnContacts => keys::Q_AGENT_PAGE_SHARE_CONTACTS,
        Fix::GrantWriting => keys::Q_AGENT_PAGE_CHOOSE_CHATS,
        Fix::LetContinue => keys::Q_AGENT_TAKEOVER_CONTINUE,
        Fix::RestartWindowControl => keys::Q_AGENT_PAGE_ALLOW_AGAIN,
    })
}

/// What a group's switch turns on, for the groups that have one.
fn group_switch(group: ToolGroup) -> Option<Fix> {
    match group {
        ToolGroup::Raw => Some(Fix::TurnOnRawCalls),
        ToolGroup::Window => Some(Fix::TurnOnWindowControl),
        ToolGroup::Read | ToolGroup::Write => None,
    }
}

fn fix_name(fix: Fix) -> &'static str {
    match fix {
        Fix::TurnOnAccess => "access",
        Fix::TurnOnContacts => "contacts",
        Fix::GrantWriting => "writing",
        Fix::TurnOnRawCalls => "raw",
        Fix::TurnOnWindowControl => "window",
        Fix::LetContinue => "continue",
        Fix::RestartWindowControl => "restart",
    }
}

fn ask_words(kind: AskKind) -> (&'static str, &'static str) {
    let (title, hint) = match kind {
        AskKind::NewAccess => (
            keys::Q_AGENT_PAGE_ASK_ACCESS,
            keys::Q_AGENT_PAGE_ASK_ACCESS_HINT,
        ),
        AskKind::BeyondOneChat => (
            keys::Q_AGENT_PAGE_ASK_BEYOND,
            keys::Q_AGENT_PAGE_ASK_BEYOND_HINT,
        ),
        AskKind::AccountAndSessions => (
            keys::Q_AGENT_PAGE_ASK_ACCOUNT,
            keys::Q_AGENT_PAGE_ASK_ACCOUNT_HINT,
        ),
        AskKind::MoneyStarsGifts => (
            keys::Q_AGENT_PAGE_ASK_MONEY,
            keys::Q_AGENT_PAGE_ASK_MONEY_HINT,
        ),
        AskKind::EachSend => (
            keys::Q_AGENT_PAGE_ASK_EACH_SEND,
            keys::Q_AGENT_PAGE_ASK_EACH_SEND_HINT,
        ),
    };
    (tr(title), tr(hint))
}

fn never_words(never: Never) -> (&'static str, &'static str) {
    let (title, hint) = match never {
        Never::HiddenStatus => (
            keys::Q_AGENT_PAGE_NEVER_HIDDEN,
            keys::Q_AGENT_PAGE_NEVER_HIDDEN_HINT,
        ),
        Never::BulkExport => (
            keys::Q_AGENT_PAGE_NEVER_EXPORT,
            keys::Q_AGENT_PAGE_NEVER_EXPORT_HINT,
        ),
        Never::SettingsAndLogout => (
            keys::Q_AGENT_PAGE_NEVER_SETTINGS,
            keys::Q_AGENT_PAGE_NEVER_SETTINGS_HINT,
        ),
        Never::PaidMessages => (
            keys::Q_AGENT_PAGE_NEVER_PAID,
            keys::Q_AGENT_PAGE_NEVER_PAID_HINT,
        ),
    };
    (tr(title), tr(hint))
}

impl QuillApp {
    /// One group per kind of tool, each tool with its state.
    pub(super) fn agent_tools_section(
        &self,
        summary: &AccessSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let groups = ToolGroup::ALL.into_iter().map(|kind| {
            let (title, hint) = group_words(kind);
            let mut rows = vec![
                div()
                    .px_3()
                    .pt_1()
                    .pb_2()
                    .text_xs()
                    .text_color(muted)
                    .child(hint)
                    .into_any_element(),
            ];
            match kind {
                ToolGroup::Raw => rows.push(switch_row(
                    "agent-raw-calls-switch",
                    tr(keys::Q_AGENT_PAGE_RAW_SWITCH),
                    Some(tr(keys::Q_AGENT_PAGE_RAW_SWITCH_HINT)),
                    summary.raw_calls,
                    cx.listener(|_, &next: &bool, _, cx| {
                        access::set_raw_calls(next);
                        cx.notify();
                    }),
                    cx,
                )),
                ToolGroup::Window => rows.push(switch_row(
                    "agent-tools-window-switch",
                    tr(keys::Q_AGENT_UI_DRIVER),
                    None,
                    summary.window.switch,
                    cx.listener(|_, &next: &bool, _, cx| {
                        access::set_ui_driver(next);
                        cx.notify();
                    }),
                    cx,
                )),
                ToolGroup::Read | ToolGroup::Write => {}
            }
            rows.extend(
                summary
                    .tools
                    .iter()
                    .filter(|tool| tool.group == kind)
                    .map(|tool| self.agent_tool_row(tool, cx)),
            );
            div()
                .id(SharedString::from(format!("agent-tools-{}", kind.as_str())))
                .debug_selector(move || format!("agent-tools-{}", kind.as_str()))
                .child(group(Some(title), rows))
        });
        div()
            .flex()
            .flex_col()
            .gap_4()
            .children(groups)
            .into_any_element()
    }

    fn agent_tool_row(&self, tool: &Tool, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let (tag, line, fix) = match tool.state {
            ToolState::Available => (
                Tag::color(ColorName::Green).child(tr(keys::Q_AGENT_PAGE_STATE_AVAILABLE)),
                None,
                None,
            ),
            ToolState::AsksFirst(asks) => (
                Tag::color(ColorName::Amber).child(tr(keys::Q_AGENT_PAGE_STATE_ASKS)),
                Some(asks_line(asks)),
                None,
            ),
            // The group's own switch sits right above and says why.
            ToolState::Off(reason)
                if reason.fix().is_some() && reason.fix() == group_switch(tool.group) =>
            {
                (
                    Tag::secondary().child(tr(keys::Q_AGENT_PAGE_STATE_OFF)),
                    None,
                    None,
                )
            }
            ToolState::Off(reason) => (
                Tag::secondary().child(tr(keys::Q_AGENT_PAGE_STATE_OFF)),
                Some(off_reason(reason)),
                reason.fix(),
            ),
        };
        let name = tool.name;
        div()
            .id(SharedString::from(format!("agent-tool-{name}")))
            .debug_selector(move || format!("agent-tool-{name}"))
            .flex()
            .items_center()
            .gap_3()
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
                    .child(div().text_sm().child(tool_name(name)))
                    .child(div().text_xs().text_color(muted).child(tool_hint(name)))
                    .when_some(line, |this, line| {
                        this.child(div().text_xs().text_color(muted).child(line))
                    }),
            )
            .when_some(fix, |this, fix| {
                this.child(
                    Button::new(SharedString::from(format!(
                        "agent-tool-{name}-fix-{}",
                        fix_name(fix)
                    )))
                    .label(fix_label(fix))
                    .outline()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| this.apply_agent_fix(fix, cx))),
                )
            })
            .child(div().flex_none().child(tag.small()))
            .into_any_element()
    }

    /// The button next to an off tool: the one change that turns it on.
    pub(super) fn apply_agent_fix(&mut self, fix: Fix, cx: &mut Context<Self>) {
        use quill::agent::ui_driver::takeover;
        match fix {
            Fix::TurnOnAccess => access::set_enabled(true),
            Fix::TurnOnContacts => self.set_agent_contacts(true, cx),
            Fix::GrantWriting => {
                self.agent_ui.page.scroll_to = Some(Spot::Section(Section::Chats));
            }
            Fix::TurnOnRawCalls => access::set_raw_calls(true),
            Fix::TurnOnWindowControl => access::set_ui_driver(true),
            Fix::LetContinue => takeover::let_continue(),
            Fix::RestartWindowControl => {
                access::set_ui_driver(false);
                access::set_ui_driver(true);
            }
        }
        cx.notify();
    }

    /// "Always asks you", then "Never allowed".
    pub(super) fn agent_asks_section(
        &self,
        summary: &AccessSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let asks: Vec<AnyElement> = summary
            .always_asks
            .iter()
            .map(|ask| {
                let (title, hint) = ask_words(ask.kind);
                let caption: SharedString = if ask.reachable {
                    hint.into()
                } else {
                    format!("{hint} {}", tr(keys::Q_AGENT_PAGE_ASK_UNREACHABLE_RAW)).into()
                };
                action_row(
                    title,
                    Some(caption),
                    Tag::color(ColorName::Amber)
                        .small()
                        .child(tr(keys::Q_AGENT_PAGE_STATE_ASKS)),
                    cx,
                )
            })
            .collect();
        let never = NEVER.into_iter().map(|never| {
            let (title, hint) = never_words(never);
            action_row(
                title,
                Some(hint.into()),
                Tag::secondary()
                    .small()
                    .child(tr(keys::Q_AGENT_PAGE_STATE_OFF)),
                cx,
            )
        });
        let _ = muted;
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .id("agent-always-asks")
                    .debug_selector(|| "agent-always-asks".to_string())
                    .child(group(None, asks)),
            )
            .child(
                div()
                    .id("agent-never")
                    .debug_selector(|| "agent-never".to_string())
                    .child(group(Some(tr(keys::Q_AGENT_PAGE_NEVER_TITLE)), never)),
            )
            .into_any_element()
    }
}
