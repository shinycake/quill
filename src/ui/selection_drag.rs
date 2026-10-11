//! Drag selection over history rows, as in Telegram Desktop's
//! `HistoryInner`: a press on a row that is not on text, moved onto other
//! rows, selects (or deselects) the contiguous range between the press row
//! and the pointer; the range previews while the button is down and
//! applies on release. Near the list's edges the history autoscrolls
//! (`Ui::DraggingScrollManager`). The rules are in
//! [`quill::selection_drag`]; this module wires them to the pointer, the
//! painted row bounds and the scroller.

use super::app::QuillApp;
use super::autoscroll_ui::scroll_history_by;
use gpui_kit::base::{GlobalState, TextSelection};
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::selection_drag::{
    AUTOSCROLL_TICK_MS, DragRange, LIMIT, RowBand, apply_drag, autoscroll_step, edge_delta,
    moved_enough, row_under,
};
use std::time::Duration;

/// A left press on a history row that may become a drag selection.
#[derive(Debug, Clone)]
pub(crate) struct DragSelect {
    chat: ChatId,
    /// The row pressed, and whether it is still being sent (not
    /// selectable).
    press: MessageId,
    pending: bool,
    origin: Point<Pixels>,
    /// Selection mode: a release without a drag toggles the press row
    /// (`needItemSelectionToggle`). Outside it a click does nothing.
    click_toggles: bool,
    /// Set once the pointer moved onto other rows.
    range: Option<DragRange>,
}

impl DragSelect {
    /// The rows a drag covers right now, for the row preview.
    pub(crate) fn range(&self) -> Option<DragRange> {
        self.range
    }
}

impl QuillApp {
    /// A left press on a row (`mouseActionStart`). In selection mode a
    /// Shift+click selects the range from the last clicked row; any other
    /// press waits for a drag or a release. Outside selection mode only a
    /// press that did not land on text can start a drag.
    pub(super) fn selection_press_row(
        &mut self,
        chat: ChatId,
        message_id: MessageId,
        pending: bool,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let click_toggles = self.selecting_in(chat);
        if click_toggles {
            // The press owns the pointer: no text selection under it.
            GlobalState::suppress_text_selection(cx);
            window.focus(&self.message_ui.history_focus, cx);
            self.message_ui.selection_focus = Some(message_id);
            if event.modifiers.shift
                && let Some(anchor) = self.message_ui.selection_anchor
            {
                let ids = self.loaded_selectable_ids(chat);
                let range = quill::selection_pin::range_between(&ids, anchor, message_id);
                self.add_to_selection(chat, range);
                self.message_ui.selection_drag = None;
                cx.notify();
                return;
            }
        } else if TextSelection::has_selection(window, cx) {
            // A standing text selection: this press clears it (the text
            // layer does that) and is not the start of a message drag.
            return;
        }
        self.message_ui.selection_drag = Some(DragSelect {
            chat,
            press: message_id,
            pending,
            origin: event.position,
            click_toggles,
            range: None,
        });
        self.start_selection_drag_loop(window, cx);
    }

