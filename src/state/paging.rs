//! Helpers for merging paged results.

use std::collections::HashSet;
use std::hash::Hash;

/// Append `incoming` items whose id isn't already in `merged` (or earlier in
/// `incoming`), keeping order. Linear in both lists.
pub(super) fn append_new_by_id<T, K: Hash + Eq>(
    merged: &mut Vec<T>,
    incoming: Vec<T>,
    id: impl Fn(&T) -> K,
) {
    let mut seen: HashSet<K> = merged.iter().map(&id).collect();
    for item in incoming {
        if seen.insert(id(&item)) {
            merged.push(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::append_new_by_id;

    #[test]
    fn appends_only_unseen_ids_in_order() {
        let mut merged = vec![(5, "a"), (3, "b")];
        append_new_by_id(
            &mut merged,
            vec![(3, "dup"), (2, "c"), (2, "dup2"), (1, "d")],
            |item| item.0,
        );
        assert_eq!(merged, vec![(5, "a"), (3, "b"), (2, "c"), (1, "d")]);
    }
}
