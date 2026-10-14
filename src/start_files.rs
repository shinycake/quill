//! Files handed to Quill from outside the window, to be sent to a chat of
//! the user's choosing: the Windows "Send to" menu entry, a file dropped on
//! the executable or the Dock icon, `quill <file>` on any platform.
//!
//! tdesktop's `Core::Launcher` treats every argument that is not a known
//! flag (and everything after `--`) as a "start URL"; `Application::
//! checkStartUrls` opens `tg:` links and collects the local files, and
//! `MainWidget::handleStartFiles` shows the "Forward to…" recipient box
//! (`ShowChooseRecipientBox`) before the files reach the send-files dialog.
//! This module is the argument half: it splits argv into links and files,
//! carries files over the single-instance socket, and queues them for the UI
//! poll loop, the way `deep_link_inbox` does for links.
//!
//! Files are untrusted input (any local process can launch Quill with any
//! path), so nothing is sent from here: the user still picks the chat and
//! confirms in the composer.

use crate::locks::LockExt;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Longest argument honored (tdesktop cuts each argument at 8192).
const MAX_ARG_BYTES: usize = 8192;
/// Files kept waiting for the UI.
const MAX_QUEUED: usize = 256;
/// Prefix of a file path forwarded to the running instance, so the owner
/// can tell it from a link or a flag.
const FORWARD_PREFIX: &str = "--send-file=";
/// Asks a running Quill to quit (tdesktop `-quit`, which its Windows jump
/// list and Linux desktop action pass). `-quit` is accepted too.
pub const QUIT_FLAG: &str = "--quit";
/// GPUI's Windows jump list launches `quill --dock-action <index>`; Quill's
/// only task there, "Quit Quill", is index 0.
const DOCK_ACTION_FLAG: &str = "--dock-action";
const DOCK_ACTION_QUIT: &str = "0";

/// Whether the launch arguments ask the running Quill to quit: `--quit`,
/// tdesktop's `-quit`, or the jump list's "Quit Quill" task.
pub fn quit_requested(args: &[String]) -> bool {
    let mut flags = args.iter().take_while(|arg| arg.as_str() != "--");
    let mut wanted = false;
    while let Some(arg) = flags.next() {
        match arg.as_str() {
            QUIT_FLAG | "-quit" => wanted = true,
            DOCK_ACTION_FLAG => {
                wanted |= flags.next().map(String::as_str) == Some(DOCK_ACTION_QUIT)
            }
            _ => {}
        }
    }
    wanted
}

/// What the launch arguments ask for.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StartArgs {
    /// `tg:` / `t.me` links, sanitized, in order.
    pub links: Vec<String>,
    /// Existing local files and folders, absolute, in order.
    pub files: Vec<PathBuf>,
}

/// Splits launch arguments (without the program name) into links and files.
///
/// Flags (`--start-minimized`, `--connect-smoke`, ...) are skipped until a
/// bare `--`; after it everything is positional, which is how the Windows
/// "Send to" shortcut passes file names that may start with a dash.
/// Relative paths resolve against `cwd`. Names that do not exist are dropped.
pub fn parse_start_args(args: &[String], cwd: &Path) -> StartArgs {
    let mut parsed = StartArgs::default();
    let mut positional = false;
    let mut skip_value = false;
    for arg in args {
        if std::mem::take(&mut skip_value) {
            continue;
        }
        if !positional && arg == "--" {
            positional = true;
            continue;
        }
        if !positional && arg.starts_with('-') {
            // The jump list's index is the flag's value, not a file name.
            skip_value = arg == DOCK_ACTION_FLAG;
            continue;
        }
        if arg.len() > MAX_ARG_BYTES {
            continue;
        }
        if let Some(link) = crate::deep_link_inbox::sanitize_link(arg) {
            parsed.links.push(link);
        } else if let Some(file) = resolve_file(arg, cwd)
            && !parsed.files.contains(&file)
        {
            parsed.files.push(file);
        }
    }
    parsed
}

/// A launch argument as an existing absolute path: a plain path (relative
/// to `cwd`) or a `file://` URL (what macOS and file managers hand over).
fn resolve_file(arg: &str, cwd: &Path) -> Option<PathBuf> {
    let arg = arg.trim();
    if arg.is_empty() || arg.chars().any(char::is_control) {
        return None;
    }
    let path = match arg.get(..7).filter(|p| p.eq_ignore_ascii_case("file://")) {
        Some(_) => file_url_path(arg.get(7..).unwrap_or_default())?,
        None => PathBuf::from(arg),
    };
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    path.exists().then_some(path)
}

