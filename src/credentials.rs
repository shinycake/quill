//! Load Telegram `api_id` / `api_hash` from process env or gitignored local files.
//! Never log or Debug-print the hash value.

use crate::auth::credentials_ready;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Owner-supplied Telegram API credentials (never commit these).
#[derive(Clone, PartialEq, Eq)]
pub struct TelegramCredentials {
    pub api_id: i32,
    pub api_hash: String,
}

impl fmt::Debug for TelegramCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TelegramCredentials")
            .field("api_id", &self.api_id)
            .field("api_hash", &"<redacted>")
            .finish()
    }
}

/// Read credentials from process environment only (`TELEGRAM_*`, then `QUILL_*` aliases).
pub fn load_from_env() -> Option<TelegramCredentials> {
    let api_id = env_i32("TELEGRAM_API_ID").or_else(|| env_i32("QUILL_API_ID"));
    let api_hash = env_string("TELEGRAM_API_HASH").or_else(|| env_string("QUILL_API_HASH"));
    finish(api_id, api_hash)
}

/// Prefer process env; fill missing values from `.env` / `quill.local.env` under
/// `CARGO_MANIFEST_DIR` and the process current directory.
pub fn load() -> Option<TelegramCredentials> {
    let mut api_id = env_i32("TELEGRAM_API_ID").or_else(|| env_i32("QUILL_API_ID"));
    let mut api_hash = env_string("TELEGRAM_API_HASH").or_else(|| env_string("QUILL_API_HASH"));

    if api_id.is_none() || api_hash.is_none() {
        let file_map = load_file_map();
        if api_id.is_none() {
            api_id = map_i32(&file_map, "TELEGRAM_API_ID")
                .or_else(|| map_i32(&file_map, "QUILL_API_ID"));
        }
        if api_hash.is_none() {
            api_hash = map_string(&file_map, "TELEGRAM_API_HASH")
                .or_else(|| map_string(&file_map, "QUILL_API_HASH"));
        }
    }

    finish(api_id, api_hash)
}

fn finish(api_id: Option<i32>, api_hash: Option<String>) -> Option<TelegramCredentials> {
    let hash_ref = api_hash.as_deref();
    if !credentials_ready(api_id, hash_ref) {
        return None;
    }
    Some(TelegramCredentials {
        api_id: api_id.expect("validated"),
        api_hash: api_hash.expect("validated"),
    })
}

fn env_string(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn env_i32(key: &str) -> Option<i32> {
    env_string(key)?.parse().ok()
}

fn map_string(map: &HashMap<String, String>, key: &str) -> Option<String> {
    map.get(key)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn map_i32(map: &HashMap<String, String>, key: &str) -> Option<i32> {
    map_string(map, key)?.parse().ok()
}

fn candidate_env_files() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let names = [".env", "quill.local.env"];
    if let Ok(manifest) = env::var("CARGO_MANIFEST_DIR") {
        let base = PathBuf::from(manifest);
        for name in names {
            paths.push(base.join(name));
        }
    }
    // Compile-time crate root (same as CARGO_MANIFEST_DIR when tests run via cargo).
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in names {
        let p = crate_root.join(name);
        if !paths.iter().any(|existing| existing == &p) {
            paths.push(p);
        }
    }
    if let Ok(cwd) = env::current_dir() {
        for name in names {
            let p = cwd.join(name);
            if !paths.iter().any(|existing| existing == &p) {
                paths.push(p);
            }
        }
    }
    paths
}

fn load_file_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for path in candidate_env_files() {
        if let Ok(text) = fs::read_to_string(&path) {
            merge_dotenv(&mut map, &text);
        }
    }
    map
}

/// Parse simple `KEY=VALUE` lines. `#` starts a comment. No export/quoting dialect.
fn merge_dotenv(map: &mut HashMap<String, String>, text: &str) {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = strip_quotes(value.trim());
        map.insert(key.to_string(), value.to_string());
    }
}