    /// The pointer moved with the button down (`mouseActionUpdate`): start
    /// the drag once it left the press row, follow it, and report how far
    /// the history should autoscroll this tick (pixels, positive down).
    pub(super) fn selection_drag_pointer(
        &mut self,
        pointer: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<f32> {
        let drag = self.message_ui.selection_drag.as_ref()?;
        let (chat, press, origin, click_toggles) =
            (drag.chat, drag.press, drag.origin, drag.click_toggles);
        let mut range = drag.range;
        if range.is_none() {
            if !click_toggles && TextSelection::has_selection(window, cx) {
                // The press landed on text: that drag selects text.
                self.message_ui.selection_drag = None;
                return None;
            }
            if !moved_enough(
                (f32::from(origin.x), f32::from(origin.y)),
                (f32::from(pointer.x), f32::from(pointer.y)),
            ) {
                return None;
            }
        }
        let bands = self.history_row_bands();
        let row = row_under(&bands, f32::from(pointer.y))?;
        let hit = self.history.rows.get(row)?;
        // The row's far end from the press, so a whole album row counts.
        let to = if hit.contains(press) {
            press
        } else if hit.first_id()?.0 > press.0 {
            hit.last_id()?
        } else {
            hit.first_id()?
        };
        let beyond = self.history.viewport.get().map_or(0, |view| {
            edge_delta(
                f32::from(pointer.y),
                f32::from(view.top()),
                f32::from(view.bottom()),
            )
        });
        match range.as_mut() {
            None => {
                if to == press && beyond == 0 {
                    return None;
                }
                let selected = self
                    .share
                    .pending_forward
                    .as_ref()
                    .is_some_and(|draft| draft.from_chat_id == chat && draft.contains(press));
                // Drag-selecting gives the history the keyboard
                // (`updateDragSelection` → `setFocus`).
                window.focus(&self.message_ui.history_focus, cx);
                self.message_ui.selection_focus = Some(press);
                range = Some(DragRange {
                    anchor: press,
                    to,
                    selecting: !selected,
                });
                cx.notify();
            }
            Some(range) if range.to != to => {
                range.to = to;
                cx.notify();
            }
            Some(_) => {}
        }
        if let Some(drag) = self.message_ui.selection_drag.as_mut() {
            drag.range = range;
        }
        (beyond != 0).then(|| autoscroll_step(beyond) as f32)
    }

    /// The button came up (`mouseActionFinish`): a drag applies its range;
    /// a plain click in selection mode toggles the pressed row.
    pub(super) fn selection_release(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.message_ui.selection_drag.take() else {
            return;
        };
        match drag.range {
            Some(range) => {
                let ids = self.loaded_selectable_ids(drag.chat);
                let selected: Vec<MessageId> = self
                    .share
                    .pending_forward
                    .as_ref()
                    .filter(|draft| draft.from_chat_id == drag.chat)
                    .map(|draft| draft.message_ids.clone())
                    .unwrap_or_default();
                let next = apply_drag(&ids, &selected, &range, LIMIT);
                self.set_selection(drag.chat, next, cx);
                // The release ends the drag, not a swipe to reply.
                self.message_ui.swipe_reply_start = None;
                self.message_ui.selection_anchor = Some(range.to);
                self.message_ui.selection_focus = Some(range.to);
            }
            None if drag.click_toggles => {
                self.toggle_forward_select(drag.chat, drag.press, drag.pending, cx);
                self.message_ui.selection_anchor = Some(drag.press);
                self.message_ui.selection_focus = Some(drag.press);
            }
            None => {}
        }
        cx.notify();
    }

    /// Replace the selection of `chat` with `ids` (ascending); an empty
    /// list ends selection mode the way the last toggle does.
    pub(super) fn set_selection(
        &mut self,
        chat: ChatId,
        ids: Vec<MessageId>,
        cx: &mut Context<Self>,
    ) {
        if ids.is_empty() {
            if self
                .share
                .pending_forward
                .as_ref()
                .is_some_and(|draft| draft.from_chat_id == chat)
            {
                self.share.pending_forward = None;
                self.share.forward_picker_open = false;
                self.message_ui.selection_focus = None;
            }
            self.connection.status_note = "selection cleared".into();
        } else {
            match self.share.pending_forward.as_mut() {
                Some(draft) if draft.from_chat_id == chat => draft.message_ids = ids,
                _ => {
                    if let Some(mut draft) =
                        quill::composer::ForwardDraft::from_message(chat, ids[0], false)
                    {
                        draft.message_ids = ids;
                        self.share.pending_forward = Some(draft);
                    }
                }
            }
            let count = self.share.pending_forward.as_ref().map_or(0, |d| d.count());
            self.connection.status_note = match count {
                1 => "1 message selected".into(),
                n => format!("{n} messages selected"),
            };
        }
        cx.notify();
    }

    /// The drag in progress over `chat`, if any.
    pub(super) fn selection_drag_in(&self, chat: ChatId) -> Option<DragRange> {
        self.message_ui
            .selection_drag
            .as_ref()
            .filter(|drag| drag.chat == chat)
            .and_then(DragSelect::range)
    }

    /// How a row shows: its selected state, or the drag's while it covers
    /// the row.
    pub(super) fn selection_shown(&self, chat: ChatId, message_id: MessageId) -> bool {
        let selected = self
            .share
            .pending_forward
            .as_ref()
            .is_some_and(|draft| draft.from_chat_id == chat && draft.contains(message_id));
        match self.selection_drag_in(chat) {
            Some(range) => range.shown(message_id, selected),
            None => selected,
        }
    }

    /// Selection mode shows in `chat`: messages are selected, or a drag is
    /// about to select some.
    pub(super) fn selection_visible_in(&self, chat: ChatId) -> bool {
        self.selecting_in(chat)
            || self
                .selection_drag_in(chat)
                .is_some_and(|range| range.selecting)
    }

    /// The count the selection bar shows: the selection as the drag in
    /// progress would leave it.
    pub(super) fn selection_shown_count(&self, chat: ChatId) -> usize {
        let selected: Vec<MessageId> = self
            .share
            .pending_forward
            .as_ref()
            .filter(|draft| draft.from_chat_id == chat)
            .map(|draft| draft.message_ids.clone())
            .unwrap_or_default();
        match self.selection_drag_in(chat) {
            Some(range) => {
                apply_drag(&self.loaded_selectable_ids(chat), &selected, &range, LIMIT).len()
            }
            None => selected.len(),
        }
    }

    /// Where the history rows were painted last, in window pixels.
    fn history_row_bands(&self) -> Vec<RowBand> {
        self.history
            .hit_rows
            .borrow()
            .iter()
            .map(|(index, bounds)| RowBand {
                index: *index,
                top: f32::from(bounds.top()),
                bottom: f32::from(bounds.bottom()),
            })
            .collect()
    }

    /// While a press is held the pointer is polled every 15 ms: a pointer
    /// resting beyond the list's edge keeps scrolling and extending the
    /// range even though no move events arrive. The loop ends with the
    /// drag; only one runs at a time.
    fn start_selection_drag_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.message_ui.selection_drag_loop {
            return;
        }
        self.message_ui.selection_drag_loop = true;
        let handle = window.window_handle();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(AUTOSCROLL_TICK_MS))
                    .await;
                let Ok(pointer) = cx.update(|window, _| window.mouse_position()) else {
                    break;
                };
                let step = this.update_in(cx, |this, window, cx| {
                    // `None` once the drag ended: the loop stops.
                    this.message_ui
                        .selection_drag
                        .is_some()
                        .then(|| this.selection_drag_pointer(pointer, window, cx))
                });
                let Ok(Some(step)) = step else {
                    break;
                };
                if let Some(dy) = step
                    && dy != 0.
                {
                    // The wheel event lands inside the list, where the
                    // scroller listens.
                    let anchor = this
                        .read_with(cx, |this, _| {
                            this.history.viewport.get().map(|view| {
                                point(
                                    pointer.x.max(view.left()).min(view.right() - px(1.)),
                                    pointer.y.max(view.top()).min(view.bottom() - px(1.)),
                                )
                            })
                        })
                        .ok()
                        .flatten();
                    if let Some(anchor) = anchor {
                        let _ = handle.update(cx, |_, window, cx| {
                            scroll_history_by(window, cx, anchor, pointer, dy);
                        });
                    }
                }
            }
            let _ = this.update(cx, |this, _| this.message_ui.selection_drag_loop = false);
        })
        .detach();
    }
}

