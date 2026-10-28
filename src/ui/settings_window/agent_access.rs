//! Agent access, the Connection and Connect sections: the switch, Reset
//! token, the agents connected now (with Revoke) and those the user keeps
//! out, and the lines that set an agent up. Everything is read from the
//! access summary on each render.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::agent_tools::tool_name;
use super::rows::{group, switch_row};
use crate::ui::agent_labels::{agent_label, since_label};
use crate::ui::message_text::MONO_FONT;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::agent::summary::{AccessSummary, Agent};
use quill::agent::{access, activity};
use quill::lang::{keys, tr, tr_with};

/// A row with a title and caption on the left and one control on the
/// right.
pub(super) fn action_row(
    title: impl Into<SharedString>,
    caption: Option<SharedString>,
    control: impl IntoElement,
    cx: &App,
) -> AnyElement {
    div()
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
                .child(div().text_sm().child(title.into()))
                .children(caption.map(|caption| {
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(caption)
                })),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

/// A quiet line of text inside a group, for an empty state.
pub(super) fn empty_line(id: &'static str, text: &'static str, cx: &App) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .px_3()
        .py_2()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}

impl QuillApp {
    pub(super) fn agent_connection_section(
        &self,
        summary: &AccessSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = summary.enabled;
        let mut rows = vec![switch_row(
            "agent-access-switch",
            tr(keys::Q_AGENT_ACCESS_ALLOW),
            Some(tr(keys::Q_AGENT_PAGE_ALLOW_HINT)),
            on,
            cx.listener(|this, &next: &bool, _, cx| this.set_agent_access(next, cx)),
            cx,
        )];
        if on {
            rows.push(action_row(
                tr(keys::Q_AGENT_PAGE_RESET),
                Some(tr(keys::Q_AGENT_PAGE_RESET_HINT).into()),
                Button::new("agent-reset-token")
                    .label(tr(keys::Q_AGENT_PAGE_RESET_BUTTON))
                    .outline()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.agent_ui.page.reset_confirm = true;
                        cx.notify();
                    })),
                cx,
            ));
        }
        let mut section = div().flex().flex_col().gap_4().child(group(None, rows));
        if on {
            let agents: Vec<AnyElement> = if summary.agents.is_empty() {
                vec![empty_line(
                    "agent-connected-empty",
                    tr(keys::Q_AGENT_PAGE_CONNECTED_NONE),
                    cx,
                )]
            } else {
                summary
                    .agents
                    .iter()
                    .enumerate()
                    .map(|(index, agent)| connected_row(index, agent, cx))
                    .collect()
            };
            section = section.child(group(Some(tr(keys::Q_AGENT_CONNECTED_TITLE)), agents));
            if !summary.refused.is_empty() {
                let rows: Vec<AnyElement> = summary
                    .refused
                    .iter()
                    .enumerate()
                    .map(|(index, name)| {
                        let agent = name.clone();
                        action_row(
                            agent_label(name),
                            Some(tr(keys::Q_AGENT_PAGE_REFUSED_HINT).into()),
                            Button::new(SharedString::from(format!("agent-allow-again-{index}")))
                                .label(tr(keys::Q_AGENT_PAGE_ALLOW_AGAIN))
                                .outline()
                                .small()
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    activity::allow_again(&agent);
                                    cx.notify();
                                })),
                            cx,
                        )
                    })
                    .collect();
                section = section.child(group(Some(tr(keys::Q_AGENT_PAGE_REFUSED_TITLE)), rows));
            }
        }
        section.into_any_element()
    }

    /// "Connect an agent": a line per app, with Copy. The Claude Desktop
    /// and Cursor lines run `quill agent connect`, which writes the app's
    /// MCP config.
    pub(super) fn agent_connect_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let lines = quill::agent::cli::setup_lines();
        group(
            None,
            [
                self.agent_setup_prompt_row(cx),
                self.agent_command_row(
                    "claude-code",
                    tr(keys::Q_AGENT_ACCESS_CLAUDE_CODE),
                    tr(keys::Q_AGENT_ACCESS_CLAUDE_CODE_HINT),
                    lines.claude_code,
                    cx,
                ),
                self.agent_command_row(
                    "claude-desktop",
                    tr(keys::Q_AGENT_PAGE_CLAUDE_DESKTOP),
                    tr(keys::Q_AGENT_PAGE_CLAUDE_DESKTOP_HINT),
                    lines.claude_desktop,
                    cx,
                ),
                self.agent_command_row(
                    "cursor",
                    tr(keys::Q_AGENT_PAGE_CURSOR),
                    tr(keys::Q_AGENT_PAGE_CURSOR_HINT),
                    lines.cursor,
                    cx,
                ),
                self.agent_command_row(
                    "other-apps",
                    tr(keys::Q_AGENT_ACCESS_OTHER_APPS),
                    tr(keys::Q_AGENT_ACCESS_OTHER_APPS_HINT),
                    lines.mcp,
                    cx,
                ),
            ],
        )
        .into_any_element()
    }

    /// A titled command in a monospace well, with Copy: one well per shell
    /// when PowerShell and Command Prompt need different lines.
    fn agent_command_row(
        &self,
        id: &'static str,
        title: &'static str,
        hint: &'static str,
        commands: Vec<(&'static str, String)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut row = div()
            .flex()
            .flex_col()
            .gap_1p5()
            .px_3()
            .py_2()
            .child(div().text_sm().child(title))
            .child(div().text_xs().text_color(muted).child(hint));
        for (index, (shell, command)) in commands.into_iter().enumerate() {
            let key = if index == 0 {
                id.to_string()
            } else {
                format!("{id}-{index}")
            };
            if !shell.is_empty() {
                row = row.child(div().text_xs().text_color(muted).child(shell));
            }
            let copied = command.clone();
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id(SharedString::from(format!("agent-command-{key}")))
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .py_1p5()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().muted)
                            .font_family(MONO_FONT)
                            .text_xs()
                            .child(command),
                    )
                    .child(
                        Button::new(SharedString::from(format!("agent-copy-{key}")))
                            .label(tr(keys::Q_AGENT_ACCESS_COPY))
                            .outline()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                                this.connection
                                    .set_note(StatusNote::info(tr(keys::Q_AGENT_ACCESS_COPIED)));
                                cx.notify();
                            })),
                    ),
            );
        }
        row.into_any_element()
    }

    /// The prompt that has an agent connect itself, the same text
    /// `quill agent setup-prompt` prints.
    fn agent_setup_prompt_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let prompt = quill::agent::cli::setup_prompt();
        let copied = prompt.clone();
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .px_3()
            .py_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(div().text_sm().child(tr(keys::Q_AGENT_PAGE_SETUP_PROMPT)))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(tr(keys::Q_AGENT_PAGE_SETUP_PROMPT_HINT)),
                            ),
                    )
                    .child(
                        Button::new("agent-copy-setup-prompt")
                            .label(tr(keys::Q_AGENT_PAGE_COPY_PROMPT))
                            .outline()
                            .small()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                                this.connection
                                    .set_note(StatusNote::info(tr(keys::Q_AGENT_ACCESS_COPIED)));
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .id("agent-setup-prompt")
                    .max_h(rems(9.))
                    .overflow_y_scroll()
                    .px_2()
                    .py_1p5()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().muted)
                    .font_family(MONO_FONT)
                    .text_xs()
                    .child(prompt),
            )
            .into_any_element()
    }

    /// Reset token, once the user said yes in its dialog.
    pub(in crate::ui) fn confirm_agent_token_reset(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_if_injecting("reset the agent token") {
            return;
        }
        self.agent_ui.page.reset_confirm = false;
        access::reset_token();
        self.connection
            .set_note(StatusNote::info(tr(keys::Q_AGENT_PAGE_RESET_DONE)));
        cx.notify();
    }
}

