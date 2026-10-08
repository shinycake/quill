use crate::input::InputModeKind;
use gpui::{Context, Pixels, Point, Window};
use sum_tree::Bias;

use crate::input::{
    InputBaseState, MoveDown, MoveEnd, MoveHome, MoveLeft, MovePageDown, MovePageUp, MoveRight,
    MoveToEnd, MoveToNextWord, MoveToPreviousWord, MoveToStart, MoveUp, RopeExt as _,
    cursor::CursorSelection,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MoveDirection {
    Up,
    Down,
}

impl<M: InputModeKind> InputBaseState<M> {
    /// Compute the column anchor for the given `offset`. Wrap/fold-aware.
    pub(super) fn preferred_column_for(&self, offset: usize) -> Option<(Pixels, usize)> {
        self.preferred_column_for_with_affinity(offset, false)
    }

    /// Like [`Self::preferred_column_for`], but resolves an offset on a soft wrap
    /// boundary to the row the caret is drawn on.
    pub(super) fn preferred_column_for_with_affinity(
        &self,
        offset: usize,
        line_end_affinity: bool,
    ) -> Option<(Pixels, usize)> {
        let last_layout = self.last_layout.as_ref()?;
        let point = self.text.offset_to_point(offset);
        let line = last_layout.line(point.row)?;
        let pos = line.position_for_index(point.column, last_layout, line_end_affinity)?;
        Some((pos.x, point.column))
    }

    /// The laid-out paragraph `offset` is in, with the byte offset of its start.
    fn laid_out_paragraph(&self, offset: usize) -> Option<(&crate::input::display_map::LineLayout, usize)> {
        let last_layout = self.last_layout.as_ref()?;
        let row = self.text.offset_to_point(offset).row;
        Some((last_layout.line(row)?, self.text.line_start_offset(row)))
    }

    /// Whether the paragraph at `offset` has a right-to-left base direction, which swaps what
    /// the arrow keys mean: Left moves visually left, so logically forward.
    pub(super) fn is_rtl_paragraph_at(&self, offset: usize) -> bool {
        self.laid_out_paragraph(offset)
            .is_some_and(|(line, _)| line.is_rtl())
    }

    /// One arrow-key step from `offset`, `left` or right of it on screen.
    ///
    /// Like Qt's visual cursor navigation (Telegram Desktop's input field), a caret in a
    /// paragraph that holds right-to-left text follows the glyphs: it moves to the caret
    /// stop next to it on screen, whatever the logical order there. At the visual edge of a
    /// row it stays put, except at the logical end of the paragraph where it crosses into the
    /// neighbouring paragraph the way the text flows. Plain left-to-right paragraphs step
    /// logically, as always.
    pub(super) fn step_horizontally(&self, offset: usize, left: bool) -> usize {
        let logical = |forward: bool| {
            if forward {
                self.next_boundary(offset)
            } else {
                self.previous_boundary(offset)
            }
        };
        let Some((line, line_start)) = self.laid_out_paragraph(offset) else {
            return logical(!left);
        };
        if !line.has_bidi() {
            return logical(!left);
        }
        let column = offset - line_start;
        if let Some(column) = line.visual_step(column, left) {
            return line_start + column;
        }
        // At the visual edge of the row: cross a paragraph boundary only from the logical
        // end the arrow points at.
        let forward = left == line.is_rtl();
        let at_edge = if forward {
            column == line.len()
        } else {
            column == 0
        };
        if at_edge { logical(forward) } else { offset }
    }

    /// Where a collapsing Left (`left`) or Right press lands for a non-empty selection.
    pub(super) fn collapse_selection_target(&self, sel: &CursorSelection, left: bool) -> usize {
        if left != self.is_rtl_paragraph_at(sel.cursor_offset()) {
            sel.start
        } else {
            sel.end
        }
    }

    /// The word step for Ctrl/Alt+Left (`left`) or Right: a right-to-left paragraph reads the
    /// other way, so the word to the left is the next one logically.
    pub(super) fn step_word_horizontally(&self, offset: usize, left: bool) -> usize {
        if left != self.is_rtl_paragraph_at(offset) {
            self.previous_start_of_word_at(offset)
        } else {
            self.next_end_of_word_at(offset)
        }
    }

    /// The line-end affinity that applies to `sel`. Only the active cursor
    /// carries one; every other cursor sits at the start of its row.
    pub(super) fn line_end_affinity_for(&self, sel: &CursorSelection) -> bool {
        sel.id == self.active_selection().id && self.cursor_line_end_affinity
    }

    /// The line-end affinity for a cursor known only by its offset. Cursors never share an
    /// offset, so this is the active cursor's affinity when `offset` is where it sits.
    pub(super) fn line_end_affinity_at(&self, offset: usize) -> bool {
        offset == self.cursor() && self.cursor_line_end_affinity
    }

    /// Called after moving the cursor. Updates the active selection's
    /// `column_anchor` if we know where the cursor now is.
    pub(super) fn update_preferred_column(&mut self) {
        let anchor =
            self.preferred_column_for_with_affinity(self.cursor(), self.cursor_line_end_affinity);
        self.active_selection_mut().column_anchor = anchor;
    }

    /// Move the cursor to the given offset.
    ///
    /// The offset is the UTF-8 offset.
    ///
    /// Ensure the offset use self.next_boundary or self.previous_boundary to get the correct offset.
    pub(crate) fn move_to(
        &mut self,
        offset: usize,
        direction: Option<MoveDirection>,
        cx: &mut Context<Self>,
    ) {
        self.move_to_with_affinity(offset, direction, false, cx);
    }

    /// Like [`Self::move_to`], but also carries the caret's line-end affinity.
    ///
    /// A soft wrap boundary is one offset shared by the end of one visual line and the start of
    /// the next, so the offset alone cannot say where to draw the caret. Callers that resolved
    /// the offset from a visual position -- a click, a drag, a vertical move -- already know
    /// which of the two rows the user meant, and pass it here. Taking it in the same call as the
    /// move is what keeps the two from drifting apart.
    pub(crate) fn move_to_with_affinity(
        &mut self,
        offset: usize,
        direction: Option<MoveDirection>,
        line_end_affinity: bool,
        cx: &mut Context<Self>,
    ) {
        self.undo_manager.break_transaction_coalescing();
        self.selections.remove_all_but_active();
        let offset = self.cursor_boundary(offset, Bias::Left);
        self.cursor_line_end_affinity = line_end_affinity;
        self.set_cursor_to(offset);
        self.scroll_to(offset, direction, cx);
        self.pause_blink_cursor(cx);
        self.update_preferred_column();
        M::hide_context_menu(self, cx);
        M::clear_inline_completion(self, cx);
        cx.notify()
    }

    /// Compute the target offset when moving a cursor at `offset` vertically by
    /// `move_lines`, honoring the remembered `column_anchor`. Wrap/fold-aware.
    ///
    /// Returns the new offset together with the line-end affinity the caret
    /// should carry there.
    pub(super) fn vertical_target(
        &self,
        offset: usize,
        column_anchor: Option<(Pixels, usize)>,
        line_end_affinity: bool,
        move_lines: isize,
    ) -> (usize, bool) {
        let Some(last_layout) = &self.last_layout else {
            return (offset, line_end_affinity);
        };

        // Start from the row the caret is drawn on, not the row the raw offset falls in: on a
        // soft wrap boundary those are two different rows.
        let mut display_point = self
            .display_map
            .offset_to_wrap_display_point_with_affinity(offset, line_end_affinity);

        // Convert wrap row → display row (skips folded rows), move, then convert back
        let current_display_row = self
            .display_map
            .wrap_row_to_display_row(display_point.row)
            .unwrap_or_else(|| {
                self.display_map
                    .nearest_visible_display_row(display_point.row)
            });
        let max_display_row = self.display_map.display_row_count().saturating_sub(1);
        let target_display_row = current_display_row
            .saturating_add_signed(move_lines)
            .min(max_display_row);
        let target_wrap_row = self
            .display_map
            .display_row_to_wrap_row(target_display_row)
            .unwrap_or(display_point.row);

        display_point.row = target_wrap_row;
        display_point.column = 0;
        let mut new_offset = self.display_map.wrap_display_point_to_offset(display_point);

        let column_anchor = column_anchor
            .or_else(|| self.preferred_column_for_with_affinity(offset, line_end_affinity));
        let mut new_affinity = false;
        if let Some((preferred_x, column)) = column_anchor {
            // Get display point again to update local_row.
            let mut next_display_point = self.display_map.offset_to_wrap_display_point(new_offset);
            next_display_point.column = 0;
            let next_point = self
                .display_map
                .wrap_display_point_to_point(next_display_point);
            let line_start_offset = self.text.line_start_offset(next_point.row);

            // If in visible range, prefer to use position to get column.
            if let Some(line) = last_layout.line(next_point.row) {
                if let Some((x, line_end_affinity)) = line.closest_index_for_position(
                    Point {
                        x: preferred_x,
                        y: next_display_point.local_row * last_layout.line_height,
                    },
                    last_layout,
                ) {
                    new_offset = line_start_offset + x;
                    // Landing on a wrap boundary means the preferred column pointed past the
                    // last glyph of the target row, so the caret stays on that row.
                    new_affinity = line_end_affinity;
                }
            } else {
                // Not in visible range, use column directly.
                let max_line_len = self.text.slice_line(next_point.row).len();
                new_offset = line_start_offset + column.min(max_line_len);
            }
        }

        (new_offset, new_affinity)
    }

    /// Extend to the document edge when there is no further visual row. Plain
    /// movement retains its column there, but selection must still reach the
    /// remaining text on the first or last row.
    pub(super) fn vertical_selection_target(
        &self,
        offset: usize,
        column_anchor: Option<(Pixels, usize)>,
        line_end_affinity: bool,
        move_lines: isize,
    ) -> (usize, bool) {
        let target = self.vertical_target(offset, column_anchor, line_end_affinity, move_lines);
        if self.last_layout.is_some() {
            let row = |offset, affinity| {
                self.display_map
                    .offset_to_wrap_display_point_with_affinity(offset, affinity)
                    .row
            };
            if row(offset, line_end_affinity) == row(target.0, target.1) {
                return (if move_lines < 0 { 0 } else { self.text.len() }, false);
            }
        }
        target
    }

    /// Move every cursor through `f`, which maps each selection to a
    /// `(new_offset, column_anchor, line_end_affinity)`, collapsing each to a
    /// cursor. Overlapping cursors are merged, then the standard post-move
    /// sequence runs. Only the active cursor's affinity is kept, see
    /// [`Self::move_to_with_affinity`].
    pub(super) fn move_all_cursors(
        &mut self,
        f: impl Fn(&Self, &CursorSelection) -> (usize, Option<(Pixels, usize)>, bool),
        direction: Option<MoveDirection>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.undo_manager.break_transaction_coalescing();
        let mut active_affinity = false;
        let new_selections: Vec<CursorSelection> = self
            .selections
            .iter()
            .map(|sel| {
                let (offset, anchor, line_end_affinity) = f(self, sel);
                if sel.id == self.active_selection().id {
                    active_affinity = line_end_affinity;
                }
                let mut new_sel = *sel;
                new_sel.place_at(self.cursor_boundary(offset, Bias::Left), anchor);
                new_sel
            })
            .collect();
        self.selections.replace_all(new_selections);
        self.selections.merge_overlapping();

        self.cursor_line_end_affinity = active_affinity;
        self.scroll_to(self.cursor(), direction, cx);
        self.pause_blink_cursor(cx);
        M::hide_context_menu(self, cx);
        M::clear_inline_completion(self, cx);
        cx.notify();
    }

    /// Move every cursor vertically by `move_lines`.
    ///
    /// When `collapse` is set, a non-empty selection first collapses to just
    /// outside its start (up) or end (down) before moving, otherwise the cursor
    /// offset is used.
    fn move_vertical(
        &mut self,
        move_lines: isize,
        collapse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_single_line() {
            return;
        }
        self.pause_blink_cursor(cx);

        let direction = if move_lines < 0 {
            MoveDirection::Up
        } else {
            MoveDirection::Down
        };

        self.move_all_cursors(
            move |s, sel| {
                let (effective, anchor, affinity) = if sel.is_empty() || !collapse {
                    (
                        sel.cursor_offset(),
                        sel.column_anchor,
                        s.line_end_affinity_for(sel),
                    )
                } else if move_lines < 0 {
                    let e = s.previous_boundary(sel.start.saturating_sub(1));
                    (e, s.preferred_column_for(e), false)
                } else {
                    let e = s.next_boundary(sel.end.saturating_sub(1));
                    let affinity = s.line_end_affinity_at(e);
                    (
                        e,
                        s.preferred_column_for_with_affinity(e, affinity),
                        affinity,
                    )
                };
                let anchor =
                    anchor.or_else(|| s.preferred_column_for_with_affinity(effective, affinity));
                let (offset, affinity) = s.vertical_target(effective, anchor, affinity, move_lines);
                (offset, anchor, affinity)
            },
            Some(direction),
            window,
            cx,
        );
    }

    pub(super) fn left(&mut self, _: &MoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        // With a lone cursor at the very start there is nowhere to move.
        // Propagate the keystroke so an ancestor (e.g. a navigable command
        // palette) can act on it. This is harmless when nothing is bound there.
        // With multiple cursors the others can still move, so only the
        // single-cursor case propagates.
        // "Nowhere to move" is judged on screen: at the right edge of a right-to-left
        // paragraph the caret sits at offset 0 and Left still has somewhere to go.
        if self.selections.is_single()
            && self.active_selection().is_empty()
            && self.step_horizontally(self.cursor(), true) == self.cursor()
        {
            cx.propagate();
            return;
        }

        self.move_all_cursors(
            |s, sel| {
                let offset = if sel.is_empty() {
                    s.step_horizontally(sel.cursor_offset(), true)
                } else {
                    s.collapse_selection_target(sel, true)
                };
                (offset, s.preferred_column_for(offset), false)
            },
            None,
            window,
            cx,
        );
    }

    pub(super) fn right(&mut self, _: &MoveRight, window: &mut Window, cx: &mut Context<Self>) {
        // Mirror `left`: a lone cursor at the end of the text has nowhere to
        // move, so let the keystroke bubble to an ancestor.
        if self.selections.is_single()
            && self.active_selection().is_empty()
            && self.step_horizontally(self.cursor(), false) == self.cursor()
        {
            cx.propagate();
            return;
        }

        self.move_all_cursors(
            |s, sel| {
                let offset = if sel.is_empty() {
                    s.step_horizontally(sel.cursor_offset(), false)
                } else {
                    s.collapse_selection_target(sel, false)
                };
                (offset, s.preferred_column_for(offset), false)
            },
            None,
            window,
            cx,
        );
    }

    pub(super) fn up(&mut self, action: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if M::handle_context_menu_action(self, Box::new(action.clone()), window, cx) {
            return;
        }

        self.move_vertical(-1, true, window, cx);
    }

    pub(super) fn down(&mut self, action: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        if M::handle_context_menu_action(self, Box::new(action.clone()), window, cx) {
            return;
        }

        self.move_vertical(1, true, window, cx);
    }

    pub(super) fn page_up(&mut self, _: &MovePageUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_single_line() {
            return;
        }

        let Some(last_layout) = &self.last_layout else {
            return;
        };

        let display_lines = (self.input_bounds.size.height / last_layout.line_height) as isize;
        self.move_vertical(-display_lines, false, window, cx);
    }

    pub(super) fn page_down(
        &mut self,
        _: &MovePageDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_single_line() {
            return;
        }

        let Some(last_layout) = &self.last_layout else {
            return;
        };

        let display_lines = (self.input_bounds.size.height / last_layout.line_height) as isize;
        self.move_vertical(display_lines, false, window, cx);
    }

    pub(super) fn home(&mut self, _: &MoveHome, window: &mut Window, cx: &mut Context<Self>) {
        self.move_all_cursors(
            |s, sel| {
                let offset = s.start_of_line_at(sel.cursor_offset(), s.line_end_affinity_for(sel));
                (offset, s.preferred_column_for(offset), false)
            },
            Some(MoveDirection::Up),
            window,
            cx,
        );
    }

    pub(super) fn end(&mut self, _: &MoveEnd, window: &mut Window, cx: &mut Context<Self>) {
        self.move_all_cursors(
            |s, sel| {
                let offset = s.end_of_line_at(sel.cursor_offset(), s.line_end_affinity_for(sel));
                // The caret belongs at the end of the visual row it is on.
                (offset, s.preferred_column_for(offset), true)
            },
            Some(MoveDirection::Down),
            window,
            cx,
        );
    }

    pub(super) fn move_to_start(
        &mut self,
        _: &MoveToStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to(0, None, cx);
    }

    pub(super) fn move_to_end(&mut self, _: &MoveToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.text.len(), None, cx);
    }

    pub(super) fn move_to_previous_word(
        &mut self,
        _: &MoveToPreviousWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_all_cursors(
            |s, sel| {
                let offset = s.step_word_horizontally(sel.cursor_offset(), true);
                (offset, s.preferred_column_for(offset), false)
            },
            None,
            window,
            cx,
        );
    }

    pub(super) fn move_to_next_word(
        &mut self,
        _: &MoveToNextWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_all_cursors(
            |s, sel| {
                let offset = s.step_word_horizontally(sel.cursor_offset(), false);
                (offset, s.preferred_column_for(offset), false)
            },
            None,
            window,
            cx,
        );
    }
}
