//! `quill agent` without a running Quill: the registry-derived commands,
//! flags, help and describe, install-skill and connect.

use super::flags::{self, ALIASES, RESERVED};
use super::{Exit, Global, Output, connect, describe, skill};
use serde_json::{Value, json};

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|w| (*w).to_string()).collect()
}

fn tool(name: &str) -> flags::ToolSpec {
    flags::find_tool(name).unwrap_or_else(|| panic!("no tool {name}"))
}

#[test]
fn every_tool_is_a_command_and_the_aliases_name_real_tools() {
    let catalog = crate::agent::tools::catalog();
    assert_eq!(flags::tools().len(), catalog.len());
    for entry in &catalog {
        let name = entry["name"].as_str().unwrap_or_default();
        assert_eq!(tool(name).name, name);
        assert_eq!(tool(&name.replace('_', "-")).name, name);
    }
    for (alias, name) in ALIASES {
        assert_eq!(tool(alias).name, *name, "{alias}");
    }
    assert!(flags::find_tool("send_everything").is_none());
}

#[test]
fn no_tool_argument_shadows_a_global_flag() {
    for tool in flags::tools() {
        let properties = tool.schema["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for reserved in RESERVED {
            assert!(
                !properties.contains_key(*reserved),
                "{} has {reserved}",
                tool.name
            );
        }
    }
}

#[test]
fn every_example_parses_and_round_trips_through_the_flags() {
    for tool in flags::tools() {
        let example = tool
            .example()
            .unwrap_or_else(|| panic!("{} has no example", tool.name));
        let args = flags::example_args(&tool).unwrap_or_default();
        let parsed = flags::parse_tool_args(&tool, &args)
            .unwrap_or_else(|f| panic!("{}: {}", tool.name, f.message));
        assert_eq!(parsed, example, "{}: {args:?}", tool.name);
        assert!(!tool.output().is_empty(), "{}", tool.name);
        assert!(flags::example_command(&tool).is_some_and(|c| c.starts_with("quill agent ")));
    }
}