/// The path of a `file://` URL body (`/home/a%20b`, `localhost/C:/x`).
fn file_url_path(rest: &str) -> Option<PathBuf> {
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(rest)?;
    // `/C:/Users/a` names `C:\Users\a` on Windows.
    let bytes = decoded.as_bytes();
    let decoded = if cfg!(windows) && bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' {
        decoded.get(1..).unwrap_or_default().to_string()
    } else {
        decoded
    };
    if !cfg!(windows) && !decoded.starts_with('/') {
        return None;
    }
    Some(PathBuf::from(decoded))
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(input.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// What a second launch sends the running instance: its links and flags
/// (`--start-minimized`, `--quit`) as the instance already understands them,
/// plus one `--send-file=<absolute path>` per file.
pub fn forwarded_args(args: &[String], cwd: &Path) -> Vec<String> {
    if quit_requested(args) {
        return vec![QUIT_FLAG.to_string()];
    }
    let parsed = parse_start_args(args, cwd);
    let start_minimized = args.iter().any(|arg| arg == "--start-minimized");
    parsed
        .links
        .into_iter()
        .chain(
            parsed
                .files
                .iter()
                .map(|file| format!("{FORWARD_PREFIX}{}", file.display())),
        )
        .chain(start_minimized.then(|| "--start-minimized".to_string()))
        .collect()
}

/// The files in a forwarded argument list and the arguments that are left
/// for the link handler. Each file is checked again: the sender is another
/// process, and the path must still be there.
fn split_forwarded(args: &[String]) -> (Vec<PathBuf>, Vec<String>) {
    let mut files = Vec::new();
    let mut rest = Vec::new();
    for arg in args {
        match arg.strip_prefix(FORWARD_PREFIX) {
            Some(path) => {
                let path = PathBuf::from(path);
                if path.is_absolute() && path.exists() && !files.contains(&path) {
                    files.push(path);
                }
            }
            None => rest.push(arg.clone()),
        }
    }
    (files, rest)
}

static INBOX: Mutex<VecDeque<PathBuf>> = Mutex::new(VecDeque::new());

fn inbox() -> std::sync::MutexGuard<'static, VecDeque<PathBuf>> {
    INBOX.lock_or_recover()
}

/// Queues files for the UI and asks for the window to be raised.
pub fn push_files(files: Vec<PathBuf>) {
    if files.is_empty() {
        return;
    }
    let mut queue = inbox();
    for file in files {
        if queue.len() < MAX_QUEUED && !queue.contains(&file) {
            queue.push_back(file);
        }
    }
    drop(queue);
    crate::deep_link_inbox::request_activation();
}

/// Queues the files named by this process's own launch arguments.
pub fn push_launch_files(args: &[String]) {
    if let Ok(cwd) = std::env::current_dir() {
        push_files(parse_start_args(args.get(1..).unwrap_or_default(), &cwd).files);
    }
}

/// Queues the local file a `file://` URL names (what the OS URL-open
/// callback delivers for a file opened with Quill). Returns whether it did.
pub fn push_file_url(url: &str) -> bool {
    let Ok(cwd) = std::env::current_dir() else {
        return false;
    };
    let is_file_url = url
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file://"));
    match resolve_file(url, &cwd).filter(|_| is_file_url) {
        Some(file) => {
            push_files(vec![file]);
            true
        }
        None => false,
    }
}

/// Handles the arguments of a later launch (on the single-instance listener
/// thread): queue its files, then let the link handler have the rest.
pub fn handle_forwarded_launch(args: &[String]) {
    let (files, rest) = split_forwarded(args);
    let has_files = !files.is_empty();
    push_files(files);
    // Files already raise the window; a bare relaunch still must.
    if !has_files || !rest.is_empty() {
        crate::deep_link_inbox::handle_forwarded_launch(&rest);
    }
}

