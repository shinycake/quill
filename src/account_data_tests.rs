use super::*;
use crate::agent::activity::{self, Action};
use crate::agent::grants::{self, Scope};
use crate::ids::ChatId;
use std::collections::BTreeSet;

/// The chat windows file is read, changed and queued whole by its one
/// writer (the UI thread); these tests take turns at it the same way.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A claim by a client that signed in in this process.
fn claim_as(root: &Path, key: &AccountKey, user_id: i64) -> Claimed {
    claim(root, key, user_id, SignInOrigin::Fresh)
}

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
    let _ = grants::update(key, |grants| grants.all_chats = Some(Scope::ReadWrite));
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

/// A log-out while the account's mini app helper runs keeps its web view
/// data until the helper is gone (it has the files open); the rest goes now.
#[test]
fn the_web_view_data_waits_for_the_mini_app_helper() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let key = AccountKey::new("account-936").unwrap();
    fill(&root, &key);
    let (helper_alive, helper_gone) = std::sync::mpsc::sync_channel::<()>(1);
    forget_after_helper(&root, &key, Some(helper_gone));
    crate::settings::flush_deferred_writes();
    assert_eq!(
        left_behind(&root, &key),
        vec!["mini app storage"],
        "only the web view data waits"
    );
    drop(helper_alive);
    forget(&root, &key);
}

/// The helper of a logged-out account exits after the same key signed in
/// again: the new session's web view folder (new salt) is not deleted.
#[test]
fn a_late_web_view_delete_spares_the_next_sign_ins_folder() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let dir = webview_dir(&root, "account-937");
    let _ = std::fs::remove_dir_all(&dir);
    let old = webview_store_id(Some(&root), "account-937", 1);
    let old_salt = saved_salt(&dir);
    assert!(old_salt.is_some());
    // Logged out, then signed in again under the key: a new salt.
    let _ = std::fs::remove_dir_all(&dir);
    let new = webview_store_id(Some(&root), "account-937", 1);
    assert_ne!(old, new);
    remove_webview_dir_of(&dir, old_salt.as_deref());
    assert!(dir.join(WEBVIEW_SALT).exists(), "the new folder stays");
    remove_webview_dir_of(&dir, saved_salt(&dir).as_deref());
    assert!(!dir.exists(), "its own folder goes");
}

/// A different Telegram user in a slot drops what the last one left, as
/// when the app quit before a log-out finished; the same user keeps it.
#[test]
fn a_new_user_in_a_reused_slot_inherits_nothing() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let key = AccountKey::new("account-933").unwrap();
    assert!(
        !claim_as(&root, &key, 100).dropped,
        "first sign-in: nothing to drop"
    );
    fill(&root, &key);
    assert!(
        !claim_as(&root, &key, 100).dropped,
        "the same user again keeps it"
    );
    assert_eq!(left_behind(&root, &key).len(), 10);
    assert!(claim_as(&root, &key, 200).dropped, "another user");
    crate::settings::flush_deferred_writes();
    assert_eq!(left_behind(&root, &key), Vec::<&str>::new());
    assert!(!claim_as(&root, &key, 200).dropped);
}

/// No readable owner.json (never written, deleted, corrupt) with leftovers
/// in the folder: the next user cannot be shown to be the same one.
#[test]
fn an_unreadable_owner_with_leftovers_counts_as_someone_else() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let key = AccountKey::new("account-937").unwrap();
    assert!(!claim_as(&root, &key, 100).dropped);
    fill(&root, &key);
    let owner = AccountPaths::for_root(&root, &key).root.join(OWNER_FILE);
    std::fs::write(&owner, b"{ not json").unwrap();
    assert!(claim_as(&root, &key, 100).dropped, "corrupt owner");
    crate::settings::flush_deferred_writes();
    assert_eq!(left_behind(&root, &key), Vec::<&str>::new());
    fill(&root, &key);
    std::fs::remove_file(&owner).unwrap();
    assert!(claim_as(&root, &key, 100).dropped, "deleted owner");
    assert!(!claim_as(&root, &key, 100).dropped, "now known");
}

