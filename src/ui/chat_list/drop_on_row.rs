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
    /// The order of the rows when the drag arrived; the list keeps it while
    /// this is not empty.
    frozen: Vec<i64>,
    /// Ends the freeze two seconds after the last update.
    unfreeze: Option<Task<()>>,
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

impl QuillApp {
    /// Whether the row of `chat_id` is the drop target of a live file drag.
    pub(in crate::ui) fn drop_hover_row(&self, chat_id: ChatId, cx: &App) -> bool {
        let state = &self.chat_list.drop_on_row;
        state.hover == Some(chat_id) && (state.preview || cx.has_active_drag())
    }

    /// Keeps the rows in the order they had when a file drag arrived, while
    /// the list is frozen; rows that came since go last, in their order.
    pub(in crate::ui) fn keep_frozen_order<T>(&self, rows: &mut [T], id: impl Fn(&T) -> i64) {
        let frozen = &self.chat_list.drop_on_row.frozen;
        if frozen.is_empty() {
            return;
        }
        rows.sort_by_key(|row| {
            let id = id(row);
            frozen.iter().position(|seen| *seen == id).unwrap_or(usize::MAX)
        });
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

    /// Freezes the list order at what is shown now, for two more seconds.
    fn freeze_list(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.chat_list.drop_on_row.frozen.is_empty() {
            let order: Vec<i64> = self
                .chat_list
                .items
                .iter()
                .filter_map(|item| match item {
                    ChatListItem::Chat { id, .. } => Some(id.0),
                    _ => None,
                })
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
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_CHATLIST_OPEN_THE_FORUM_AND_DROP)));
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
];

#[cfg(feature = "demo-capture")]
impl QuillApp {
    fn demo_drop_on_row(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let state = &mut self.chat_list.drop_on_row;
        state.hover = Some(ChatId(12));
        state.preview = true;
    }
}