/// UI integration tests: real pointer and key events against the demo
/// `QuillApp` (gpui-kit's test support needs the `demo-capture` feature).
#[cfg(all(test, feature = "demo-capture"))]
mod tests {
    use crate::ui::HistoryRow;
    use crate::ui::app::QuillApp;
    use crate::ui::keybindings::bind_keys;
    use crate::ui::screenshot_demo::ScreenshotDemo;
    use crate::ui::shell::QuillShell;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Entity, Focusable, Modifiers, MouseButton, Pixels, Point,
        TestAppContext, VisualTestContext, point, px, size,
    };
    use quill::ids::{ChatId, MessageId};

    fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
        cx.update(gpui_kit::init);
        cx.update(bind_keys);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
            let view = cx.new(|cx| {
                QuillApp::new_with_demo(
                    window,
                    cx,
                    None,
                    Some(ScreenshotDemo::named("ready-chats")),
                )
            });
            *slot_in.borrow_mut() = Some(view.clone());
            let focus = view.focus_handle(cx);
            window.focus(&focus, cx);
            let shell = cx.new(|_| QuillShell::new(view));
            Root::new(shell, window, cx)
        });
        let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        // Two frames: the scroller measures its rows on the first.
        for _ in 0..2 {
            vcx.update(|window, cx| window.render_frame(cx));
        }
        vcx.run_until_parked();
        (app, vcx)
    }

    /// A painted single-message row: its message, outgoing flag and bounds.
    struct Row {
        id: MessageId,
        outgoing: bool,
        bounds: Bounds<Pixels>,
    }

    /// The selectable single rows fully on screen, top to bottom (the
    /// scroller also paints overdraw rows outside the viewport).
    fn rows(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Vec<Row> {
        app.read_with(cx, |app, _| {
            let viewport = app.history.viewport.get().expect("the list painted");
            let mut painted: Vec<(usize, Bounds<Pixels>)> = app.history.hit_rows.borrow().clone();
            painted.sort_by(|a, b| a.1.top().cmp(&b.1.top()));
            painted
                .into_iter()
                .filter(|(_, bounds)| {
                    bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom()
                })
                .filter_map(|(ix, bounds)| match app.history.rows.get(ix)? {
                    HistoryRow::Single(inputs)
                        if inputs.message.id.0 > 0
                            && !inputs.message.pending
                            && inputs.message.can_pin() =>
                    {
                        Some(Row {
                            id: inputs.message.id,
                            outgoing: inputs.message.is_outgoing,
                            bounds,
                        })
                    }
                    _ => None,
                })
                .collect()
        })
    }

    /// A point on the row that is not on its bubble: the empty side, near
    /// the bottom (a day label or the unread divider sits at the top).
    fn blank_point(row: &Row) -> Point<Pixels> {
        let y = row.bounds.bottom() - px(10.);
        if row.outgoing {
            point(row.bounds.left() + px(6.), y)
        } else {
            point(row.bounds.right() - px(6.), y)
        }
    }

    fn open_chat(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> ChatId {
        app.read_with(cx, |app, _| app.session().and_then(|s| s.open_chat))
            .expect("the demo opens a chat")
    }

    fn selected(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Vec<MessageId> {
        app.read_with(cx, |app, _| {
            app.share
                .pending_forward
                .as_ref()
                .map(|draft| draft.message_ids.clone())
                .unwrap_or_default()
        })
    }

    fn focus(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<MessageId> {
        app.read_with(cx, |app, _| app.message_ui.selection_focus)
    }

    fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    }

    fn release(cx: &mut VisualTestContext, at: Point<Pixels>) {
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    }

    #[gpui_kit::test]
    fn dragging_over_rows_selects_them_on_release(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        let chat = open_chat(&app, vcx);
        let rows = rows(&app, vcx);
        assert!(
            rows.len() >= 3,
            "the demo paints {} selectable rows",
            rows.len()
        );
        let (a, b, c) = (&rows[0], &rows[1], &rows[2]);

        drag(vcx, blank_point(a), blank_point(c));
        // While the button is down the range previews and nothing is
        // selected yet.
        let range = app.read_with(vcx, |app, _| app.selection_drag_in(chat));
        let range = range.expect("the drag covers rows");
        assert_eq!(
            (range.anchor, range.to, range.selecting),
            (a.id, c.id, true)
        );
        assert!(app.read_with(vcx, |app, _| app.selection_shown(chat, b.id)));
        assert!(app.read_with(vcx, |app, _| app.selection_visible_in(chat)));
        assert!(selected(&app, vcx).is_empty());
        assert_eq!(
            app.read_with(vcx, |app, _| app.selection_shown_count(chat)),
            3
        );

        // Dragging back shrinks the range: row c leaves it.
        vcx.simulate_mouse_move(blank_point(b), MouseButton::Left, Modifiers::default());
        assert!(!app.read_with(vcx, |app, _| app.selection_shown(chat, c.id)));

        release(vcx, blank_point(b));
        assert_eq!(selected(&app, vcx), vec![a.id, b.id]);
        assert!(app.read_with(vcx, |app, _| app.selecting_in(chat)));
        assert!(app.read_with(vcx, |app, _| app.message_ui.selection_drag.is_none()));
        // The rows took the keyboard.
        assert!(
            vcx.update(|window, cx| { app.read(cx).message_ui.history_focus.is_focused(window) })
        );
    }

    #[gpui_kit::test]
    fn click_toggles_and_a_drag_from_a_selected_row_deselects(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        let chat = open_chat(&app, vcx);
        let rows = rows(&app, vcx);
        assert!(rows.len() >= 3);
        let (a, b, c) = (&rows[0], &rows[1], &rows[2]);

        drag(vcx, blank_point(a), blank_point(c));
        release(vcx, blank_point(c));
        assert_eq!(selected(&app, vcx), vec![a.id, b.id, c.id]);

        // A click on a row in selection mode toggles it on release; the
        // small movement of a click is not a drag.
        let at = blank_point(b);
        let near = at + point(px(3.), px(2.));
        vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
        assert_eq!(selected(&app, vcx), vec![a.id, b.id, c.id]);
        vcx.simulate_mouse_move(near, MouseButton::Left, Modifiers::default());
        release(vcx, near);
        assert_eq!(selected(&app, vcx), vec![a.id, c.id]);

        // Pressing a selected row and dragging deselects the covered rows.
        drag(vcx, blank_point(c), blank_point(a));
        let range = app.read_with(vcx, |app, _| app.selection_drag_in(chat));
        assert_eq!(range.map(|r| r.selecting), Some(false));
        assert!(!app.read_with(vcx, |app, _| app.selection_shown(chat, a.id)));
        release(vcx, blank_point(a));
        assert!(selected(&app, vcx).is_empty());
        assert!(!app.read_with(vcx, |app, _| app.selecting_in(chat)));
    }

    #[gpui_kit::test]
    fn shift_click_selects_the_range_from_the_last_click(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        let rows = rows(&app, vcx);
        assert!(rows.len() >= 3);
        let (a, b, c) = (&rows[0], &rows[1], &rows[2]);

        // Select a and b by drag; the drag ends on b, the range anchor.
        drag(vcx, blank_point(a), blank_point(b));
        release(vcx, blank_point(b));
        vcx.simulate_click(blank_point(a), Modifiers::default());
        assert_eq!(selected(&app, vcx), vec![b.id], "a click toggles a off");
        assert_eq!(
            app.read_with(vcx, |app, _| app.message_ui.selection_anchor),
            Some(a.id),
            "the last clicked row anchors the next Shift+click"
        );

        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        vcx.simulate_mouse_down(blank_point(c), MouseButton::Left, shift);
        vcx.simulate_mouse_up(blank_point(c), MouseButton::Left, shift);
        assert_eq!(selected(&app, vcx), vec![a.id, b.id, c.id]);
        // Shift+click only ever adds to the selection.
        vcx.simulate_mouse_down(blank_point(b), MouseButton::Left, shift);
        vcx.simulate_mouse_up(blank_point(b), MouseButton::Left, shift);
        assert_eq!(selected(&app, vcx), vec![a.id, b.id, c.id]);
    }

    #[gpui_kit::test]
    fn keys_move_the_focus_toggle_and_type_into_the_composer(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        let rows = rows(&app, vcx);
        assert!(rows.len() >= 3);
        let (a, b, c) = (&rows[0], &rows[1], &rows[2]);

        drag(vcx, blank_point(b), blank_point(c));
        release(vcx, blank_point(c));
        assert_eq!(selected(&app, vcx), vec![b.id, c.id]);
        assert_eq!(focus(&app, vcx), Some(c.id));

        // Space toggles the focused row (the history has the keyboard).
        vcx.simulate_keystrokes("space");
        assert_eq!(selected(&app, vcx), vec![b.id]);
        vcx.simulate_keystrokes("space");
        assert_eq!(selected(&app, vcx), vec![b.id, c.id]);

        // Up moves the focus to the older row; Shift+Up grows the range.
        vcx.simulate_keystrokes("up");
        assert_eq!(focus(&app, vcx), Some(b.id));
        vcx.simulate_keystrokes("shift-up");
        assert_eq!(focus(&app, vcx), Some(a.id));
        assert_eq!(selected(&app, vcx), vec![a.id, b.id, c.id]);
        // Shift+Down steps back and drops the row it left.
        vcx.simulate_keystrokes("shift-down");
        assert_eq!(selected(&app, vcx), vec![b.id, c.id]);

        // Typing goes to the composer instead of being swallowed.
        vcx.simulate_keystrokes("x");
        assert_eq!(
            app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string()),
            "x"
        );

        // Escape clears the selection.
        vcx.simulate_keystrokes("escape");
        assert!(selected(&app, vcx).is_empty());
    }

    #[gpui_kit::test]
    fn pointer_beyond_the_list_edge_asks_for_autoscroll(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        let rows = rows(&app, vcx);
        assert!(rows.len() >= 2);
        let (a, b) = (&rows[0], &rows[1]);
        drag(vcx, blank_point(a), blank_point(b));
        let viewport = app
            .read_with(vcx, |app, _| app.history.viewport.get())
            .expect("the list painted");
        let below = point(blank_point(b).x, viewport.bottom() + px(40.));
        let step = app.update_in(vcx, |app, window, cx| {
            app.selection_drag_pointer(below, window, cx)
        });
        assert_eq!(
            step,
            Some(7.),
            "41 px past the edge scrolls 41*3/20+1 a tick"
        );
        let above = point(blank_point(b).x, viewport.top() - px(100.));
        let step = app.update_in(vcx, |app, window, cx| {
            app.selection_drag_pointer(above, window, cx)
        });
        assert_eq!(step, Some(-16.));
        release(vcx, above);
        assert!(app.read_with(vcx, |app, _| app.message_ui.selection_drag.is_none()));
    }
}