#[test]
fn flags_are_typed_by_the_schema() {
    let read = tool("read");
    let parsed = flags::parse_tool_args(
        &read,
        &words(&[
            "--chat",
            "-1001234567890",
            "--limit=20",
            "--before-message-id",
            "42",
        ]),
    );
    assert_eq!(
        parsed.ok(),
        Some(json!({"chat": -1_001_234_567_890_i64, "limit": 20, "before_message_id": 42}))
    );
    let parsed = flags::parse_tool_args(&read, &words(&["--chat", "Ann Lee"]));
    assert_eq!(parsed.ok(), Some(json!({"chat": "Ann Lee"})));

    let chats = tool("chats");
    let parsed = flags::parse_tool_args(&chats, &words(&["--unread-only", "--no-archived"]));
    assert_eq!(
        parsed.ok(),
        Some(json!({"unread_only": true, "archived": false}))
    );

    // --json gives everything; a later flag overrides one field.
    let parsed = flags::parse_tool_args(
        &read,
        &words(&["--json", r#"{"chat": "saved", "limit": 5}"#, "--limit", "9"]),
    );
    assert_eq!(parsed.ok(), Some(json!({"chat": "saved", "limit": 9})));

    let bad = flags::parse_tool_args(&read, &words(&["--limit", "many"]));
    assert_eq!(bad.err().map(|f| f.exit), Some(Exit::Usage));
    let unknown = flags::parse_tool_args(&read, &words(&["--colour", "red"]))
        .err()
        .map(|f| f.message)
        .unwrap_or_default();
    assert!(unknown.contains("--chat"), "{unknown}");
    let missing = flags::parse_tool_args(&tool("search"), &[])
        .err()
        .map(|f| f.message)
        .unwrap_or_default();
    assert!(missing.contains("--query"), "{missing}");
}

#[test]
fn global_options_come_out_wherever_they_are() {
    let (global, rest) = flags::split_global(&words(&[
        "send", "--chat", "saved", "--json", "--start", "--text", "hi",
    ]))
    .unwrap_or_else(|(_, f)| panic!("{}", f.message));
    assert!(global.json && global.start);
    assert_eq!(rest, words(&["send", "--chat", "saved", "--text", "hi"]));

    let (global, rest) =
        flags::split_global(&words(&["call", "read_history", "--json", "{\"chat\": 1}"]))
            .unwrap_or_else(|(_, f)| panic!("{}", f.message));
    assert!(global.json, "--json '{{…}}' also asks for JSON output");
    assert_eq!(
        rest,
        words(&["call", "read_history", "--json", "{\"chat\": 1}"])
    );
    assert!(flags::split_global(&words(&["--images"])).is_err());
}

#[test]
fn help_comes_from_the_registry() {
    let help = flags::main_help();
    for tool in flags::tools() {
        assert!(help.contains(&tool.name), "{}", tool.name);
    }
    for exit in Exit::ALL {
        assert!(help.contains(exit.meaning()), "{exit:?}");
    }
    let send = flags::tool_help(&tool("send"));
    assert!(send.starts_with("quill agent send: "), "{send}");
    assert!(send.contains("--chat") && send.contains("--text"), "{send}");
    assert!(send.contains("Example:\n  quill agent send "), "{send}");
    assert!(
        send.contains("--chat <string|integer>\n      Required."),
        "{send}"
    );
    let read = flags::tool_help(&tool("read"));
    assert!(read.contains("1 to 99. Default 50."), "{read}");
    for topic in [
        "call",
        "describe",
        "guide",
        "install-skill",
        "connect",
        "setup-prompt",
    ] {
        assert!(flags::builtin_help(topic).is_some(), "{topic}");
    }
}

#[test]
fn describe_covers_every_tool_resource_and_exit_code() {
    let description = describe::describe();
    let tools = description["tools"].as_array().cloned().unwrap_or_default();
    assert_eq!(tools.len(), crate::agent::tools::catalog().len());
    for tool in &tools {
        assert!(tool["input_schema"].is_object(), "{tool}");
        assert!(tool["example"].is_object(), "{tool}");
        assert!(
            !tool["description"]
                .as_str()
                .unwrap_or_default()
                .contains("Example input")
        );
    }
    assert_eq!(
        description["resources"].as_array().map(Vec::len),
        Some(crate::agent::guide::resources().len())
    );
    assert_eq!(
        description["exit_codes"].as_array().map(Vec::len),
        Some(Exit::ALL.len())
    );
}

#[test]
fn tool_results_become_output_or_the_right_exit_code() {
    let ok = json!({"content": [{"type": "text", "text": "Done."}], "structuredContent": {"id": 7}, "isError": false});
    assert!(matches!(super::shape(&ok, false, &[]), Ok(Output::Text(t)) if t == "Done."));
    assert!(matches!(super::shape(&ok, true, &[]), Ok(Output::Json(v)) if v == json!({"id": 7})));

    let error = |text: &str| json!({"content": [{"type": "text", "text": text}], "isError": true});
    let exit = |text: &str| super::shape(&error(text), false, &[]).err().map(|f| f.exit);
    assert_eq!(exit("Chat not granted."), Some(Exit::ToolError));
    assert_eq!(
        exit(crate::agent::ui_driver::policy::DRIVER_OFF),
        Some(Exit::WindowControlOff)
    );
    assert_eq!(
        exit(crate::agent::server::OFF_MESSAGE),
        Some(Exit::AccessOff)
    );

    let failure = super::Failure::new(Exit::NotRunning, "x");
    assert_eq!(super::error_json(&failure)["error"]["exit_code"], 3);
    assert_eq!(super::error_json(&failure)["error"]["kind"], "not_running");
}

#[test]
fn the_skill_is_an_agent_skill_and_agrees_with_the_exit_codes() {
    let [(name, skill_md), (guide_name, guide)] = skill::FILES;
    assert_eq!((name, guide_name), ("SKILL.md", "guide.md"));
    assert!(
        skill_md.starts_with("---\nname: quill\ndescription: "),
        "frontmatter"
    );
    assert!(skill_md.contains("](guide.md)"));
    for exit in Exit::ALL {
        let row = format!("| {} | {} |", exit.code(), exit.kind());
        assert!(skill_md.contains(&row), "{row}");
    }
    assert!(
        guide.contains("quill agent"),
        "the guide names the command line"
    );
    assert!(guide.contains("install-skill"));
}

#[test]
fn install_skill_is_idempotent_and_keeps_edits_unless_forced() {
    let dir = crate::scratch::process_dir("quill-skill-install").join("quill");
    let _ = std::fs::remove_dir_all(&dir);
    let report = skill::install(&dir, false).unwrap_or_else(|f| panic!("{}", f.message));
    assert_eq!(report.written, words(&["SKILL.md", "guide.md"]));
    let again = skill::install(&dir, false).unwrap_or_else(|f| panic!("{}", f.message));
    assert_eq!(again.unchanged.len(), 2);
    assert!(again.written.is_empty());

    // A copy Quill wrote earlier (its hash is in the manifest) is updated.
    let old = "---\nname: quill\ndescription: old\n---\n";
    let manifest = json!({"files": {"SKILL.md": sha_hex(old)}});
    std::fs::write(dir.join("SKILL.md"), old).unwrap_or_default();
    std::fs::write(dir.join(".quill-skill.json"), manifest.to_string()).unwrap_or_default();
    let updated = skill::install(&dir, false).unwrap_or_else(|f| panic!("{}", f.message));
    assert_eq!(updated.updated, words(&["SKILL.md"]));

    // The user's own edit is kept, with a diff, until --force.
    let edited = skill::FILES[0].1.replace("# Quill", "# Quill (my notes)");
    std::fs::write(dir.join("SKILL.md"), &edited).unwrap_or_default();
    let kept = skill::install(&dir, false)
        .err()
        .unwrap_or_else(|| panic!("kept"));
    assert_eq!(kept.exit, Exit::LocalFile);
    assert!(
        kept.message.contains("- # Quill (my notes)"),
        "{}",
        kept.message
    );
    assert!(kept.message.contains("+ # Quill"), "{}", kept.message);
    assert_eq!(
        std::fs::read_to_string(dir.join("SKILL.md")).ok(),
        Some(edited.clone())
    );
    let forced = skill::install(&dir, true).unwrap_or_else(|f| panic!("{}", f.message));
    assert_eq!(
        forced.replaced,
        vec![("SKILL.md".to_string(), "SKILL.md.bak".to_string())]
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("SKILL.md.bak")).ok(),
        Some(edited)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("SKILL.md"))
            .ok()
            .as_deref(),
        Some(skill::FILES[0].1)
    );
}