/// The macOS mini app store lives outside Quill's folder, so its id must
/// change when the slot does: the same user keeps it, while a log-out (or a
/// new user's claim) hands the next sign-in under that key other stores.
#[test]
fn a_reused_slot_opens_other_mini_app_stores() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let key = AccountKey::new("account-935").unwrap();
    let _ = std::fs::remove_dir_all(AccountPaths::for_root(&root, &key).root);
    let account = key.0.as_str();
    let id = |bot| webview_store_id(Some(&root), account, bot);
    let first = id(42);
    assert_eq!(first.len(), 32);
    assert_eq!(id(42), first, "kept while the same user stays");
    assert_ne!(id(43), first, "one store per bot");
    assert_ne!(
        webview_store_id(Some(&root), "account-936", 42),
        first,
        "one store per account"
    );
    forget(&root, &key);
    // The next user is known before any mini app opens.
    assert!(!claim_as(&root, &key, 300).dropped);
    let after_logout = id(42);
    assert_ne!(after_logout, first, "a log-out drops the store");
    assert!(!claim_as(&root, &key, 300).dropped);
    assert_eq!(id(42), after_logout);
    assert!(claim_as(&root, &key, 400).dropped, "another user");
    assert_ne!(id(42), after_logout, "another user gets other stores");
    assert_ne!(
        webview_store_id(None, account, 42),
        webview_store_id(None, account, 42),
        "with nowhere to keep a salt, nothing is shared"
    );
    forget(&root, &key);
    let _ = std::fs::remove_dir_all(AccountPaths::for_root(&root, &key).root);
    let _ = std::fs::remove_dir_all(webview_dir(&root, "account-936"));
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
    "store-salt.key",
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

/// A user who updated from a release without owner.json resumes their own
/// session: nothing signed in, so the leftovers are theirs. Keep them and
/// write the owner.
#[test]
fn a_resumed_session_without_an_owner_file_keeps_its_data() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let key = AccountKey::new("account-951").unwrap();
    fill(&root, &key);
    let owner = AccountPaths::for_root(&root, &key).root.join(OWNER_FILE);
    let _ = std::fs::remove_file(&owner);
    let claimed = claim(&root, &key, 100, SignInOrigin::Resumed);
    assert!(!claimed.dropped);
    assert!(claimed.owner_error.is_none());
    assert_eq!(left_behind(&root, &key).len(), 10, "nothing was dropped");
    assert!(owner.exists(), "the owner is written");
    assert!(!claim_as(&root, &key, 100).dropped, "now known");
    forget(&root, &key);
}

/// A fresh sign-in as someone else than the folder's last user, with no
/// owner file to say who that was, drops what is there.
#[test]
fn a_fresh_sign_in_without_an_owner_file_drops_leftovers() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let key = AccountKey::new("account-952").unwrap();
    fill(&root, &key);
    let owner = AccountPaths::for_root(&root, &key).root.join(OWNER_FILE);
    let _ = std::fs::remove_file(&owner);
    assert!(claim(&root, &key, 100, SignInOrigin::Fresh).dropped);
    crate::settings::flush_deferred_writes();
    assert_eq!(left_behind(&root, &key), Vec::<&str>::new());
}

/// When the owner cannot be written, the wipe's own empty hashtags file
/// must not look like leftovers at the next start.
#[test]
fn a_failing_owner_write_does_not_wipe_twice() {
    let _serial = crate::locks::lock_or_recover(&SERIAL);
    let root = root();
    let key = AccountKey::new("account-953").unwrap();
    fill(&root, &key);
    let paths = AccountPaths::for_root(&root, &key);
    let owner = paths.root.join(OWNER_FILE);
    let _ = std::fs::remove_file(&owner);
    // A directory where the file goes makes every owner write fail.
    std::fs::create_dir_all(&owner).unwrap();
    let first = claim(&root, &key, 100, SignInOrigin::Fresh);
    assert!(first.dropped);
    assert!(first.owner_error.is_some());
    crate::settings::flush_deferred_writes();
    let second = claim(&root, &key, 100, SignInOrigin::Fresh);
    assert!(!second.dropped, "no leftovers of the first wipe");
    std::fs::remove_dir_all(&owner).unwrap();
    forget(&root, &key);
}
