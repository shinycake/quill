//! Files dragged over the chat list (Telegram Desktop `Dialogs::Widget`
//! `dragEnterEvent` / `dragMoveEvent` / `dropEvent`): the row under the
//! pointer lights up, a pointer that rests on it for a second opens that chat
//! (`ChoosePeerByDragTimeout`, `chooseRow`), and a drop on it opens the chat
//! and attaches the files the way a drop on the conversation does
//! (`drop_zones`). A drag nothing in it can be sent from (`classify` says
//! `None`) is not accepted, like tdesktop's `ComputeMimeDataState`. While the
//! drag hovers the list its order is frozen (`_shownList->freeze()`,
//! `kFreezeTimeout`), so the lit row is the row the drop lands on. GPUI
//! delivers the same `ExternalPaths` drag on macOS, Windows and Linux.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::status_note::StatusNote;
use quill::drop_modes::{DragEntry, DragState, DropAction, classify};
use quill::forward_drag::{Destination, Drop, ForwardDrag, list_accepts};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// How long the pointer rests on a row before its chat opens
/// (`ChoosePeerByDragTimeout`, `config.h:32`).
const CHOOSE_BY_DRAG: Duration = Duration::from_millis(1000);

/// How long the list order stays frozen after the last drag update
/// (`kFreezeTimeout`, `dialogs_inner_widget.cpp:108`).
const FREEZE_TIMEOUT: Duration = Duration::from_millis(2000);

/// The file drag over the list.
#[derive(Default)]
pub(in crate::ui) struct DropOnRow {
    /// The row the drag is over and can drop on.
    hover: Option<ChatId>,
    /// Where the pointer was at the last update. Platforms resend updates
    /// while it rests, so only a different point restarts the wait.
    position: Option<Point<Pixels>>,
    /// The dragged paths seen last and what they hold, when anything in them
    /// can be sent.
    paths: Vec<PathBuf>,
    drag: Option<DragState>,
    /// The one-second wait that opens the row's chat; replacing or dropping
    /// it cancels it.
    rest: Option<Task<()>>,
    /// Each chat's position when the drag arrived; the list keeps that order
    /// while this is not empty.
    frozen: HashMap<i64, usize>,
    /// Ends the freeze two seconds after the last update.
    unfreeze: Option<Task<()>>,
    /// The messages of a forward drag over the list, seen last. A forward
    /// drag skips the one-second wait (`_dragForward`).
    forward: Option<ForwardDrag>,
    /// The forward drag's pointer is inside the list (`_dragInScroll`).
    in_list: bool,
    /// A screenshot demo's fixed drag (no live drag exists there).
    preview: bool,
}

impl DropOnRow {
    fn same_paths(&self, paths: &[PathBuf]) -> bool {
        self.paths == paths
    }

    /// Back to idle: nothing lit, no wait, no freeze.
    fn clear(&mut self) {
        *self = Self {
            preview: self.preview,
            ..Self::default()
        };
    }
}

/// Sorts `rows` by the position each had when the list froze; rows that came
/// since go last, in their order. Does nothing while the list is not frozen.
fn sort_frozen<T>(rows: &mut [T], frozen: &HashMap<i64, usize>, id: impl Fn(&T) -> i64) {
    if frozen.is_empty() {
        return;
    }
    rows.sort_by_key(|row| frozen.get(&id(row)).copied().unwrap_or(usize::MAX));
}

impl QuillApp {
    /// Whether the row of `chat_id` is the drop target of a live file drag.
    pub(in crate::ui) fn drop_hover_row(&self, chat_id: ChatId, cx: &App) -> bool {
        let state = &self.chat_list.drop_on_row;
        state.hover == Some(chat_id) && (state.preview || cx.has_active_drag())
    }

    /// Keeps the rows in the order they had when a file drag arrived, while
    /// the list is frozen; rows that came since go last, in their order.
    pub(in crate::ui) fn keep_frozen_order<T>(&self, rows: &mut [T], id: impl Fn(&T) -> i64) {
        sort_frozen(rows, &self.chat_list.drop_on_row.frozen, id);
    }

    /// The file drag left the row or the window, or ended.
    pub(in crate::ui) fn drag_left(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.chat_list.drop_on_row;
        if state.hover.is_some() || !state.frozen.is_empty() || state.rest.is_some() {
            state.clear();
            cx.notify();
        }
    }

