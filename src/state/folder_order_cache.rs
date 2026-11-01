//! The order of a folder tab's chats, kept between sidebar renders.
//!
//! The order is a function of `Session::chats` (each chat's position in the
//! folder). A folder's stored order is dropped whenever a chat's position in
//! that folder changes (the main and archive orders do not feed it), so a
//! missing order is computed again from the chats, exactly as an uncached
//! read would.

use super::*;
use std::cell::RefCell;

#[derive(Debug, Default)]
pub(crate) struct FolderOrderCache {
    orders: RefCell<HashMap<i32, Vec<ChatId>>>,
}

impl FolderOrderCache {
    /// A chat's position in `folder_id` changed.
    pub(crate) fn invalidate(&mut self, folder_id: i32) {
        self.orders.get_mut().remove(&folder_id);
    }

    #[cfg(test)]
    pub(crate) fn stores(&self, folder_id: i32) -> bool {
        self.orders.borrow().contains_key(&folder_id)
    }

    /// The ids of `folder_id`'s chats in order: the stored list, else
    /// `compute`'s (which is then stored).
    pub(crate) fn ids(&self, folder_id: i32, compute: impl FnOnce() -> Vec<ChatId>) -> Vec<ChatId> {
        // Never held across `compute`, and a busy cell just skips the cache.
        if let Ok(orders) = self.orders.try_borrow()
            && let Some(ids) = orders.get(&folder_id)
        {
            return ids.clone();
        }
        let ids = compute();
        if let Ok(mut orders) = self.orders.try_borrow_mut() {
            orders.insert(folder_id, ids.clone());
        }
        ids
    }
}

impl Session {
    /// Chats in `chatListFolder(folder_id)`, highest TDLib folder order
    /// first, ties by higher chat id.
    pub fn ordered_folder_chats(&self, folder_id: i32) -> Vec<&ChatSummary> {
        let ids = self
            .chat_order_sync
            .folder_orders
            .ids(folder_id, || self.compute_folder_order(folder_id));
        ids.iter()
            .filter_map(|id| self.chats.get(&id.0))
            .filter(|chat| chat.folder_positions.contains_key(&folder_id))
            .collect()
    }

    fn compute_folder_order(&self, folder_id: i32) -> Vec<ChatId> {
        let mut rows: Vec<&ChatSummary> = self
            .chats
            .values()
            .filter(|chat| chat.folder_positions.contains_key(&folder_id))
            .collect();
        rows.sort_by(|a, b| {
            b.folder_positions
                .get(&folder_id)
                .cmp(&a.folder_positions.get(&folder_id))
                .then(b.id.0.cmp(&a.id.0))
        });
        rows.into_iter().map(|chat| chat.id).collect()
    }
}
