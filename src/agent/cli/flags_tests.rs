//! Global options only in flag position, `--no-`, `--json=`, repeated array
//! flags after `--args`, and failures that honour `--json`.

use super::flags;
use super::{Exit, Output};
use serde_json::json;

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|w| (*w).to_string()).collect()
}

fn tool(name: &str) -> flags::ToolSpec {
    flags::find_tool(name).unwrap_or_else(|| panic!("no tool {name}"))
}

fn split(list: &[&str]) -> (super::Global, Vec<String>) {
    flags::split_global(&words(list)).unwrap_or_else(|(_, f)| panic!("{}", f.message))
}

#[test]
fn a_flag_value_is_never_taken_for_a_global_option() {
    // `-h`, `--start` and `--json` here are the text to send.
    let (global, rest) = split(&["send", "--chat", "saved", "--text", "-h"]);
    assert!(!global.help);
    assert_eq!(rest, words(&["send", "--chat", "saved", "--text", "-h"]));
    let (global, rest) = split(&["send", "--text", "--start", "--chat", "saved"]);
    assert!(!global.start);
    assert_eq!(
        rest,
        words(&["send", "--text", "--start", "--chat", "saved"])
    );
    let (global, rest) = split(&["send", "--text", "--json", "--chat", "saved", "--json"]);
    assert!(global.json, "the last --json is in flag position");
    assert_eq!(
        rest,
        words(&["send", "--text", "--json", "--chat", "saved"])
    );
    // Through `call`, and with `--text=` inline (the next word is a flag again).
    let (global, rest) = split(&["call", "send_message", "--text", "--help"]);
    assert!(!global.help);
    assert_eq!(rest, words(&["call", "send_message", "--text", "--help"]));
    let (global, _) = split(&["send", "--text=hi", "--help"]);
    assert!(global.help);
    // A boolean takes no value, so the word after it is in flag position.
    let (global, _) = split(&["chats", "--archived", "--json"]);
    assert!(global.json);
    // install-skill's --dir takes a value.
    let (global, rest) = split(&["install-skill", "--dir", "-h"]);
    assert!(!global.help);
    assert_eq!(rest, words(&["install-skill", "--dir", "-h"]));
    // Global options before the command, inline values too.
    let (global, rest) = split(&["--client=Script", "--images", "shots", "status"]);
    assert_eq!(global.client.as_deref(), Some("Script"));
    assert_eq!(global.images.as_deref(), Some("shots"));
    assert_eq!(rest, words(&["status"]));
}

#[test]
fn json_with_equals_works_like_json_with_a_space() {
    let read = tool("read");
    for form in [
        words(&["read", "--json", r#"{"chat": "saved"}"#]),
        words(&["read", r#"--json={"chat": "saved"}"#]),
    ] {
        let (global, rest) =
            flags::split_global(&form).unwrap_or_else(|(_, f)| panic!("{}", f.message));
        assert!(global.json, "{form:?}");
        let args = rest.get(1..).unwrap_or_default();
        let parsed = flags::parse_tool_args(&read, args).map_err(|f| f.message);
        assert_eq!(parsed, Ok(json!({"chat": "saved"})), "{form:?}");
    }
}

#[test]
fn no_only_negates_an_on_off_flag() {
    let chats = tool("chats");
    let usage = |list: &[&str]| {
        flags::parse_tool_args(&chats, &words(list))
            .err()
            .map(|f| (f.exit, f.message))
    };
    assert_eq!(
        flags::parse_tool_args(&chats, &words(&["--no-archived"])).ok(),
        Some(json!({"archived": false}))
    );
    let (exit, message) = usage(&["--no-archived=true"]).unwrap_or((Exit::Ok, String::new()));
    assert_eq!(exit, Exit::Usage);
    assert!(message.contains("takes no value"), "{message}");
    let (exit, message) = usage(&["--no-limit", "5"]).unwrap_or((Exit::Ok, String::new()));
    assert_eq!(exit, Exit::Usage);
    assert!(message.contains("on/off"), "{message}");
    // `--archived=false` is the way to give a boolean a value.
    assert_eq!(
        flags::parse_tool_args(&chats, &words(&["--archived=false"])).ok(),
        Some(json!({"archived": false}))
    );
}

#[test]
fn an_array_flag_after_args_replaces_the_field_and_repeats_add() {
    let poll = tool("send_poll");
    let parse = |list: &[&str]| flags::parse_tool_args(&poll, &words(list)).map_err(|f| f.message);
    let base = r#"{"chat": "saved", "question": "Lunch?", "options": ["Pizza", "Soup"]}"#;
    assert_eq!(
        parse(&["--args", base, "--options", "Salad", "--options", "Tacos"]),
        Ok(json!({"chat": "saved", "question": "Lunch?", "options": ["Salad", "Tacos"]}))
    );
    // A later --args replaces what flags set, and a flag after it starts over.
    assert_eq!(
        parse(&[
            "--chat",
            "saved",
            "--question",
            "Q",
            "--options",
            "A",
            "--options",
            "B",
            "--args",
            r#"{"options": ["C", "D"]}"#,
            "--options",
            "E",
            "--options",
            "F",
        ]),
        Ok(json!({"chat": "saved", "question": "Q", "options": ["E", "F"]}))
    );
}

#[test]
fn a_global_option_error_still_answers_in_json() {
    let failure = flags::split_global(&words(&["status", "--json", "--client"]));
    let Err((global, failure)) = failure else {
        panic!("--client without a value is an error");
    };
    assert!(global.json);
    assert_eq!(failure.exit, Exit::Usage);
    assert_eq!(
        super::run(&words(&["--json", "--images"])),
        Exit::Usage.code()
    );
}

#[test]
fn help_after_call_tool_is_that_tools_help() {
    let (global, rest) = split(&["call", "read_history", "--help"]);
    let help = super::dispatch(&global, &rest);
    let Ok(Output::Text(text)) = help else {
        panic!("help text");
    };
    assert!(text.starts_with("quill agent read: "), "{text}");
    let Ok(Output::Text(text)) = super::dispatch(&global, &words(&["call"])) else {
        panic!("help text");
    };
    assert!(text.starts_with("quill agent call "), "{text}");
}

#[test]
fn no_images_folder_is_made_without_images() {
    let dir = crate::scratch::process_dir("quill-agent-no-images").join("shots");
    let _ = std::fs::remove_dir_all(&dir);
    let global = super::Global {
        images: Some(dir.display().to_string()),
        ..super::Global::default()
    };
    let text_only = json!({"content": [{"type": "text", "text": "Done."}]});
    assert_eq!(
        super::save_images(&global, &text_only).ok(),
        Some(Vec::new())
    );
    assert!(!dir.exists());
    let png =
        json!({"content": [{"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"}]});
    let saved = super::save_images(&global, &png).unwrap_or_default();
    assert_eq!(saved.len(), 1);
    assert!(dir.is_dir());
}
