//! Files dragged over the chat list (Telegram Desktop `Dialogs::Widget`
//! `dragEnterEvent` / `dragMoveEvent` / `dropEvent`): the row under the
//! pointer lights up, a pointer that rests on it for a second opens that chat
//! (`ChoosePeerByDragTimeout`, `chooseRow`), and a drop on it opens the chat
//! and attaches the files the way a drop on the conversation does
//! (`drop_zones`). A drag nothing in it can be sent from (`classify` says
//! `None`) is not accepted, like tdesktop's `ComputeMimeDataState`. GPUI
//! delivers the same `ExternalPaths` drag on macOS, Windows and Linux.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::drop_modes::{DragEntry, classify};
use std::path::PathBuf;
use std::time::Duration;

/// How long the pointer rests on a row before its chat opens
/// (`ChoosePeerByDragTimeout`, `config.h:32`).
pub(super) const CHOOSE_BY_DRAG: Duration = Duration::from_millis(1000);

/// How often the rest timer looks at the pointer.
const TICK: Duration = Duration::from_millis(100);

/// The file drag over the list.
#[derive(Default)]
pub(in crate::ui) struct DropOnRow {
    /// The row the drag is over and can drop on.
    hover: Option<ChatId>,
    /// Bumped when the row changes, so a stale rest timer stops.
    generation: u64,
    /// Timer ticks since the pointer last moved over the row; `None` once the
    /// row has been chosen.
    idle_ticks: Option<u32>,
    /// The dragged paths seen last and whether anything in them can be sent.
    paths: Vec<PathBuf>,
    accepted: bool,
    /// A screenshot demo's fixed drag (no live drag exists there).
    preview: bool,
}

impl DropOnRow {
    /// Whether the pointer has rested on a row for long enough to open it.
    pub(super) fn rested(idle_ticks: Option<u32>) -> bool {
        idle_ticks.is_some_and(|ticks| TICK * ticks >= CHOOSE_BY_DRAG)
    }
}

impl QuillApp {
    /// Whether the row of `chat_id` is the drop target of a live file drag.
    pub(in crate::ui) fn drop_hover_row(&self, chat_id: ChatId, cx: &App) -> bool {
        let state = &self.chat_list.drop_on_row;
        state.hover == Some(chat_id) && (state.preview || cx.has_active_drag())
    }

    /// A file drag moved while over (`over`) or outside the row of `chat_id`.
    pub(in crate::ui) fn drag_over_row(
        &mut self,
        chat_id: ChatId,
        over: bool,
        paths: &[PathBuf],
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !over || self.rows_selecting() {
            if self.chat_list.drop_on_row.hover == Some(chat_id) {
                self.chat_list.drop_on_row.hover = None;
                cx.notify();
            }
            return;
        }
        let state = &mut self.chat_list.drop_on_row;
        if state.paths != paths {
            state.paths = paths.to_vec();
            let entries: Vec<DragEntry> = paths.iter().map(|path| DragEntry::probe(path)).collect();
            state.accepted = classify(&entries).is_some();
        }
        if !state.accepted {
            if state.hover.take().is_some() {
                cx.notify();
            }
            return;
        }
        state.idle_ticks = Some(0);
        if state.hover == Some(chat_id) {
            return;
        }
        state.hover = Some(chat_id);
        state.generation += 1;
        let generation = state.generation;
        cx.notify();
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let keep_going = this
                    .update_in(cx, |this, window, cx| {
                        this.choose_row_by_drag(chat_id, generation, window, cx)
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
    }

    /// One look of the rest timer. `true` while it should keep watching.
    fn choose_row_by_drag(
        &mut self,
        chat_id: ChatId,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let state = &mut self.chat_list.drop_on_row;
        if state.generation != generation || state.hover != Some(chat_id) {
            return false;
        }
        if !cx.has_active_drag() {
            state.hover = None;
            cx.notify();
            return false;
        }
        state.idle_ticks = state.idle_ticks.map(|ticks| ticks + 1);
        if !DropOnRow::rested(state.idle_ticks) {
            return true;
        }
        // The pointer rested: the chat opens, the drag goes on.
        state.idle_ticks = None;
        self.select_listed_chat(chat_id, window, cx);
        false
    }

    /// Files dropped on the row of `chat_id`: the chat opens and the files
    /// are attached to it.
    pub(in crate::ui) fn drop_on_row(
        &mut self,
        chat_id: ChatId,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.chat_list.drop_on_row;
        let accepted = state.hover == Some(chat_id) || state.accepted;
        *state = DropOnRow::default();
        cx.notify();
        if !accepted || self.rows_selecting() {
            return;
        }
        self.select_listed_chat(chat_id, window, cx);
        // A chat that shows in a window of its own does not become this
        // window's open chat.
        if self.session().and_then(|s| s.open_chat) == Some(chat_id) {
            self.drop_files_plain(paths, cx);
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
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<ExternalPaths>, window, cx| {
                    let over = event.bounds.contains(&event.event.position);
                    let paths = event.drag(cx).paths().to_vec();
                    this.drag_over_row(chat_id, over, &paths, window, cx);
                }),
            )
            .on_drop(
                cx.listener(move |this, paths: &ExternalPaths, window, cx| {
                    cx.stop_propagation();
                    this.drop_on_row(chat_id, paths.paths(), window, cx);
                }),
            )
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

#[cfg(test)]
mod tests {
    use super::{CHOOSE_BY_DRAG, DropOnRow, TICK};

    #[test]
    fn a_pointer_rests_after_a_second_of_ticks_without_moving() {
        assert!(!DropOnRow::rested(None));
        assert!(!DropOnRow::rested(Some(0)));
        let ticks = (CHOOSE_BY_DRAG.as_millis() / TICK.as_millis()) as u32;
        assert!(!DropOnRow::rested(Some(ticks - 1)));
        assert!(DropOnRow::rested(Some(ticks)));
    }
}
