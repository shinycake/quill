use super::*;
use crate::agent::activity::{self, Action};
use crate::agent::grants::{self, Scope};
use crate::ids::ChatId;
use std::collections::BTreeSet;

/// The chat windows file is read, changed and queued whole by its one
/// writer (the UI thread); these tests take turns at it the same way.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn root() -> PathBuf {
    crate::settings::safe_app_root().unwrap_or_else(|| panic!("a test app root"))
}

fn action(account: &AccountKey) -> Action {
    Action {
        id: 0,
        time: 0,
        account: account.clone(),
        agent: "Test agent".into(),
        tool: "send_message".into(),
        chat_id: 5,
        chat_title: "Old chat".into(),
        summary: "hello".into(),
        ok: true,
        declined: false,
        message_ids: Vec::new(),
        undone: false,
    }
}

fn window(account: &str) -> crate::settings::SavedChatWindow {
    crate::settings::SavedChatWindow {
        account: account.into(),
        chat_id: 5,
        x: 0.,
        y: 0.,
        width: 800.,
        height: 600.,
    }
}

/// Everything one account left behind, for `key`.
fn fill(root: &Path, key: &AccountKey) {
    let account = key.0.as_str();
    grants::update(key, |grants| grants.all_chats = Some(Scope::ReadWrite));
    activity::record(action(key));
    crate::web_app::trust::mark_trusted(account, 77);
    crate::notify_volume::update(account, |volumes| volumes.set_chat(ChatId(5), 40));
    let mut windows = crate::settings::load_chat_windows();
    windows.set_windows_of(account, vec![window(account)]);
    crate::settings::save_chat_windows(&windows);
    let paths = AccountPaths::for_root(root, key);
    std::fs::create_dir_all(&paths.root).unwrap();
    let mut tags = crate::suggest::RecentHashtags::default();
    tags.record_message("#secret");
    crate::settings::save_recent_hashtags(&paths, &tags);
    std::fs::write(
        paths.root.join(crate::connect::PAID_TRUST_FILE),
        b"{\"5\":10}",
    )
    .unwrap();
    std::fs::write(paths.root.join("launch-lines.bin"), b"QLL1").unwrap();
    std::fs::create_dir_all(webview_dir(root, account).join("bot-77")).unwrap();
    std::fs::create_dir_all(widget_dir(root, account)).unwrap();
}

fn left_behind(root: &Path, key: &AccountKey) -> Vec<&'static str> {
    let account = key.0.as_str();
    let paths = AccountPaths::for_root(root, key);
    let mut left = Vec::new();
    if grants::for_account(key).all_chats.is_some() {
        left.push("agent grants");
    }
    if !activity::recent(key).is_empty() {
        left.push("agent log");
    }
    if crate::web_app::trust::is_trusted(account, 77) {
        left.push("mini app trust");
    }
    if !crate::notify_volume::account(account).chats.is_empty() {
        left.push("notification volumes");
    }
    if !crate::settings::load_chat_windows()
        .windows_of(account)
        .is_empty()
    {
        left.push("chat windows");
    }
    if !crate::settings::load_recent_hashtags(&paths)
        .tags
        .is_empty()
    {
        left.push("recent hashtags");
    }
    if paths.root.join(crate::connect::PAID_TRUST_FILE).exists() {
        left.push("paid trust");
    }
    if paths.root.join("launch-lines.bin").exists() {
        left.push("launch lines");
    }
    if webview_dir(root, account).exists() {
        left.push("mini app storage");
    }
    if widget_dir(root, account).exists() {
        left.push("widget storage");
    }
    left
}