fn strip_quotes(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return &value[1..value.len() - 1];
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        keys: Vec<&'static str>,
        previous: Vec<Option<String>>,
    }

    impl EnvGuard {
        fn capture(keys: &[&'static str]) -> Self {
            let previous = keys.iter().map(|k| env::var(k).ok()).collect();
            Self {
                keys: keys.to_vec(),
                previous,
            }
        }

        fn set(&self, key: &str, value: &str) {
            // SAFETY: tests hold ENV_LOCK; we restore in Drop.
            unsafe { env::set_var(key, value) };
        }

        fn remove(&self, key: &str) {
            unsafe { env::remove_var(key) };
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, prev) in self.keys.iter().zip(self.previous.iter()) {
                match prev {
                    Some(v) => unsafe { env::set_var(key, v) },
                    None => unsafe { env::remove_var(key) },
                }
            }
        }
    }

    const ENV_KEYS: &[&str] = &[
        "TELEGRAM_API_ID",
        "TELEGRAM_API_HASH",
        "QUILL_API_ID",
        "QUILL_API_HASH",
    ];

    #[test]
    fn load_from_env_telegram_names() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard::capture(ENV_KEYS);
        for k in ENV_KEYS {
            guard.remove(k);
        }
        guard.set("TELEGRAM_API_ID", "123456");
        guard.set("TELEGRAM_API_HASH", "deadbeefcafebabe");
        let creds = load_from_env().expect("credentials");
        assert_eq!(creds.api_id, 123456);
        assert_eq!(creds.api_hash, "deadbeefcafebabe");
        let debug = format!("{creds:?}");
        assert!(debug.contains("123456"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("deadbeef"));
    }

    #[test]
    fn load_from_env_quill_aliases_when_telegram_unset() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard::capture(ENV_KEYS);
        for k in ENV_KEYS {
            guard.remove(k);
        }
        guard.set("QUILL_API_ID", "42");
        guard.set("QUILL_API_HASH", "alias-hash-value");
        let creds = load_from_env().expect("alias credentials");
        assert_eq!(creds.api_id, 42);
        assert_eq!(creds.api_hash, "alias-hash-value");
    }

    #[test]
    fn telegram_names_win_over_quill_aliases() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard::capture(ENV_KEYS);
        for k in ENV_KEYS {
            guard.remove(k);
        }
        guard.set("TELEGRAM_API_ID", "99");
        guard.set("TELEGRAM_API_HASH", "telegram-hash");
        guard.set("QUILL_API_ID", "1");
        guard.set("QUILL_API_HASH", "quill-hash");
        let creds = load_from_env().expect("telegram wins");
        assert_eq!(creds.api_id, 99);
        assert_eq!(creds.api_hash, "telegram-hash");
    }

    #[test]
    fn sample_hash_rejected() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard::capture(ENV_KEYS);
        for k in ENV_KEYS {
            guard.remove(k);
        }
        guard.set("TELEGRAM_API_ID", "1");
        guard.set("TELEGRAM_API_HASH", "YOUR_API_HASH");
        assert!(load_from_env().is_none());
    }

    #[test]
    fn load_from_temp_dotenv_file() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard::capture(ENV_KEYS);
        for k in ENV_KEYS {
            guard.remove(k);
        }

        let dir = env::temp_dir().join(format!("quill-cred-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let env_path = dir.join(".env");
        fs::write(
            &env_path,
            "# test\nTELEGRAM_API_ID=777\nTELEGRAM_API_HASH=file-hash-ok\n",
        )
        .expect("write .env");

        let previous_cwd = env::current_dir().ok();
        env::set_current_dir(&dir).expect("chdir");
        let creds = load();
        if let Some(cwd) = previous_cwd {
            let _ = env::set_current_dir(cwd);
        }
        let _ = fs::remove_dir_all(&dir);

        let creds = creds.expect("file credentials");
        assert_eq!(creds.api_id, 777);
        assert_eq!(creds.api_hash, "file-hash-ok");
    }

    #[test]
    fn merge_dotenv_skips_comments_and_export() {
        let mut map = HashMap::new();
        merge_dotenv(
            &mut map,
            "# hi\nexport TELEGRAM_API_ID=5\nTELEGRAM_API_HASH=\"abc\"\n",
        );
        assert_eq!(map.get("TELEGRAM_API_ID").map(String::as_str), Some("5"));
        assert_eq!(
            map.get("TELEGRAM_API_HASH").map(String::as_str),
            Some("abc")
        );
    }
}