fn sha_hex(text: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn connect_merges_into_the_config_and_keeps_a_backup() {
    let dir = crate::scratch::process_dir("quill-agent-connect");
    let path = dir.join("Claude").join("claude_desktop_config.json");
    let _ = std::fs::remove_dir_all(dir.join("Claude"));
    let entry = connect::server_entry("/Applications/Quill.app/Contents/MacOS/quill");

    assert_eq!(
        connect::merge_into(&path, &entry).ok(),
        Some(connect::Merge::Written {
            backup: None,
            replaced: false
        })
    );
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
        .unwrap_or_default();
    assert_eq!(written["mcpServers"]["quill"], entry);
    assert_eq!(
        connect::merge_into(&path, &entry).ok(),
        Some(connect::Merge::AlreadySet)
    );

    let theirs = json!({"theme": "dark", "mcpServers": {"other": {"command": "x"}, "quill": {"command": "old"}}});
    std::fs::write(&path, theirs.to_string()).unwrap_or_default();
    let merged = connect::merge_into(&path, &entry).unwrap_or_else(|f| panic!("{}", f.message));
    let connect::Merge::Written {
        backup: Some(backup),
        replaced: true,
    } = merged
    else {
        panic!("backed up and replaced: {merged:?}");
    };
    let backed_up: Value =
        serde_json::from_str(&std::fs::read_to_string(&backup).unwrap_or_default())
            .unwrap_or_default();
    assert_eq!(backed_up, theirs);
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
        .unwrap_or_default();
    assert_eq!(written["theme"], "dark");
    assert_eq!(written["mcpServers"]["other"], json!({"command": "x"}));
    assert_eq!(written["mcpServers"]["quill"], entry);

    std::fs::write(&path, "{not json").unwrap_or_default();
    let refused = connect::merge_into(&path, &entry).err().map(|f| f.exit);
    assert_eq!(refused, Some(Exit::LocalFile));
    assert_eq!(
        std::fs::read_to_string(&path).ok().as_deref(),
        Some("{not json")
    );
    std::fs::write(&path, r#"{"mcpServers": []}"#).unwrap_or_default();
    assert!(connect::merge_into(&path, &entry).is_err());
}

#[test]
fn connect_prints_the_setup_with_this_executable() {
    let line = connect::claude_code_commands("/Applications/Quill.app/Contents/MacOS/quill", false);
    assert_eq!(
        line,
        vec![(
            "",
            "claude mcp add quill -- /Applications/Quill.app/Contents/MacOS/quill mcp".to_string()
        )]
    );
    let global = Global::default();
    let out = connect::run(&global, &words(&["cursor", "--print"]));
    let Ok(Output::Text(text)) = out else {
        panic!("printed");
    };
    let snippet: Value = serde_json::from_str(&text).unwrap_or_default();
    assert_eq!(snippet["mcpServers"]["quill"]["args"], json!(["mcp"]));
    assert!(connect::run(&global, &words(&["vim"])).is_err());
    for app in [connect::App::ClaudeDesktop, connect::App::Cursor] {
        assert!(app.config_file().is_some(), "{app:?}");
    }
}