/// A log-out or removal drops every store of that account, and no other
/// account's.
#[test]
fn forgetting_an_account_drops_all_it_left_and_nothing_else() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let gone = AccountKey::new("account-931").unwrap();
    let kept = AccountKey::new("account-932").unwrap();
    fill(&root, &gone);
    fill(&root, &kept);
    assert_eq!(left_behind(&root, &gone).len(), 10, "the test filled all");
    forget(&root, &gone);
    crate::settings::flush_deferred_writes();
    assert_eq!(left_behind(&root, &gone), Vec::<&str>::new());
    assert_eq!(
        left_behind(&root, &kept).len(),
        10,
        "another account's stay"
    );
    forget(&root, &kept);
}

/// A different Telegram user in a slot drops what the last one left, as
/// when the app quit before a log-out finished; the same user keeps it.
#[test]
fn a_new_user_in_a_reused_slot_inherits_nothing() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let key = AccountKey::new("account-933").unwrap();
    assert!(!claim(&root, &key, 100), "first sign-in: nothing to drop");
    fill(&root, &key);
    assert!(!claim(&root, &key, 100), "the same user again keeps it");
    assert_eq!(left_behind(&root, &key).len(), 10);
    assert!(claim(&root, &key, 200), "another user");
    crate::settings::flush_deferred_writes();
    assert_eq!(left_behind(&root, &key), Vec::<&str>::new());
    assert!(!claim(&root, &key, 200));
}

/// A prompt an agent filed for an account that then logged out is
/// answered Denied, so a later Allow can't reach the next user.
#[test]
fn a_waiting_prompt_of_a_forgotten_account_is_denied() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let _guard = crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK);
    // The switch stays in memory: `quill agent` tests read it from disk.
    crate::fast_buttons::set_persistence(false);
    crate::agent::access::set_enabled(true);
    let key = AccountKey::new("account-934").unwrap();
    let (_, rx) = crate::agent::prompts::file(
        key.clone(),
        "Test agent".into(),
        crate::agent::prompts::Ask::Contacts,
        "why".into(),
    )
    .unwrap_or_else(|| panic!("filed"));
    forget(&root(), &key);
    assert_eq!(
        rx.try_recv().ok(),
        Some(crate::agent::prompts::Decision::Denied)
    );
    assert!(crate::agent::prompts::current_for(&key).is_none());
    crate::agent::access::set_enabled(false);
}

/// What a log-out drops ([`forget`]). Each is checked by the tests above.
const DROPPED: &[&str] = &[
    "agent_grants.json",
    "agent_actions.json",
    "web_app_trust.json",
    "notify_volumes.json",
    "chat_windows.json",
    "recent_hashtags.json",
    "paid_trust.json",
    "launch-lines.bin",
];