/// One connected agent: who, since when, the tools it used, and Revoke.
fn connected_row(index: usize, agent: &Agent, cx: &mut Context<QuillApp>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let tools = if agent.tools.is_empty() {
        tr(keys::Q_AGENT_PAGE_AGENT_TOOLS_NONE).to_string()
    } else {
        let names: Vec<&str> = agent.tools.iter().map(|tool| tool_name(tool)).collect();
        tr_with(
            keys::Q_AGENT_PAGE_AGENT_TOOLS,
            &[("tools", &names.join(", "))],
        )
    };
    let id = agent.id;
    div()
        .id(SharedString::from(format!("agent-connected-{id}")))
        .debug_selector(move || format!("agent-connected-row-{index}"))
        .flex()
        .items_center()
        .gap_3()
        .w_full()
        .min_h(px(40.))
        .px_3()
        .py_2()
        .child(
            Icon::new(IconName::Bot)
                .small()
                .text_color(muted)
                .flex_none(),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_2()
                        .child(div().text_sm().truncate().child(agent_label(&agent.name)))
                        .child(
                            div()
                                .flex_none()
                                .text_xs()
                                .text_color(muted)
                                .child(since_label(agent.since)),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .whitespace_normal()
                        .child(tools),
                ),
        )
        .child(
            Button::new(SharedString::from(format!("agent-revoke-{index}")))
                .label(tr(keys::Q_AGENT_REVOKE))
                .outline()
                .small()
                .tooltip(tr(keys::Q_AGENT_PAGE_REVOKE_HINT))
                .on_click(cx.listener(move |_, _, _, cx| {
                    activity::revoke(id);
                    cx.notify();
                })),
        )
        .into_any_element()
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_access_tests.rs"]
mod tests;
