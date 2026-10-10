//! Fast buttons mode, after tdesktop's `FastButtonsBots` and
//! `HistoryWidget::setupFastButtonMode`: for bots the person switched it on
//! for, keys 1 to 9 press the inline buttons of the chat's last message while
//! the composer is empty. The bot ids live in `fast_buttons.json` under the
//! app data root (isolated in screenshot demos).

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Most buttons a digit key can reach (keys 1 to 9).
pub const MAX_FAST_BUTTONS: usize = 9;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    ids: BTreeSet<i64>,
}

static BOTS: RwLock<Option<Stored>> = RwLock::new(None);
static PERSIST: AtomicBool = AtomicBool::new(true);

/// Turn disk writes off (tests keep the set in memory only).
pub fn set_persistence(enabled: bool) {
    PERSIST.store(enabled, Ordering::SeqCst);
}

fn path() -> Option<PathBuf> {
    crate::settings::safe_app_root().map(|root| root.join("fast_buttons.json"))
}

fn current() -> Stored {
    if let Ok(guard) = BOTS.read()
        && let Some(stored) = guard.as_ref()
    {
        return stored.clone();
    }
    let loaded = path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
        .unwrap_or_default();
    if let Ok(mut guard) = BOTS.write() {
        return guard.get_or_insert(loaded).clone();
    }
    loaded
}

/// Whether fast buttons mode is on for this bot.
pub fn is_enabled(bot_id: i64) -> bool {
    current().ids.contains(&bot_id)
}

/// Switch the mode for one bot and persist it.
pub fn set_enabled(bot_id: i64, on: bool) {
    let mut stored = current();
    let changed = if on {
        stored.ids.insert(bot_id)
    } else {
        stored.ids.remove(&bot_id)
    };
    if !changed {
        return;
    }
    if PERSIST.load(Ordering::SeqCst)
        && let Some(path) = path()
    {
        let _ = crate::settings::write_json_atomic(&path, &stored);
    }
    if let Ok(mut guard) = BOTS.write() {
        *guard = Some(stored);
    }
}

/// The 0-based button index a bare digit key stands for: `"1"` is 0 and
/// `"9"` is 8. Any modifier, or any other key, is not a fast button
/// (tdesktop lets those through to the field).
pub fn index_for_key(key: &str, has_modifiers: bool) -> Option<usize> {
    if has_modifiers {
        return None;
    }
    let mut chars = key.chars();
    let digit = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    match digit.to_digit(10)? {
        0 => None,
        n => Some(n as usize - 1),
    }
}

/// Row and column of the `index`-th button when the rows are counted left to
/// right, top to bottom (tdesktop `ReplyKeyboard::getLinkByIndex`). Empty
/// rows add nothing.
pub fn locate(row_lens: &[usize], index: usize) -> Option<(usize, usize)> {
    let mut left = index;
    for (row, len) in row_lens.iter().enumerate() {
        if left < *len {
            return Some((row, left));
        }
        left -= len;
    }
    None
}

/// The digit drawn on the button at `row`/`col`, or `None` past the ninth.
pub fn badge_number(row_lens: &[usize], row: usize, col: usize) -> Option<u8> {
    let before: usize = row_lens.iter().take(row).sum();
    let flat = before + col;
    (col < *row_lens.get(row)? && flat < MAX_FAST_BUTTONS).then(|| flat as u8 + 1)
}

#[cfg(test)]
mod tests {
    use super::{badge_number, index_for_key, locate};

    #[test]
    fn digits_map_to_zero_based_indexes() {
        assert_eq!(index_for_key("1", false), Some(0));
        assert_eq!(index_for_key("9", false), Some(8));
        assert_eq!(index_for_key("0", false), None);
        assert_eq!(index_for_key("a", false), None);
        assert_eq!(index_for_key("12", false), None);
        assert_eq!(index_for_key("", false), None);
    }

    #[test]
    fn modifiers_leave_the_key_alone() {
        assert_eq!(index_for_key("3", true), None);
    }

    #[test]
    fn locate_counts_across_rows_and_skips_empty_ones() {
        let rows = [2, 0, 3];
        assert_eq!(locate(&rows, 0), Some((0, 0)));
        assert_eq!(locate(&rows, 1), Some((0, 1)));
        assert_eq!(locate(&rows, 2), Some((2, 0)));
        assert_eq!(locate(&rows, 4), Some((2, 2)));
        assert_eq!(locate(&rows, 5), None);
        assert_eq!(locate(&[], 0), None);
    }

    #[test]
    fn badges_stop_at_nine() {
        let rows = [5, 5];
        assert_eq!(badge_number(&rows, 0, 0), Some(1));
        assert_eq!(badge_number(&rows, 1, 3), Some(9));
        assert_eq!(badge_number(&rows, 1, 4), None);
        assert_eq!(badge_number(&rows, 2, 0), None);
        assert_eq!(badge_number(&rows, 0, 7), None);
    }

    #[test]
    fn badges_and_locate_agree() {
        let rows = [3, 2, 4];
        for index in 0..9 {
            let (row, col) = locate(&rows, index).unwrap();
            assert_eq!(badge_number(&rows, row, col), Some(index as u8 + 1));
        }
    }
}
