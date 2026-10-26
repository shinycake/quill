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

/// This executable, quoted for the shell the line is pasted into. An
/// AppImage runs from a temporary mount, so the line names the AppImage
/// file itself.
fn quill_exe() -> String {
    let appimage = cfg!(target_os = "linux")
        .then(|| std::env::var_os("APPIMAGE"))
        .flatten()
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from);
    let exe = appimage
        .or_else(|| std::env::current_exe().ok())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "quill".to_string());
    shell_quote(&exe)
}

/// The command that starts Quill's MCP server.
fn mcp_command() -> String {
    format!("{} mcp", quill_exe())
}

/// Quote `word` for a POSIX shell, or for PowerShell and cmd on Windows,
/// where paths cannot contain `"`.
fn shell_quote(word: &str) -> String {
    let plain =
        |c: char| c.is_ascii_alphanumeric() || "/._-+:".contains(c) || (cfg!(windows) && c == '\\');
    if !word.is_empty() && word.chars().all(plain) {
        word.to_string()
    } else if cfg!(windows) {
        format!("\"{word}\"")
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

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
            cx.listener(|_, &next: &bool, _, cx| {
                access::set_enabled(next);
                cx.notify();
            }),
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
        let exe = quill_exe();
        let command = mcp_command();
        group(
            None,
            [
                self.agent_command_row(
                    "claude-code",
                    tr(keys::Q_AGENT_ACCESS_CLAUDE_CODE),
                    tr(keys::Q_AGENT_ACCESS_CLAUDE_CODE_HINT),
                    format!("claude mcp add quill -- {command}"),
                    cx,
                ),
                self.agent_command_row(
                    "claude-desktop",
                    tr(keys::Q_AGENT_PAGE_CLAUDE_DESKTOP),
                    tr(keys::Q_AGENT_PAGE_CLAUDE_DESKTOP_HINT),
                    format!("{exe} agent connect claude-desktop --yes"),
                    cx,
                ),
                self.agent_command_row(
                    "cursor",
                    tr(keys::Q_AGENT_PAGE_CURSOR),
                    tr(keys::Q_AGENT_PAGE_CURSOR_HINT),
                    format!("{exe} agent connect cursor --yes"),
                    cx,
                ),
                self.agent_command_row(
                    "other-apps",
                    tr(keys::Q_AGENT_ACCESS_OTHER_APPS),
                    tr(keys::Q_AGENT_ACCESS_OTHER_APPS_HINT),
                    command,
                    cx,
                ),
            ],
        )
        .into_any_element()
    }

    /// A titled command in a monospace well, with Copy.
    fn agent_command_row(
        &self,
        id: &'static str,
        title: &'static str,
        hint: &'static str,
        command: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let copied = command.clone();
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .px_3()
            .py_2()
            .child(div().text_sm().child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(hint),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id(SharedString::from(format!("agent-command-{id}")))
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
                        Button::new(SharedString::from(format!("agent-copy-{id}")))
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
