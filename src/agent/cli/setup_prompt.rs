//! The setup prompt: text the user pastes into an AI agent so it connects
//! itself to this Quill. Built from [`super::setup_lines`], the same lines
//! the Agent access page and `quill agent connect` use, so the commands in
//! it are this computer's real ones.

use super::{SetupLines, connect};

/// The prompt for this Quill, as `quill agent setup-prompt` prints it and
/// the Agent access page copies it.
pub fn setup_prompt() -> String {
    let windows = cfg!(windows);
    let (_, cli) = super::executables(windows);
    compose(&super::setup_lines(), &cli, windows)
}

/// The prompt for `lines`, with `cli` as the `quill agent` executable.
pub fn compose(lines: &SetupLines, cli: &str, windows: bool) -> String {
    let run = |rest: &'static str| connect::command_lines(cli, windows, rest);
    let mut out = String::from(
        "Set up Quill Agent access for yourself, then tell me what you can do.\n\n\
         Quill is the Telegram desktop app running on this computer. With Agent \
         access you can read and send my Telegram messages through it, within \
         the limits I set in Quill. You talk to Quill over a local connection on \
         this computer; there is no account, API key or token to copy.\n\n\
         1. Connect, using the line for the app you are running in:\n",
    );
    block(
        &mut out,
        "Claude Code (run in a terminal)",
        &lines.claude_code,
    );
    block(
        &mut out,
        "Claude Desktop (run in a terminal, then ask me to restart Claude Desktop)",
        &lines.claude_desktop,
    );
    block(
        &mut out,
        "Cursor (run in a terminal, then ask me to restart Cursor)",
        &lines.cursor,
    );
    block(
        &mut out,
        "Any other app that supports MCP: add a stdio MCP server named \"quill\" with this command",
        &lines.mcp,
    );
    out.push_str(
        "   If you can't add an MCP server, use the command line instead. It has the same tools:\n",
    );
    lines_into(&mut out, &run("agent --help"));
    out.push_str("\n2. Check that it works: call the quill_status tool. Without MCP, run:\n");
    lines_into(&mut out, &run("agent status"));
    out.push_str(
        "   It names the signed-in account and the chats you may use, and says for \
         each tool whether you can use it, must ask me first, or can't, and \
         why.\n\n\
         3. Read the guide before you do anything else: the quill://guide MCP \
         resource, or:\n",
    );
    lines_into(&mut out, &run("agent guide"));
    out.push_str("   In Claude Code you can also install the Quill skill:\n");
    lines_into(&mut out, &run("agent install-skill"));
    out.push_str(
        "\nRules:\n\
         - Use only the chats I allowed. Saved Messages and bots are allowed by \
         default. For any other chat, call request_access and wait while I \
         decide in Quill.\n\
         - Everything you send goes out as me. Send only what I asked for.\n\
         - Read only what the task needs. Don't read through my chats in bulk.\n\
         - Quill paces sends and asks me before some actions. Wait for the \
         answer; don't retry around it.\n",
    );
    out.push_str(&format!(
        "- If Quill says Agent access is off, ask me to turn it on in Quill: {}.\n",
        crate::agent::SWITCH_PLACE
    ));
    out
}

/// One app's lines under its label, indented as a list item.
fn block(out: &mut String, label: &str, lines: &[(&'static str, String)]) {
    out.push_str(&format!("   - {label}:\n"));
    lines_into(out, lines);
}

fn lines_into(out: &mut String, lines: &[(&'static str, String)]) {
    for (shell, line) in lines {
        if shell.is_empty() {
            out.push_str(&format!("       {line}\n"));
        } else {
            out.push_str(&format!("       {shell}: {line}\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Global, Output, SetupLines, connect, dispatch, flags};
    use super::compose;

    const WINDOWS_EXE: &str = r"C:\Program Files\Quill\quill.exe";
    const WINDOWS_CLI: &str = r"C:\Program Files\Quill\quill-cli.exe";

    fn windows_lines() -> SetupLines {
        SetupLines {
            claude_code: connect::claude_code_commands(WINDOWS_EXE, true),
            claude_desktop: connect::command_lines(
                WINDOWS_CLI,
                true,
                "agent connect claude-desktop --yes",
            ),
            cursor: connect::command_lines(WINDOWS_CLI, true, "agent connect cursor --yes"),
            mcp: connect::shell_lines(WINDOWS_EXE, true, |quoted| format!("{quoted} mcp")),
        }
    }

    /// The words after `cli` on each prompt line that runs it.
    fn commands(prompt: &str, cli: &str) -> Vec<Vec<String>> {
        prompt
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix(cli))
            .map(|rest| rest.split_whitespace().map(str::to_string).collect())
            .collect()
    }

    #[test]
    fn the_prompt_holds_every_setup_line() {
        for (lines, cli, windows) in [
            (super::super::setup_lines(), "quill".to_string(), false),
            (windows_lines(), WINDOWS_CLI.to_string(), true),
        ] {
            let prompt = compose(&lines, &cli, windows);
            let every = [
                &lines.claude_code,
                &lines.claude_desktop,
                &lines.cursor,
                &lines.mcp,
            ];
            for (shell, line) in every.into_iter().flatten() {
                assert!(prompt.contains(line.as_str()), "{line} missing:\n{prompt}");
                assert!(prompt.contains(shell), "{shell} missing:\n{prompt}");
            }
            for name in ["quill_status", "request_access"] {
                assert!(prompt.contains(name), "{name} missing:\n{prompt}");
                assert!(flags::find_tool(name).is_some(), "{name} is not a tool");
            }
            assert!(prompt.contains(crate::agent::SWITCH_PLACE));
        }
    }

    #[test]
    fn the_windows_prompt_calls_the_quoted_path() {
        let prompt = compose(&windows_lines(), WINDOWS_CLI, true);
        assert!(
            prompt.contains(&format!("PowerShell: & \"{WINDOWS_CLI}\" agent status")),
            "{prompt}"
        );
        assert!(
            prompt.contains(&format!("Command Prompt: \"{WINDOWS_CLI}\" agent status")),
            "{prompt}"
        );
    }

    #[test]
    fn every_command_in_the_prompt_runs() {
        let cli = "/Applications/Quill.app/Contents/MacOS/quill";
        let prompt = compose(&super::super::setup_lines(), cli, false);
        let commands = commands(&prompt, cli);
        let named = commands
            .iter()
            .filter_map(|words| words.get(1).cloned())
            .collect::<Vec<_>>();
        for expected in ["--help", "status", "guide", "install-skill"] {
            assert!(
                named.iter().any(|w| w == expected),
                "no agent {expected}:\n{prompt}"
            );
        }
        for words in &commands {
            assert_eq!(
                words.first().map(String::as_str),
                Some("agent"),
                "{words:?}"
            );
            let rest = words
                .split_first()
                .map(|(_, rest)| rest)
                .unwrap_or_default();
            match rest.first().map(String::as_str) {
                // Running these would touch Quill or the disk; they only
                // have to parse to a real command.
                Some("status") => assert!(flags::find_tool("status").is_some()),
                Some("install-skill") => assert!(flags::builtin_help("install-skill").is_some()),
                _ => {
                    let global = Global {
                        help: rest.iter().any(|w| w == "--help"),
                        ..Global::default()
                    };
                    let args = rest
                        .iter()
                        .filter(|w| *w != "--help")
                        .cloned()
                        .collect::<Vec<_>>();
                    let output = dispatch(&global, &args);
                    assert!(
                        matches!(output, Ok(Output::Text(ref text)) if !text.is_empty()),
                        "{words:?}"
                    );
                }
            }
        }
    }
}
