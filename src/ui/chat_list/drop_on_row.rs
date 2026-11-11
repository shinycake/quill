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

use super::drop_topic::DropPayload;
use super::*;
use crate::ui::status_note::StatusNote;
use quill::drop_modes::{DragEntry, DragState, classify};
use quill::forward_drag::{Destination, Drop, ForwardDrag, list_accepts};
use quill::suggested_posts::DirectMessagesRole;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// How long the pointer rests on a row before its chat opens
/// (`ChoosePeerByDragTimeout`, `config.h:32`).
const CHOOSE_BY_DRAG: Duration = Duration::from_millis(1000);

/// How long the list order stays frozen after the last drag update
/// (`kFreezeTimeout`, `dialogs_inner_widget.cpp:108`).
const FREEZE_TIMEOUT: Duration = Duration::from_millis(2000);

/// The drag over the list.
#[derive(Default)]
pub(in crate::ui) struct DropOnRow {
    /// The row the drag is over and can drop on.
    hover: Option<ChatId>,
    /// What is being dragged over the list.
    seen: Dragged,
    /// The list order held while a drag hovers.
    freeze: Freeze,
    /// A screenshot demo's fixed drag (no live drag exists there).
    preview: bool,
}

/// The drag seen last: files, or messages out of a history (never both).
#[derive(Default)]
enum Dragged {
    #[default]
    Nothing,
    Files(FileDrag),
    /// The messages of a forward drag over the list. A forward drag skips
    /// the one-second wait (`_dragForward`).
    Forward {
        drag: ForwardDrag,
        /// Its pointer is inside the list (`_dragInScroll`).
        in_list: bool,
    },
}

/// A file drag over the list.
#[derive(Default)]
struct FileDrag {
    /// Where the pointer was at the last update. Platforms resend updates
    /// while it rests, so only a different point restarts the wait.
    position: Option<Point<Pixels>>,
    /// The dragged paths seen last.
    paths: Vec<PathBuf>,
    /// What they hold, when anything in them can be sent.
    kind: Option<DragState>,
    /// The one-second wait that opens the row's chat; replacing or dropping
    /// it cancels it.
    rest: Option<Task<()>>,
}

impl FileDrag {
    fn has_state(&self) -> bool {
        self.kind.is_some() || self.rest.is_some() || !self.paths.is_empty()
    }
}

/// The list order: free, or held on each chat's position when the drag
/// arrived, until two seconds after the last update.
#[derive(Default)]
enum Freeze {
    #[default]
    Thawed,
    Frozen {
        /// Empty once the freeze ended while its timer still runs.
        order: HashMap<i64, usize>,
        _timer: Task<()>,
    },
}

impl Freeze {
    fn order(&self) -> Option<&HashMap<i64, usize>> {
        match self {
            Self::Thawed => None,
            Self::Frozen { order, .. } => Some(order),
        }
    }

    fn is_held(&self) -> bool {
        self.order().is_some_and(|order| !order.is_empty())
    }

    /// The order is released; a running timer stays.
    fn release_order(&mut self) {
        if let Self::Frozen { order, .. } = self {
            order.clear();
        }
    }
}

impl DropOnRow {
    fn same_paths(&self, paths: &[PathBuf]) -> bool {
        match &self.seen {
            Dragged::Files(files) => files.paths == paths,
            _ => paths.is_empty(),
        }
    }

    /// Work on the file drag, which a file drag over the list starts.
    fn with_files<R>(&mut self, work: impl FnOnce(&mut FileDrag) -> R) -> R {
        if !matches!(self.seen, Dragged::Files(_)) {
            self.seen = Dragged::Files(FileDrag::default());
        }
        match &mut self.seen {
            Dragged::Files(files) => work(files),
            Dragged::Nothing | Dragged::Forward { .. } => work(&mut FileDrag::default()),
        }
    }

    /// What the dragged files hold, when anything in them can be sent.
    fn file_kind(&self) -> Option<DragState> {
        match &self.seen {
            Dragged::Files(files) => files.kind,
            _ => None,
        }
    }

