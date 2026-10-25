//! Hidden developer command-line tools that talk to Telegram without the
//! UI: `--connect-smoke` and `--passkey-probe [login]`, plus `mcp`, the
//! agent bridge (`crate::agent::mcp`). `main` asks here
//! once, so a new tool does not need to touch `main.rs`.

/// The exit code of the developer tool `args` asks for, or `None` when
/// they ask for none.
#[must_use]
pub fn exit_code(args: &[String]) -> Option<i32> {
    if args.get(1).is_some_and(|a| a == "--passkey-probe") {
        return Some(crate::passkey_probe::cli_exit_code(
            args.get(2).is_some_and(|a| a == "login"),
        ));
    }
    if args.get(1).is_some_and(|a| a == "--embedded-credentials") {
        // Release pipeline check; prints only whether a pair is embedded.
        let embedded = crate::credentials::has_embedded();
        println!("{}", if embedded { "embedded" } else { "none" });
        return Some(0);
    }
    if args.get(1).is_some_and(|a| a == "mcp") {
        return Some(crate::agent::mcp::run());
    }
    if args.iter().skip(1).any(|a| a == "--connect-smoke") {
        return Some(crate::connect_smoke::cli_exit_code());
    }
    None
}
