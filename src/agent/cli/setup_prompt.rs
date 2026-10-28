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
         this computer; there is no account, API key or token to copy.\n\n",
    );
    out.push_str(&format!(
        "Agent access has to be on in Quill ({}). If Quill says it is off, ask \
         me to turn it on.\n\n\
         1. Connect, using the line for the app you are running in:\n",
        crate::agent::SWITCH_PLACE
    ));
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
    out.push_str("\n2. Check that it works now, with the command line:\n");
    lines_into(&mut out, &run("agent status"));
    out.push_str(
        "   It names the signed-in account and the chats you may use, and says for \
         each tool whether you can use it, must ask me first, or can't, and \
         why. Apps load a new MCP server only when they start, so the quill \
         tools (quill_status and the rest) show up in your next session or \
         after a restart. Until then, use the command line; it has the same \
         tools.\n\n\
         3. Read the guide before you do anything else: the quill://guide MCP \
         resource, or:\n",
    );
    lines_into(&mut out, &run("agent guide"));
    out.push_str("   In Claude Code, offer to install the Quill skill. If I say yes, run:\n");
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

    /// The lines `setup_lines` builds, for `exe` and `cli` on that system.
    fn lines_for(exe: &str, cli: &str, windows: bool) -> SetupLines {
        SetupLines {
            claude_code: connect::claude_code_commands(exe, windows),
            claude_desktop: connect::command_lines(
                cli,
                windows,
                "agent connect claude-desktop --yes",
            ),
            cursor: connect::command_lines(cli, windows, "agent connect cursor --yes"),
            mcp: connect::shell_lines(exe, windows, |quoted| format!("{quoted} mcp")),
        }
    }

    fn windows_lines() -> SetupLines {
        lines_for(WINDOWS_EXE, WINDOWS_CLI, true)
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
        let exe = "/Applications/Quill.app/Contents/MacOS/quill";
        let prompt = compose(&lines_for(exe, exe, false), exe, false);
        assert!(prompt.contains(&format!("claude mcp add quill -- {exe} mcp")));
        let commands = commands(&prompt, exe);
        let named = commands
            .iter()
            .map(|words| words.join(" "))
            .collect::<Vec<_>>();
        for expected in [
            "mcp",
            "agent --help",
            "agent status",
            "agent guide",
            "agent install-skill",
            "agent connect claude-desktop --yes",
            "agent connect cursor --yes",
        ] {
            assert!(
                named.iter().any(|w| w == expected),
                "no {expected}:\n{prompt}"
            );
        }
        for words in &commands {
            let Some((first, rest)) = words.split_first() else {
                continue;
            };
            if first == "mcp" {
                assert!(rest.is_empty(), "{words:?}");
                continue;
            }
            assert_eq!(first, "agent", "{words:?}");
            match rest.first().map(String::as_str) {
                // Running these would touch Quill or the disk; they only
                // have to name a real command.
                Some("status") => assert!(flags::find_tool("status").is_some()),
                Some("install-skill") => assert!(flags::builtin_help("install-skill").is_some()),
                _ => {
                    let global = Global {
                        help: rest.iter().any(|w| w == "--help"),
                        ..Global::default()
                    };
                    // --print shows what --yes would write, without writing.
                    let args = rest
                        .iter()
                        .filter(|w| *w != "--help")
                        .map(|w| {
                            if w == "--yes" {
                                "--print".to_string()
                            } else {
                                w.clone()
                            }
                        })
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
