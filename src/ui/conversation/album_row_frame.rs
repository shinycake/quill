//! The frame around an album row (photos and videos, or files): what a
//! single message's row has around its bubble, for the album as one
//! message (tdesktop's `GroupedMedia` view): the selection check and tint,
//! the context menu, drag selection, swipe to reply and the quick action.
//! A press on one tile picks that message alone (`album_select`).

use super::*;

/// The album row a frame is for.
pub(super) struct AlbumFrame {
    pub(super) chat: ChatId,
    /// The album's messages, ascending.
    pub(super) ids: Vec<MessageId>,
    /// The bubble sits on the right (`hasRightLayout`).
    pub(super) right: bool,
    /// The wide layout's check position (`selection_check_right`).
    pub(super) wide_right: Option<f32>,
}

impl QuillApp {
    /// Where the wide layout draws the selection check of `message`'s row,
    /// from the history's left edge (`chat_wide::selection_right`).
    pub(super) fn selection_check_right(
        &self,
        message: &HistoryMessage,
        look: &BubbleLook,
        cx: &App,
    ) -> f32 {
        let limit = crate::ui::history::bubble_width::message_width_limit(
            cx,
            &message.content,
            look.font,
            look.plain,
        )
        .map_or(quill::bubble_layout::MSG_MAX_WIDTH as f32, f32::from);
        let width = self
            .history
            .viewport
            .get()
            .map_or(f32::MAX, |view| f32::from(view.full.size.width));
        quill::chat_wide::selection_right(limit, width)
    }

    pub(super) fn album_row_frame(
        &self,
        frame: AlbumFrame,
        row: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let AlbumFrame {
            chat,
            ids,
            right,
            wide_right,
        } = frame;
        let Some(&first) = ids.first() else {
            return row;
        };
        let selection = self.selection_row(chat, &ids, wide_right, cx);
        let menu_ids = ids.clone();
        div()
            .relative()
            .rounded_md()
            .when(selection.tint > 0., |this| {
                this.bg(cx.theme().selection.opacity(selection.tint))
            })
            .when_some(self.menu_target_tint(chat, first, cx), |this, tint| {
                this.bg(tint)
            })
            // Right-click: over a tile the menu is that message's, elsewhere
            // the album's (`asGroup`, see `MenuScope`).
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let message_id = this
                        .history
                        .tile_hits
                        .at(event.position)
                        .filter(|tile| menu_ids.contains(tile))
                        .unwrap_or(first);
                    this.open_message_menu(
                        MessageMenuState {
                            chat_id: chat,
                            message_id,
                            position: event.position,
                        },
                        window,
                        cx,
                    );
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.chat_ui
                        .swipe_reply
                        .press(chat, first, event.position.x);
                    // In selection mode the overlay takes the press.
                    if !this.selecting_in(chat) {
                        this.selection_press_row(chat, first, event, window, cx);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    if this
                        .chat_ui
                        .swipe_reply
                        .release_on(chat, first, event.position.x)
                    {
                        this.begin_reply_from_message(chat, first, window, cx);
                    }
                    if event.click_count == 2
                        && !this.selecting_in(chat)
                        && !gpui_kit::base::TextSelection::has_selection(window, cx)
                    {
                        this.double_click_action(chat, first, window, cx);
                    }
                    cx.notify();
                }),
            )
            // Selecting slides an outgoing album aside for the check.
            .child(if right && selection.slide > 0. {
                div()
                    .relative()
                    .right(px(selection.slide))
                    .child(row)
                    .into_any_element()
            } else {
                row
            })
            .children(selection.overlay)
            .into_any_element()
    }
}
