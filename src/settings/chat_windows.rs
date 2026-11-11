//! The chat windows open at the last run, so they can come back at the
//! next launch (Telegram Desktop `Window::SavedWindows`: the separate
//! windows are saved with their positions; the first time, the user is
//! asked whether to restore them, and the answer is kept).

use super::write_json_atomic;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "chat_windows.json";

/// One chat window: the account it belongs to, its chat and its geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedChatWindow {
    pub account: String,
    pub chat_id: i64,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SavedChatWindow {
    /// A usable entry: finite geometry no smaller than a chat window's
    /// minimum size.
    pub fn sanitized(self) -> Option<Self> {
        let finite = [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite());
        (finite && self.width >= 380. && self.height >= 400.).then_some(self)
    }
}

/// The answer to "restore the windows from last time?".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RestoreAnswer {
    /// The question was not asked yet.
    #[default]
    Unasked,
    Yes,
    No,
}

/// What the file holds: the answer to the restore question and the
/// windows of the last run. The file keeps the keys it always had,
/// `asked`, `restore` and `windows` (an answer only counts when it was
/// asked).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "serde_json::Value", into = "serde_json::Value")]
pub struct ChatWindowsState {
    pub answer: RestoreAnswer,
    pub windows: Vec<SavedChatWindow>,
}

impl From<serde_json::Value> for ChatWindowsState {
    fn from(file: serde_json::Value) -> Self {
        let flag = |key: &str| file.get(key).and_then(serde_json::Value::as_bool);
        let answer = match (
            flag("asked").unwrap_or(false),
            flag("restore").unwrap_or(false),
        ) {
            (false, _) => RestoreAnswer::Unasked,
            (true, true) => RestoreAnswer::Yes,
            (true, false) => RestoreAnswer::No,
        };
        let windows = file
            .get("windows")
            .and_then(|windows| serde_json::from_value(windows.clone()).ok())
            .unwrap_or_default();
        Self { answer, windows }
    }
}

impl From<ChatWindowsState> for serde_json::Value {
    fn from(state: ChatWindowsState) -> Self {
        serde_json::json!({
            "asked": state.answer != RestoreAnswer::Unasked,
            "restore": state.answer == RestoreAnswer::Yes,
            "windows": state.windows,
        })
    }
}

impl ChatWindowsState {
    /// The windows of `account` worth offering or restoring.
    pub fn windows_of(&self, account: &str) -> Vec<SavedChatWindow> {
        self.windows
            .iter()
            .filter(|window| window.account == account)
            .cloned()
            .filter_map(|window| window.sanitized())
            .collect()
    }

    /// Replace `account`'s windows with the ones open now; other accounts'
    /// stay.
    pub fn set_windows_of(&mut self, account: &str, windows: Vec<SavedChatWindow>) {
        self.windows.retain(|window| window.account != account);
        self.windows.extend(
            windows
                .into_iter()
                .filter(|window| window.account == account),
        );
    }

    /// Record the answer to "restore the windows from last time?".
    pub fn answer(&mut self, restore: bool) {
        self.answer = if restore {
            RestoreAnswer::Yes
        } else {
            RestoreAnswer::No
        };
    }
}

fn path() -> Option<PathBuf> {
    super::safe_app_root().map(|root| root.join(FILE_NAME))
}

/// The saved state; a missing or corrupt file is an empty one.
pub fn load_chat_windows() -> ChatWindowsState {
    path()
        .map(|path| load_chat_windows_from(&path))
        .unwrap_or_default()
}

pub fn load_chat_windows_from(path: &Path) -> ChatWindowsState {
    super::read_with_pending(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Queue the state for a background write (the caller is on the UI thread);
/// a load sees it at once and quitting flushes it. With no app data
/// directory there is nowhere to save.
pub fn save_chat_windows(state: &ChatWindowsState) {
    if let Some(path) = path() {
        super::write_json_deferred(&path, state);
    }
}

/// `account` logged out or was removed: the windows it had open are not
/// offered to whoever signs in next under the key.
pub fn forget_chat_windows_of(account: &str) {
    let mut state = load_chat_windows();
    if !state.windows.iter().any(|window| window.account == account) {
        return;
    }
    state.windows.retain(|window| window.account != account);
    save_chat_windows(&state);
}

pub fn save_chat_windows_to(path: &Path, state: &ChatWindowsState) -> std::io::Result<()> {
    write_json_atomic(path, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(account: &str, chat_id: i64) -> SavedChatWindow {
        SavedChatWindow {
            account: account.into(),
            chat_id,
            x: 40.,
            y: 60.,
            width: 800.,
            height: 600.,
        }
    }

    #[test]
    fn windows_are_kept_per_account_and_unusable_geometry_is_dropped() {
        let mut state = ChatWindowsState::default();
        state.set_windows_of("a", vec![window("a", 1), window("a", 2)]);
        state.set_windows_of("b", vec![window("b", 3)]);
        // Replacing one account's windows leaves the other's alone.
        state.set_windows_of("a", vec![window("a", 2)]);
        assert_eq!(
            state
                .windows_of("a")
                .iter()
                .map(|w| w.chat_id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert_eq!(state.windows_of("b").len(), 1);
        // A window entry filed under another account is not adopted.
        state.set_windows_of("a", vec![window("b", 9)]);
        assert!(state.windows_of("a").is_empty());
        assert!(
            SavedChatWindow {
                width: 10.,
                ..window("a", 1)
            }
            .sanitized()
            .is_none()
        );
        assert!(
            SavedChatWindow {
                x: f32::NAN,
                ..window("a", 1)
            }
            .sanitized()
            .is_none()
        );
    }

    #[test]
    fn the_answer_is_one_value_and_an_unasked_yes_is_not_an_answer() {
        let read = |json: &str| {
            serde_json::from_str::<ChatWindowsState>(json)
                .unwrap()
                .answer
        };
        assert_eq!(read("{}"), RestoreAnswer::Unasked);
        assert_eq!(read(r#"{"asked":true,"restore":true}"#), RestoreAnswer::Yes);
        assert_eq!(read(r#"{"asked":true}"#), RestoreAnswer::No);
        // A stored yes that was never asked for does not restore anything.
        assert_eq!(read(r#"{"restore":true}"#), RestoreAnswer::Unasked);
        // The keys of the file stay as they were.
        let mut state = ChatWindowsState::default();
        state.answer(true);
        let written: serde_json::Value = serde_json::to_value(&state).unwrap();
        assert_eq!(written["asked"], true);
        assert_eq!(written["restore"], true);
    }

    #[test]
    fn the_file_round_trips_and_a_missing_or_broken_one_is_empty() {
        let dir = std::env::temp_dir().join(format!("quill-chat-windows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(FILE_NAME);
        assert_eq!(load_chat_windows_from(&path), ChatWindowsState::default());
        let mut state = ChatWindowsState::default();
        state.answer(true);
        state.set_windows_of("a", vec![window("a", 7)]);
        save_chat_windows_to(&path, &state).unwrap();
        assert_eq!(load_chat_windows_from(&path), state);
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(load_chat_windows_from(&path), ChatWindowsState::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
