//! Pack strings live for the rest of the process: each distinct text is
//! stored once, so render code can borrow it as `&'static str` and a
//! switch back to a language used before adds nothing.
//!
//! The table only grows by texts not seen before, so it is bounded by the
//! strings of the packs used in this run (a few hundred KB per language).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

static TABLE: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();

/// `text` as a process-lifetime string, shared with every earlier equal
/// text. Called while building snapshots, never on a render path.
pub(crate) fn intern(text: &str) -> &'static str {
    let table = TABLE.get_or_init(|| Mutex::new(HashSet::new()));
    let mut set = table
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = set.get(text) {
        return existing;
    }
    let stored: &'static str = Box::leak(text.to_owned().into_boxed_str());
    set.insert(stored);
    stored
}

#[cfg(test)]
mod tests {
    use super::intern;

    #[test]
    fn equal_texts_share_one_copy() {
        let a = intern(&String::from("Configurações"));
        let b = intern(&String::from("Configurações"));
        assert!(std::ptr::eq(a, b));
        assert_eq!(a, "Configurações");
    }
}
