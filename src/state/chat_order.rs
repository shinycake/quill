//! Keeps `Session::main_order` / `archive_order` in TDLib order (descending
//! `(order, chat_id)`) by placing only the chats that moved, instead of
//! sorting every chat after each poll batch.
use super::*;

impl Session {
    pub(super) fn place_moved_chats(&mut self) {
        let moved = std::mem::take(&mut self.chat_order_sync.moved);
        // Placing costs a shift of the list per chat: past this share of
        // the chats (the first loads), one sort is cheaper.
        if moved.len().saturating_mul(8) > self.chats.len() {
            self.rebuild_main_order();
            return;
        }
        place(&mut self.main_order, &self.chats, &moved, |chat| {
            chat.in_main_list.then_some(chat.order)
        });
        place(&mut self.archive_order, &self.chats, &moved, |chat| {
            chat.in_archive.then_some(chat.archive_order)
        });
    }
}

/// Drop the moved chats (and any that left the list) from `list`, then
/// insert each moved chat still in it at its sorted place.
fn place(
    list: &mut Vec<ChatId>,
    chats: &HashMap<i64, ChatSummary>,
    moved: &HashSet<i64>,
    order: impl Fn(&ChatSummary) -> Option<i64>,
) {
    let rank = |id: i64| chats.get(&id).and_then(&order).map(|order| (order, id));
    list.retain(|id| !moved.contains(&id.0) && rank(id.0).is_some());
    for &id in moved {
        let Some(key) = rank(id) else {
            continue;
        };
        let at = list.partition_point(|other| rank(other.0) > Some(key));
        list.insert(at, ChatId(id));
    }
}