/// Every other file name production code reads or writes, and why a
/// log-out leaves it.
const KEPT: &[(&str, &str)] = &[
    (
        "owner.json",
        "the claim itself: who the slot was last signed in as",
    ),
    ("accounts.json", "the account list; removal edits it"),
    (
        "passcode.json",
        "app-wide lock; the lock-screen wipe deletes it",
    ),
    (
        "db-encryption.key",
        "the slot's database key; removal deletes it with the folder and keychain entry",
    ),
    (
        "db-encryption.wrapped",
        "the same key wrapped by the passcode; goes with the folder",
    ),
    (
        "prefs.json",
        "device preference (notification view, sounds, keys)",
    ),
    (
        "call_prefs.json",
        "device preference (call devices, push-to-talk)",
    ),
    ("media_prefs.json", "device preference (playback)"),
    (
        "data_storage.json",
        "device preference (auto-download, storage limits)",
    ),
    ("badge_prefs.json", "device preference (badge counting)"),
    (
        "contacts_prefs.json",
        "device preference (contact sync switch)",
    ),
    (
        "language_prefs.json",
        "device preference (TDLib language tag)",
    ),
    ("device_prefs.json", "device preference (device name)"),
    ("proxy_prefs.json", "app-wide proxy settings"),
    ("ton_wallet_prefs.json", "display currency only"),
    ("appearance_prefs.json", "app-wide"),
    ("chat_prefs.json", "app-wide"),
    ("spellcheck_languages.json", "app-wide"),
    ("spellcheck_words.json", "app-wide dictionary"),
    ("translate_prefs.json", "app-wide"),
    ("settings_search_recent.json", "app-wide"),
    ("agent_access.json", "app-wide agent switch"),
    ("ai_compose_prefs.json", "app-wide"),
    ("experimental_options.json", "app-wide"),
    ("fast_buttons.json", "app-wide"),
    ("file_prefs.json", "app-wide"),
    ("first_run.json", "app-wide"),
    ("gift_market.json", "app-wide filters"),
    ("hardware_video.json", "app-wide"),
    ("lang_cache.json", "app-wide language pack cache"),
    ("language.json", "app-wide interface language"),
    ("language_recent.json", "app-wide"),
    ("notify_prefs.json", "app-wide"),
    ("privacy_hints.json", "app-wide"),
    (
        "psa_about_seen.json",
        "promoted chat ids, the same for everyone",
    ),
    ("video_pip.json", "app-wide"),
    ("window_state.json", "app-wide"),
    ("launch-glyphs.bin", "glyph shapes, no text"),
    ("last-update.json", "updater"),
    ("latest.json", "updater"),
    ("releases.json", "updater"),
    ("refused-release.json", "updater"),
    ("update-plan.json", "updater demo"),
    ("config.json", "an emoji pack's manifest"),
    (
        "result.json",
        "an export's output, in the folder the user picked",
    ),
    (".quill-skill.json", "the agent CLI's install marker"),
    ("mcp.json", "another app's agent config the CLI edits"),
    (
        "claude_desktop_config.json",
        "another app's agent config the CLI edits",
    ),
];

/// Store-like file names (`name.json`, `.jsonl`, `.bin`, `.key`,
/// `.wrapped`) in the string literals of `text`.
fn store_names(text: &str) -> Vec<String> {
    const SUFFIXES: &[&str] = &[".json", ".jsonl", ".bin", ".key", ".wrapped"];
    text.split('"')
        .skip(1)
        .step_by(2)
        .filter(|lit| {
            !lit.is_empty()
                && lit
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                && SUFFIXES.iter().any(|suffix| lit.ends_with(suffix))
        })
        .map(str::to_owned)
        .collect()
}

/// The text before a file's test module.
fn production_part(text: &str) -> &str {
    let mut at = 0;
    while let Some(found) = text.get(at..).and_then(|rest| rest.find("\n#[cfg(test)]")) {
        let start = at + found + 1;
        let after = text.get(start..).unwrap_or("");
        let next_item = after
            .lines()
            .skip(1)
            .find(|line| !line.trim_start().starts_with("#["))
            .unwrap_or("");
        if next_item.starts_with("mod ") || next_item.starts_with("pub mod ") {
            return text.get(..start).unwrap_or(text);
        }
        at = start + 1;
    }
    text
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "tests" {
                walk(&path, out);
            }
        } else if name.ends_with(".rs") && !name.contains("test") {
            out.push(path);
        }
    }
}

/// A new file a store keeps is either dropped by a log-out or listed in
/// `KEPT` with the reason it stays.
#[test]
fn every_store_is_classified() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    let known: BTreeSet<&str> = DROPPED
        .iter()
        .copied()
        .chain(KEPT.iter().map(|(name, _)| *name))
        .collect();
    let mut unclassified = BTreeSet::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for name in store_names(production_part(&text)) {
            if !known.contains(name.as_str()) {
                let at = file
                    .strip_prefix(&src)
                    .unwrap_or(&file)
                    .display()
                    .to_string();
                unclassified.insert(format!("{name} ({at})"));
            }
        }
    }
    assert!(
        unclassified.is_empty(),
        "classify these in account_data_tests.rs (DROPPED, and drop them in `forget`, or KEPT with a reason): {unclassified:?}"
    );
}