    /// A file drag moved over the list at `position`, over the row of
    /// `chat_id` (`over`). `fresh` holds the dragged paths when they differ
    /// from the ones seen last.
    pub(in crate::ui) fn drag_over_row(
        &mut self,
        chat_id: ChatId,
        over: bool,
        position: Point<Pixels>,
        fresh: Option<Vec<PathBuf>>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        // A file drag replaces any forward drag seen before.
        let state = &mut self.chat_list.drop_on_row;
        if state.forward.take().is_some() {
            state.in_list = false;
        }
        if !over {
            if self.chat_list.drop_on_row.hover == Some(chat_id) {
                self.chat_list.drop_on_row.hover = None;
                self.chat_list.drop_on_row.rest = None;
                cx.notify();
            }
            return;
        }
        if self.rows_selecting() {
            return;
        }
        self.freeze_list(window, cx);
        let state = &mut self.chat_list.drop_on_row;
        if let Some(paths) = fresh {
            let entries: Vec<DragEntry> = paths.iter().map(|path| DragEntry::probe(path)).collect();
            state.drag = classify(&entries);
            state.paths = paths;
        }
        if state.drag.is_none() {
            if state.hover.take().is_some() {
                state.rest = None;
                cx.notify();
            }
            return;
        }
        if state.hover == Some(chat_id) && state.position == Some(position) {
            return;
        }
        let entered = state.hover != Some(chat_id);
        state.hover = Some(chat_id);
        state.position = Some(position);
        if entered {
            cx.notify();
        }
        // A new row or a real move restarts the wait.
        let rest = self.spawn_scoped_in(window, cx, async move |this, cx| {
            cx.background_executor().timer(CHOOSE_BY_DRAG).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.choose_row_by_drag(chat_id, window, cx);
            });
        });
        self.chat_list.drop_on_row.rest = Some(rest);
    }

    /// Messages dragged out of a history moved over the list at `position`
    /// (`dragMoveEvent` with `_dragForward`): the row under the pointer
    /// lights up and the list keeps its order, but nothing opens by resting
    /// on a row. `over` says the pointer is inside this row, `in_list`
    /// inside the visible list.
    pub(in crate::ui) fn forward_over_row(
        &mut self,
        chat_id: ChatId,
        over: bool,
        in_list: bool,
        drag: &ForwardDrag,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !list_accepts(self.shell_layout().one_column) {
            return;
        }
        let state = &mut self.chat_list.drop_on_row;
        if state.forward.as_ref() != Some(drag) || state.in_list != in_list {
            state.forward = Some(drag.clone());
            state.in_list = in_list;
            cx.notify();
        }
        if !over || self.rows_selecting() {
            if self.chat_list.drop_on_row.hover == Some(chat_id) {
                self.chat_list.drop_on_row.hover = None;
                cx.notify();
            }
            return;
        }
        self.freeze_list(window, cx);
        let state = &mut self.chat_list.drop_on_row;
        if state.hover != Some(chat_id) {
            state.hover = Some(chat_id);
            cx.notify();
        }
    }

    /// How many rows the list order is frozen on.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(in crate::ui) fn frozen_rows(&self) -> usize {
        self.chat_list.drop_on_row.frozen.len()
    }

    /// The forward-info banner shows over the conversation while a forward
    /// drag is inside the list (`showDragForwardInfo`); never in one column.
    pub(in crate::ui) fn forward_banner_shown(&self, cx: &App) -> bool {
        let state = &self.chat_list.drop_on_row;
        state.forward.is_some()
            && state.in_list
            && (state.preview || cx.has_active_drag())
            && list_accepts(self.shell_layout().one_column)
    }

    /// Messages dropped on the row of `chat_id` (`filesOrForwardDrop`): the
    /// chat opens with them waiting in its forward bar, the same state the
    /// Forward box leaves when one chat is chosen. A chat that takes no
    /// forwards refuses with a toast; a forum needs a topic first.
    pub(in crate::ui) fn drop_forward_on_row(
        &mut self,
        chat_id: ChatId,
        drag: &ForwardDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.chat_list.drop_on_row.clear();
        cx.notify();
        if self.rows_selecting() {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        let Some(chat) = session.chats.get(&chat_id.0) else {
            return;
        };
        let route = Destination {
            can_receive: quill::state::Session::can_share_to(chat),
            needs_topic: session.chat_shown_as_forum(chat_id)
                || session.direct_messages_role(chat_id).is_some(),
        }
        .route();
        match route {
            Drop::Cannot => {
                self.connection
                    .set_note(StatusNote::error(tr(keys::FORWARD_CANT)));
                return;
            }
            Drop::ChooseTopic => {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_CHATLIST_OPEN_THE_FORUM_AND_FORWARD,
                )));
                return;
            }
            Drop::Forward => {}
        }
        // Secret chats and slow mode refuse with their own notes.
        if let Some(note) = self.share_dest_refusal(chat_id, cx) {
            self.connection.set_note(StatusNote::error(note));
            return;
        }
        let Some(first) = drag.ids().first().copied() else {
            return;
        };
        let Some(mut draft) =
            quill::composer::ForwardDraft::from_message(drag.from_chat(), first, false)
        else {
            return;
        };
        draft.message_ids = drag.ids().to_vec();
        self.share.pending_forward = Some(draft);
        self.share.forward_bar_dest = Some(chat_id);
        self.share.forward_picker_open = false;
        self.share.selection.clear();
        self.select_listed_chat(chat_id, window, cx);
        self.connection.set_note(StatusNote::silent(tr(
            keys::Q_OPS_FORWARD_BAR_READY_PRESS_SEND,
        )));
        cx.notify();
    }

    /// Freezes the list order at what is shown now, for two more seconds.
    fn freeze_list(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.chat_list.drop_on_row.frozen.is_empty() {
            let order: HashMap<i64, usize> = self
                .chat_list
                .items
                .iter()
                .filter_map(|item| match item {
                    ChatListItem::Chat { id, .. } => Some(id.0),
                    _ => None,
                })
                .enumerate()
                .map(|(position, id)| (id, position))
                .collect();
            self.chat_list.drop_on_row.frozen = order;
        }
        self.chat_list.drop_on_row.unfreeze =
            Some(self.spawn_scoped_in(window, cx, async move |this, cx| {
                cx.background_executor().timer(FREEZE_TIMEOUT).await;
                let _ = this.update(cx, |this, cx| {
                    this.chat_list.drop_on_row.frozen.clear();
                    cx.notify();
                });
            }));
    }

    /// The pointer rested on the row: its chat opens, the drag goes on.
    fn choose_row_by_drag(&mut self, chat_id: ChatId, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.chat_list.drop_on_row;
        state.rest = None;
        if state.hover != Some(chat_id) || !cx.has_active_drag() {
            return;
        }
        self.select_listed_chat(chat_id, window, cx);
    }

    /// Files dropped on the row of `chat_id`: the chat opens and the files
    /// are attached to it, unless it takes none or is a forum.
    pub(in crate::ui) fn drop_on_row(
        &mut self,
        chat_id: ChatId,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.chat_list.drop_on_row;
        // The lit row is the one that takes the drop.
        let drag = state.drag.filter(|_| state.hover == Some(chat_id));
        state.clear();
        cx.notify();
        let Some(drag) = drag else {
            return;
        };
        if self.rows_selecting() {
            return;
        }
        // tdesktop checks before it opens the chat (`filesOrForwardDrop`).
        if let Some(note) = self.start_files_refusal(chat_id) {
            self.connection.set_note(StatusNote::error(note));
            return;
        }
        // A forum (or direct messages group) asks for a topic first in
        // tdesktop (`ShowDropMediaBox`); Quill has no chooser yet.
        let session = self.session();
        if session.is_some_and(|s| {
            s.chat_shown_as_forum(chat_id) || s.direct_messages_role(chat_id).is_some()
        }) {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHATLIST_OPEN_THE_FORUM_AND_DROP,
            )));
            return;
        }
        self.select_listed_chat(chat_id, window, cx);
        // A chat that shows in a window of its own does not become this
        // window's open chat.
        if self.session().and_then(|s| s.open_chat) != Some(chat_id) {
            return;
        }
        match drag {
            // One folder goes as one archive (`history_widget.cpp:7960`).
            DragState::Folder => self.drop_on_zone(DropAction::Archive, paths, cx),
            _ => self.drop_files_plain(paths, cx),
        }
    }

    /// The row wrapped so a file drag can light it up and drop on it.
    pub(in crate::ui) fn drop_row_wrap(
        &self,
        row: AnyElement,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hot = self.drop_hover_row(chat_id, cx);
        div()
            .id(("drop-row", chat_id.0 as u64))
            .test_support()
            .relative()
            .w_full()
            .on_drag_move(cx.listener(
                move |this, event: &DragMoveEvent<ExternalPaths>, window, cx| {
                    let position = event.event.position;
                    // Inside the row and inside the visible list (a row can
                    // sit half under the header).
                    let over = event.bounds.contains(&position)
                        && this
                            .chat_list
                            .scroll
                            .base_handle()
                            .bounds()
                            .contains(&position);
                    let fresh = if over {
                        let paths = event.drag(cx).paths();
                        (!this.chat_list.drop_on_row.same_paths(paths)).then(|| paths.to_vec())
                    } else {
                        None
                    };
                    this.drag_over_row(chat_id, over, position, fresh, window, cx);
                },
            ))
            .on_drag_move(cx.listener(
                move |this, event: &DragMoveEvent<ForwardDrag>, window, cx| {
                    let position = event.event.position;
                    let in_list = this
                        .chat_list
                        .scroll
                        .base_handle()
                        .bounds()
                        .contains(&position);
                    let over = in_list && event.bounds.contains(&position);
                    let drag = event.drag(cx).clone();
                    this.forward_over_row(chat_id, over, in_list, &drag, window, cx);
                },
            ))
            .on_drop(cx.listener(move |this, drag: &ForwardDrag, window, cx| {
                cx.stop_propagation();
                this.drop_forward_on_row(chat_id, drag, window, cx);
            }))
            .on_file_drop_exit(cx.listener(|this, _, _, cx| this.drag_left(cx)))
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, window, cx| {
                cx.stop_propagation();
                this.drop_on_row(chat_id, paths.paths(), window, cx);
            }))
            .child(row)
            .when(hot, |this| {
                this.child(
                    div()
                        .id(("drop-row-hot", chat_id.0 as u64))
                        .test_support()
                        .absolute()
                        .inset_0()
                        .rounded_md()
                        .border_2()
                        .border_color(accent())
                        .bg(cx.theme().accent.opacity(0.12)),
                )
            })
            .into_any_element()
    }
}

