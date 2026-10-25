//! Settings > Advanced > Agent access: the switch that lets local AI agents
//! use the account through Quill, and how to connect one
//! (`docs/decisions/agent-access.md`). The page reads the switch from
//! `quill::agent::access` on every render.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::rows::{group, switch_row};
use crate::ui::message_text::MONO_FONT;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::agent::access;
use quill::lang::{keys, tr};

/// The command that starts Quill's MCP server: this executable with `mcp`,
/// quoted for the shell it is pasted into. An AppImage runs from a temporary
/// mount, so the command names the AppImage file itself.
fn mcp_command() -> String {
    let appimage = cfg!(target_os = "linux")
        .then(|| std::env::var_os("APPIMAGE"))
        .flatten()
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from);
    let exe = appimage
        .or_else(|| std::env::current_exe().ok())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "quill".to_string());
    format!("{} mcp", shell_quote(&exe))
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

impl QuillApp {
    pub(super) fn agent_access_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = access::enabled();
        let muted = cx.theme().muted_foreground;
        let mut page = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .id("agent-access-about")
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_AGENT_ACCESS_ABOUT)),
            )
            .child(group(
                None,
                [switch_row(
                    "agent-access-switch",
                    tr(keys::Q_AGENT_ACCESS_ALLOW),
                    on.then(|| tr(keys::Q_AGENT_ACCESS_SCOPE)),
                    on,
                    cx.listener(|_, &next: &bool, _, cx| {
                        access::set_enabled(next);
                        cx.notify();
                    }),
                    cx,
                )],
            ));
        if on {
            page = page.child(self.agent_confirm_section(cx));
            page = page.children(self.agent_connected_section(cx));
            page = page.children(self.agent_grants_section(cx));
            let command = mcp_command();
            page = page.child(group(
                Some(tr(keys::Q_AGENT_ACCESS_CONNECT)),
                [
                    self.agent_command_row(
                        "claude-code",
                        tr(keys::Q_AGENT_ACCESS_CLAUDE_CODE),
                        tr(keys::Q_AGENT_ACCESS_CLAUDE_CODE_HINT),
                        format!("claude mcp add quill -- {command}"),
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
            ));
        }
        page.child(self.agent_activity_section(cx))
            .child(
                div()
                    .id("agent-access-terms")
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_AGENT_ACCESS_TERMS)),
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
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-agent-access").setup(
        |app, _, _| {
            app.open_settings(None);
            app.settings.page = Some(quill::settings_catalog::Page::Advanced);
            app.settings.sub = Some(super::SubPage::AgentAccess);
        }
    ),
];

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_access_tests.rs"]
mod tests;