    /// The wait for the row's chat ends.
    fn stop_rest(&mut self) {
        if let Dragged::Files(files) = &mut self.seen {
            files.rest = None;
        }
    }

    fn is_forward(&self) -> bool {
        matches!(self.seen, Dragged::Forward { .. })
    }

    /// Back to idle: nothing lit, no wait, no freeze.
    fn clear(&mut self) {
        *self = Self {
            preview: self.preview,
            ..Self::default()
        };
    }

    /// The drag is over (cancelled with no exit event, or dropped
    /// elsewhere): what it showed and what was probed for it go, so the next
    /// drag of the same paths is probed again. The running freeze timer is
    /// left alone.
    fn drag_ended(&mut self) {
        let mut freeze = std::mem::take(&mut self.freeze);
        freeze.release_order();
        self.clear();
        self.freeze = freeze;
    }

    /// A forward drag came over the list: it replaces any file drag seen
    /// before, with its one-second wait (forward drags never open a chat by
    /// resting, `_dragForward`).
    fn forward_takes_over(&mut self) {
        if let Dragged::Files(files) = &self.seen {
            if files.has_state() {
                self.hover = None;
            }
            self.seen = Dragged::Nothing;
        }
    }
}

/// A drop on this chat asks for a topic first: a forum, or the direct
/// messages group an administrator reads (`ShowDropMediaBox` for a forum or a
/// monoforum).
fn needs_topic(session: &quill::state::Session, chat_id: ChatId) -> bool {
    session.chat_shown_as_forum(chat_id)
        || session.direct_messages_role(chat_id) == Some(DirectMessagesRole::Admin)
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
        if let Some(order) = self.chat_list.drop_on_row.freeze.order() {
            sort_frozen(rows, order, id);
        }
    }

    /// The file drag left the row or the window, or ended.
    pub(in crate::ui) fn drag_left(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.chat_list.drop_on_row;
        let files_state = matches!(&state.seen, Dragged::Files(files) if files.has_state());
        if state.hover.is_some() || state.freeze.is_held() || files_state {
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
        if state.is_forward() {
            state.seen = Dragged::Nothing;
        }
        if !over {
            if self.chat_list.drop_on_row.hover == Some(chat_id) {
                self.chat_list.drop_on_row.hover = None;
                self.chat_list.drop_on_row.stop_rest();
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
            state.with_files(|files| {
                files.kind = classify(&entries);
                files.paths = paths;
            });
        }
        if state.file_kind().is_none() {
            if state.hover.take().is_some() {
                state.stop_rest();
                cx.notify();
            }
            return;
        }
        let moved = state.with_files(|files| {
            let same = files.position == Some(position);
            files.position = Some(position);
            !same
        });
        if state.hover == Some(chat_id) && !moved {
            return;
        }
        let entered = state.hover != Some(chat_id);
        state.hover = Some(chat_id);
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
        self.chat_list
            .drop_on_row
            .with_files(|files| files.rest = Some(rest));
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
        state.forward_takes_over();
        let unchanged = matches!(
            &state.seen,
            Dragged::Forward { drag: seen, in_list: was } if seen.same(drag) && *was == in_list
        );
        if !unchanged {
            state.seen = Dragged::Forward {
                drag: drag.clone(),
                in_list,
            };
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
        self.chat_list
            .drop_on_row
            .freeze
            .order()
            .map_or(0, HashMap::len)
    }

    /// A lifted message drag ended (dropped on a row, elsewhere, or
    /// cancelled): nothing stays lit and the list unfreezes.
    pub(in crate::ui) fn end_forward_drag(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.chat_list.drop_on_row;
        if state.is_forward() {
            state.clear();
            cx.notify();
        }
    }

    /// The forward-info banner shows over the conversation while a forward
    /// drag is inside the list (`showDragForwardInfo`); never in one column.
    pub(in crate::ui) fn forward_banner_shown(&self, cx: &App) -> bool {
        let state = &self.chat_list.drop_on_row;
        matches!(state.seen, Dragged::Forward { in_list: true, .. })
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
            needs_topic: needs_topic(session, chat_id),
        }
        .route();
        match route {
            Drop::Cannot => {
                self.set_note(StatusNote::error(tr(keys::FORWARD_CANT)));
                return;
            }
            Drop::ChooseTopic => {
                if session.chat_shown_as_forum(chat_id) {
                    self.open_drop_topic(chat_id, DropPayload::Forward(drag.clone()), window, cx);
                } else {
                    // A direct messages group an administrator reads has
                    // conversations, not topics; Quill has no chooser for them.
                    self.set_note(StatusNote::silent(tr(
                        keys::Q_CHATLIST_OPEN_THE_DIRECT_MESSAGES_AND_FORWARD,
                    )));
                }
                return;
            }
            Drop::Forward => {}
        }
        // Secret chats and slow mode refuse with their own notes.
        if let Some(note) = self.share_dest_refusal(chat_id, cx) {
            self.set_note(StatusNote::error(note));
            return;
        }
        // The forward rides with the open, to the window that shows the
        // chat (its own, when it has one).
        if let Some(draft) = Self::forward_draft_of(drag, None) {
            let open = crate::ui::chat_window::ChatOpen::new(chat_id).forwarding(draft);
            self.select_chat_request(open, window, cx);
        }
    }

    /// The forward of `drag`'s messages, for `topic` when there is one.
    /// None when nothing could be carried.
    pub(in crate::ui) fn forward_draft_of(
        drag: &ForwardDrag,
        topic: Option<i32>,
    ) -> Option<quill::composer::ForwardDraft> {
        let first = drag.ids().first().copied()?;
        // A dropped forward rides to its destination's forward bar, which
        // keeps it whatever the view shows; it is no selection of a topic.
        let mut draft = quill::composer::ForwardDraft::from_message(
            drag.from_chat(),
            quill::ids::TopicId::None,
            first,
            false,
        )?;
        draft.message_ids = drag.ids().to_vec();
        draft.topic_id = topic;
        Some(draft)
    }

    /// Freezes the list order at what is shown now, for two more seconds.
    fn freeze_list(&mut self, window: &Window, cx: &mut Context<Self>) {
        let held = match &mut self.chat_list.drop_on_row.freeze {
            Freeze::Frozen { order, .. } if !order.is_empty() => Some(std::mem::take(order)),
            _ => None,
        };
        let order = held.unwrap_or_else(|| {
            self.chat_list
                .items
                .iter()
                .filter_map(|item| match item {
                    ChatListItem::Chat { id, .. } => Some(id.0),
                    _ => None,
                })
                .enumerate()
                .map(|(position, id)| (id, position))
                .collect()
        });
        let timer = self.spawn_scoped_in(window, cx, async move |this, cx| {
            cx.background_executor().timer(FREEZE_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                let state = &mut this.chat_list.drop_on_row;
                if cx.has_active_drag() {
                    state.freeze.release_order();
                } else {
                    // No drag any more and no exit event told us (Esc).
                    state.drag_ended();
                }
                cx.notify();
            });
        });
        self.chat_list.drop_on_row.freeze = Freeze::Frozen {
            order,
            _timer: timer,
        };
    }

    /// The pointer rested on the row: its chat opens, the drag goes on.
    fn choose_row_by_drag(&mut self, chat_id: ChatId, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.chat_list.drop_on_row;
        state.stop_rest();
        if state.hover != Some(chat_id) || state.is_forward() || !cx.has_active_drag() {
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
        let drag = state.file_kind().filter(|_| state.hover == Some(chat_id));
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
            self.set_note(StatusNote::error(note));
            return;
        }
        // A forum asks for a topic first (`ShowDropMediaBox`).
        if let Some(session) = self.session()
            && needs_topic(session, chat_id)
        {
            if session.chat_shown_as_forum(chat_id) {
                let payload = DropPayload::Files {
                    paths: paths.to_vec(),
                    kind: drag,
                };
                self.open_drop_topic(chat_id, payload, window, cx);
            } else {
                self.set_note(StatusNote::silent(tr(
                    keys::Q_CHATLIST_OPEN_THE_DIRECT_MESSAGES_AND_DROP,
                )));
            }
            return;
        }
        // The files ride with the open, to the window that shows the chat
        // (its own, when it has one). One folder goes as one archive
        // (`history_widget.cpp:7960`).
        let archive = matches!(drag, DragState::Folder);
        let open =
            crate::ui::chat_window::ChatOpen::new(chat_id).with_files(paths.to_vec(), archive);
        self.select_chat_request(open, window, cx);
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
    crate::ui::screenshot_demo::DemoSpec::chats(
        "ready-drop-media-forward",
        "screenshot demo — chat list: one album picture dragged over a row"
    )
    .setup(QuillApp::demo_drop_media_forward),
];

#[cfg(feature = "demo-capture")]
impl QuillApp {
    fn demo_drop_on_row(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let state = &mut self.chat_list.drop_on_row;
        state.hover = Some(ChatId(12));
        state.preview = true;
    }

    /// The video of the own album (message 812 of chat 11), picked up by its
    /// picture, carried over the row of chat 12.
    fn demo_drop_media_forward(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let chat = ChatId(11);
        let id = quill::ids::MessageId(812);
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui
                .seq
                .store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
            crate::ui::history::apply_ready_albums(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        let drag = self.session().and_then(|session| {
            super::super::forward_drag_source::drag_for(session, chat, id, &[])
        });
        let state = &mut self.chat_list.drop_on_row;
        state.hover = Some(ChatId(12));
        if let Some(drag) = drag {
            state.seen = Dragged::Forward {
                drag,
                in_list: true,
            };
        }
        state.preview = true;
    }

    /// Two selected messages of chat 11 carried over the row of chat 12.
    fn demo_drop_forward(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let chat = ChatId(11);
        let ids = [quill::ids::MessageId(101), quill::ids::MessageId(102)];
        let drag = ForwardDrag::begin(chat, quill::ids::MessageId(101), &ids, false, |_| true).ok();
        if let Some(mut draft) = quill::composer::ForwardDraft::from_message(
            chat,
            quill::ids::TopicId::None,
            quill::ids::MessageId(101),
            false,
        ) {
            draft.toggle(
                chat,
                quill::ids::TopicId::None,
                quill::ids::MessageId(102),
                false,
            );
            self.share.forward = crate::ui::forward_stage::ForwardStage::Selecting(draft);
        }
        let state = &mut self.chat_list.drop_on_row;
        state.hover = Some(ChatId(12));
        if let Some(drag) = drag {
            state.seen = Dragged::Forward {
                drag,
                in_list: true,
            };
        }
        state.preview = true;
    }
}

#[cfg(test)]
mod tests {
    use super::{Dragged, DropOnRow, FileDrag, sort_frozen};
    use gpui_kit::Task;
    use quill::drop_modes::DragState;
    use quill::ids::ChatId;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn over_a_file_drag() -> DropOnRow {
        DropOnRow {
            hover: Some(ChatId(1)),
            seen: Dragged::Files(FileDrag {
                paths: vec![PathBuf::from("a")],
                kind: Some(DragState::Folder),
                rest: Some(Task::ready(())),
                ..FileDrag::default()
            }),
            ..DropOnRow::default()
        }
    }

    #[test]
    fn a_forward_drag_drops_the_file_drags_wait_and_probe() {
        let mut state = over_a_file_drag();
        state.forward_takes_over();
        assert!(
            matches!(state.seen, Dragged::Nothing),
            "no timer opens the chat"
        );
        assert!(state.file_kind().is_none() && state.hover.is_none());
        assert!(state.same_paths(&[]));
    }

    #[test]
    fn a_drag_that_ended_is_probed_again_next_time() {
        let mut state = over_a_file_drag();
        state.drag_ended();
        assert!(!state.same_paths(&[PathBuf::from("a")]));
        assert!(
            state.file_kind().is_none()
                && state.hover.is_none()
                && matches!(state.seen, Dragged::Nothing)
        );
    }

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