#[cfg(feature = "demo-capture")]
crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chats(
        "ready-drop-on-row",
        "screenshot demo — chat list: files dragged over a row"
    )
    .setup(QuillApp::demo_drop_on_row),
    crate::ui::screenshot_demo::DemoSpec::chats(
        "ready-drop-forward",
        "screenshot demo — chat list: messages dragged over a row"
    )
    .setup(QuillApp::demo_drop_forward),
];

#[cfg(feature = "demo-capture")]
impl QuillApp {
    fn demo_drop_on_row(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let state = &mut self.chat_list.drop_on_row;
        state.hover = Some(ChatId(12));
        state.preview = true;
    }

    /// Two selected messages of chat 11 carried over the row of chat 12.
    fn demo_drop_forward(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let chat = ChatId(11);
        let ids = [quill::ids::MessageId(101), quill::ids::MessageId(102)];
        let drag = ForwardDrag::begin(chat, quill::ids::MessageId(101), &ids, false, |_| true).ok();
        if let Some(mut draft) =
            quill::composer::ForwardDraft::from_message(chat, quill::ids::MessageId(101), false)
        {
            draft.toggle(chat, quill::ids::MessageId(102), false);
            self.share.pending_forward = Some(draft);
        }
        let state = &mut self.chat_list.drop_on_row;
        state.hover = Some(ChatId(12));
        state.forward = drag;
        state.in_list = true;
        state.preview = true;
    }
}

#[cfg(test)]
mod tests {
    use super::sort_frozen;
    use std::collections::HashMap;

    #[test]
    fn a_frozen_list_of_thousands_keeps_its_order_and_new_rows_go_last() {
        const N: i64 = 5000;
        let frozen: HashMap<i64, usize> = (0..N).map(|id| (id, id as usize)).collect();
        // Shuffled by new activity (a stride walk coprime with N), plus two new rows.
        let mut rows: Vec<i64> = (0..N).map(|i| (i * 1237) % N).collect();
        rows.push(N + 1);
        rows.push(N);
        sort_frozen(&mut rows, &frozen, |id| *id);
        let expected: Vec<i64> = (0..N).chain([N + 1, N]).collect();
        assert_eq!(rows, expected);
    }

    #[test]
    fn an_unfrozen_list_is_left_alone() {
        let mut rows = vec![3, 1, 2];
        sort_frozen(&mut rows, &HashMap::new(), |id| *id);
        assert_eq!(rows, vec![3, 1, 2]);
    }
}