/// Everything queued so far, oldest first.
pub fn take_files() -> Vec<PathBuf> {
    inbox().drain(..).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        FORWARD_PREFIX, MAX_ARG_BYTES, QUIT_FLAG, StartArgs, forwarded_args, parse_start_args,
        quit_requested, split_forwarded,
    };
    use std::path::PathBuf;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-start-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn splits_links_files_and_flags() {
        let dir = scratch("split");
        let file = dir.join("a b.txt");
        std::fs::write(&file, b"x").unwrap();
        let parsed = parse_start_args(
            &args(&[
                "--start-minimized",
                "tg://resolve?domain=durov",
                file.to_str().unwrap(),
                "https://example.com/not-ours",
                "/definitely/not/here",
            ]),
            &dir,
        );
        assert_eq!(parsed.links, vec!["tg://resolve?domain=durov"]);
        assert_eq!(parsed.files, vec![file]);
    }

    #[test]
    fn everything_after_a_double_dash_is_positional() {
        let dir = scratch("dashes");
        let odd = dir.join("-weird.txt");
        std::fs::write(&odd, b"x").unwrap();
        let before = parse_start_args(&args(&["-weird.txt"]), &dir);
        assert!(before.files.is_empty());
        let after = parse_start_args(&args(&["--", "-weird.txt", "--"]), &dir);
        assert_eq!(after.files, vec![odd]);
    }

    #[test]
    fn relative_paths_resolve_against_the_working_directory() {
        let dir = scratch("relative");
        std::fs::write(dir.join("r.png"), b"x").unwrap();
        let parsed = parse_start_args(&args(&["r.png", "missing.png"]), &dir);
        assert_eq!(parsed.files, vec![dir.join("r.png")]);
    }

    #[cfg(not(windows))]
    #[test]
    fn file_urls_are_decoded() {
        let dir = scratch("url");
        let file = dir.join("\u{e9} x.txt");
        std::fs::write(&file, b"x").unwrap();
        let url = format!("file://{}", file.to_str().unwrap().replace(' ', "%20"));
        let parsed = parse_start_args(&args(&[&url, "file://relative/x", "file:///%zz"]), &dir);
        assert_eq!(parsed.files, vec![file]);
    }

    #[test]
    fn long_and_control_arguments_are_refused() {
        let dir = scratch("refuse");
        let long = "a".repeat(MAX_ARG_BYTES + 1);
        let parsed = parse_start_args(&args(&[&long, "a\nb", " "]), &dir);
        assert_eq!(parsed, StartArgs::default());
    }

    #[test]
    fn forwarded_files_survive_the_round_trip_and_links_stay_links() {
        let dir = scratch("forward");
        let file = dir.join("f.txt");
        std::fs::write(&file, b"x").unwrap();
        let sent = forwarded_args(
            &args(&["--start-minimized", "tg:resolve?domain=a", "f.txt"]),
            &dir,
        );
        assert_eq!(
            sent,
            vec![
                "tg://resolve?domain=a".to_string(),
                format!("{FORWARD_PREFIX}{}", file.display()),
                "--start-minimized".to_string(),
            ]
        );
        let (files, rest) = split_forwarded(&sent);
        assert_eq!(files, vec![file]);
        assert_eq!(rest, vec!["tg://resolve?domain=a", "--start-minimized"]);
    }

    #[test]
    fn a_forwarded_file_that_vanished_or_is_relative_is_dropped() {
        let (files, rest) = split_forwarded(&args(&[
            "--send-file=relative.txt",
            "--send-file=/no/such/file",
            "--other",
        ]));
        assert!(files.is_empty());
        assert_eq!(rest, vec!["--other"]);
    }

    #[test]
    fn quit_flags_and_the_jump_list_task_ask_the_running_quill_to_quit() {
        assert!(quit_requested(&args(&["--quit"])));
        assert!(quit_requested(&args(&["-quit"])));
        assert!(quit_requested(&args(&["--dock-action", "0"])));
        assert!(!quit_requested(&args(&["--dock-action", "1"])));
        assert!(!quit_requested(&args(&["--dock-action"])));
        assert!(!quit_requested(&args(&["--", "--quit"])));
        assert!(!quit_requested(&args(&["tg://resolve?domain=a"])));
        let dir = scratch("quit");
        assert_eq!(
            forwarded_args(&args(&["--dock-action", "0"]), &dir),
            vec![QUIT_FLAG.to_string()]
        );
    }

    #[test]
    fn the_jump_list_index_is_never_read_as_a_file() {
        let dir = scratch("dock-index");
        std::fs::write(dir.join("0"), b"x").unwrap();
        let parsed = parse_start_args(&args(&["--dock-action", "0"]), &dir);
        assert!(parsed.files.is_empty());
    }
}
