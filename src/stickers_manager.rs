//! The stickers manager box's local edits: which sets were removed and in
//! what order the installed ones are shown, kept until the box closes.
//!
//! tdesktop does the same (`StickersBox::Inner::fullOrder`,
//! `removedSets`): removing a row only marks it, "Undo" clears the mark,
//! dragging only moves the row, and `saveChanges` sends both when the box
//! closes.

/// Removals and the order of the installed sets, not yet sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagerEdits {
    removed: Vec<i64>,
    order: Option<Vec<i64>>,
}

impl ManagerEdits {
    pub fn is_removed(&self, id: i64) -> bool {
        self.removed.contains(&id)
    }

    /// Mark the set removed, or take the mark off (Undo).
    pub fn set_removed(&mut self, id: i64, removed: bool) {
        if removed {
            if !self.removed.contains(&id) {
                self.removed.push(id);
            }
        } else {
            self.removed.retain(|other| *other != id);
        }
    }

    /// The installed sets in the order the box shows them: the order they
    /// were dragged to, then any set that arrived later.
    pub fn arranged(&self, installed: &[i64]) -> Vec<i64> {
        let Some(order) = &self.order else {
            return installed.to_vec();
        };
        let mut arranged: Vec<i64> = order
            .iter()
            .copied()
            .filter(|id| installed.contains(id))
            .collect();
        for id in installed {
            if !arranged.contains(id) {
                arranged.push(*id);
            }
        }
        arranged
    }

    /// Drop `dragged` onto the row of `target`: it takes that slot and the
    /// rows in between shift by one.
    pub fn drag_onto(&mut self, installed: &[i64], dragged: i64, target: i64) {
        let mut arranged = self.arranged(installed);
        let (Some(from), Some(to)) = (
            arranged.iter().position(|id| *id == dragged),
            arranged.iter().position(|id| *id == target),
        ) else {
            return;
        };
        if from == to {
            return;
        }
        arranged.remove(from);
        let at = to.min(arranged.len());
        arranged.insert(at, dragged);
        self.order = Some(arranged);
    }

    /// What to send when the box closes: the sets to uninstall, and the
    /// order of the rest when it changed.
    pub fn changes(&self, installed: &[i64]) -> (Vec<i64>, Option<Vec<i64>>) {
        let removed: Vec<i64> = self
            .removed
            .iter()
            .copied()
            .filter(|id| installed.contains(id))
            .collect();
        let kept = |ids: &[i64]| -> Vec<i64> {
            ids.iter()
                .copied()
                .filter(|id| !removed.contains(id))
                .collect()
        };
        let arranged = kept(&self.arranged(installed));
        let order = (arranged != kept(installed)).then_some(arranged);
        (removed, order)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_marks_and_undo_clears() {
        let mut edits = ManagerEdits::default();
        edits.set_removed(2, true);
        edits.set_removed(2, true);
        assert!(edits.is_removed(2));
        assert_eq!(edits.changes(&[1, 2, 3]), (vec![2], None));
        edits.set_removed(2, false);
        assert!(!edits.is_removed(2));
        assert_eq!(edits.changes(&[1, 2, 3]), (vec![], None));
    }

    #[test]
    fn dragging_takes_the_slot_of_the_row_it_lands_on() {
        let mut edits = ManagerEdits::default();
        edits.drag_onto(&[1, 2, 3, 4], 1, 3);
        assert_eq!(edits.arranged(&[1, 2, 3, 4]), vec![2, 3, 1, 4]);
        edits.drag_onto(&[1, 2, 3, 4], 4, 2);
        assert_eq!(edits.arranged(&[1, 2, 3, 4]), vec![4, 2, 3, 1]);
        assert_eq!(edits.changes(&[1, 2, 3, 4]).1, Some(vec![4, 2, 3, 1]));
    }

    #[test]
    fn unknown_or_same_rows_change_nothing() {
        let mut edits = ManagerEdits::default();
        edits.drag_onto(&[1, 2], 1, 1);
        edits.drag_onto(&[1, 2], 9, 1);
        edits.drag_onto(&[1, 2], 1, 9);
        assert_eq!(edits, ManagerEdits::default());
    }

    #[test]
    fn dragging_back_to_the_start_sends_no_order() {
        let mut edits = ManagerEdits::default();
        edits.drag_onto(&[1, 2, 3], 1, 3);
        edits.drag_onto(&[1, 2, 3], 1, 2);
        assert_eq!(edits.arranged(&[1, 2, 3]), vec![1, 2, 3]);
        assert_eq!(edits.changes(&[1, 2, 3]), (vec![], None));
    }

    #[test]
    fn removed_sets_leave_the_order_to_send() {
        let mut edits = ManagerEdits::default();
        edits.drag_onto(&[1, 2, 3], 3, 1);
        edits.set_removed(2, true);
        assert_eq!(edits.changes(&[1, 2, 3]), (vec![2], Some(vec![3, 1])));
    }

    #[test]
    fn a_set_removed_elsewhere_is_not_sent_again() {
        let mut edits = ManagerEdits::default();
        edits.set_removed(7, true);
        assert_eq!(edits.changes(&[1, 2]), (vec![], None));
    }

    #[test]
    fn sets_that_arrive_later_go_last() {
        let mut edits = ManagerEdits::default();
        edits.drag_onto(&[1, 2, 3], 3, 1);
        assert_eq!(edits.arranged(&[1, 2, 3, 4]), vec![3, 1, 2, 4]);
        assert_eq!(edits.arranged(&[3, 1]), vec![3, 1]);
    }
}
