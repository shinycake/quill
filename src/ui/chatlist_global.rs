//! Contacts tab chrome: search field, sort toggle, Invite friends, section
//! headers and the alphabetical index bar (tdesktop `ContactsBoxController`
//! and `PeerListSectionIndex`; the ordering and bar maths are in
//! `quill::contacts_index`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::contacts_index::{
    self as index, BAR_APPROACH, BAR_WIDTH, ListItem, SortMode, fisheye_scale, section_stops,
};
use quill::lang::{keys, tr};

/// State of the contacts tab and the other chat-list additions of the
/// "chatlist-global" work.
pub(super) struct ChatlistGlobal {
    pub contacts_sort: SortMode,
    pub contacts_search: Entity<TextareaState>,
    pub contacts_scroll: ScrollHandle,
    /// Pointer over the index bar, relative to the bar's top-left.
    pub index_cursor: Option<(f32, f32)>,
    /// Slot being scrubbed (pointer held down on the bar).
    pub index_current: Option<usize>,
    /// The "Clear calls" confirm box and its "Delete for everyone" box.
    pub clear_calls_open: bool,
    pub clear_calls_revoke: bool,
    /// Right-click menu of a stories-strip tile: chat id and pointer.
    pub story_menu: Option<(i64, Point<Pixels>)>,
}

impl ChatlistGlobal {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let contacts_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_CHATLIST_SEARCH_CONTACTS))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        cx.subscribe_in(
            &contacts_search,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.chat_list.global.index_current = None;
                    this.notify_sidebar(cx);
                }
            },
        )
        .detach();
        Self {
            contacts_sort: SortMode::default(),
            contacts_search,
            contacts_scroll: ScrollHandle::new(),
            index_cursor: None,
            index_current: None,
            clear_calls_open: false,
            clear_calls_revoke: false,
            story_menu: None,
        }
    }
}

impl QuillApp {
    /// The whole Contacts tab: toolbar, list and index bar.
    pub(super) fn contacts_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let sort = self.chat_list.global.contacts_sort;
        let query = self
            .chat_list
            .global
            .contacts_search
            .read(cx)
            .value()
            .trim()
            .to_string();
        let items = self.contact_items(&query);
        let stops = section_stops(&items);
        let total: f32 = items.iter().map(index::item_height).sum();
        let toolbar = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div().flex_1().child(
                            Textarea::new(&self.chat_list.global.contacts_search)
                                .aria_label(tr(keys::Q_CHATLIST_SEARCH_CONTACTS))
                                .h(px(40.)),
                        ),
                    )
                    .child(
                        Button::new("contacts-sort")
                            .icon(match sort {
                                SortMode::Online => IconName::ArrowDownAZ,
                                SortMode::Alphabet => IconName::Clock,
                            })
                            .ghost()
                            .tooltip(sort.switch_label())
                            .accessibility_label(sort.switch_label())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.chat_list.global.contacts_sort =
                                    this.chat_list.global.contacts_sort.toggled();
                                this.chat_list
                                    .global
                                    .contacts_scroll
                                    .set_offset(point(px(0.), px(0.)));
                                this.chat_list.global.index_cursor = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                Button::new("contacts-invite")
                    .icon(IconName::UserPlus)
                    .label(tr(keys::Q_CHATLIST_INVITE_FRIENDS))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.copy_invitation(cx))),
            );
        let mut scroll = div()
            .id("contacts-scroll")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.chat_list.global.contacts_scroll)
            .on_scroll(cx.listener(|this, _, _, cx| this.notify_sidebar(cx)))
            .child(self.contacts_list(items, cx));
        scroll = scroll.pr(px(if index::index_shown(&stops) {
            BAR_WIDTH
        } else {
            0.
        }));
        let bar = index::index_shown(&stops).then(|| self.index_bar(&stops, total, cx));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_1()
            .child(toolbar)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(scroll)
                    .children(bar),
            )
            .into_any_element()
    }

    /// The ordered, filtered and sectioned contacts.
    fn contact_items(&self, query: &str) -> Vec<ListItem> {
        let rows = self.session().map(|s| s.contact_rows()).unwrap_or_default();
        index::arrange(&rows, self.chat_list.global.contacts_sort, query)
    }

    fn copy_invitation(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(index::INVITE_TEXT.to_string()));
        self.connection.set_note(StatusNote::info(tr(
            keys::Q_CHATLIST_INVITATION_COPIED_TO_THE_CLIPBOARD,
        )));
        cx.notify();
    }

    /// The letters column on the right edge of the list.
    fn index_bar(
        &self,
        stops: &[index::SectionStop],
        total: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let handle = &self.chat_list.global.contacts_scroll;
        let bounds = handle.bounds();
        let height = f32::from(bounds.size.height);
        let layout = index::bar_layout(stops, height);
        let scroll = -f32::from(handle.offset().y);
        let visible = index::visible_letters(stops, total, scroll, height);
        let width = BAR_WIDTH + BAR_APPROACH;
        let cursor = self.chat_list.global.index_cursor;
        let scales: Vec<f32> = layout
            .slots
            .iter()
            .map(|slot| fisheye_scale(slot.y, layout.pitch, cursor, width))
            .collect();
        let centers = index::slot_centers(&layout, &scales, height, cursor.is_some());
        let theme = cx.theme();
        let (muted, active) = (theme.muted_foreground, theme.primary);
        let mut bar = div()
            .id("contacts-index")
            .absolute()
            .top_0()
            .right_0()
            .h_full()
            .w(px(width))
            .cursor_pointer()
            .role(gpui_kit::Role::Group)
            .aria_label(tr(keys::Q_CHATLIST_ALPHABET_INDEX));
        for (i, slot) in layout.slots.iter().enumerate() {
            let (Some(&center), Some(&scale)) = (centers.get(i), scales.get(i)) else {
                continue;
            };
            let on =
                self.chat_list.global.index_current == Some(i) || visible.contains(&slot.letter);
            bar = bar.child(
                div()
                    .absolute()
                    .right_0()
                    .w(px(BAR_WIDTH))
                    .top(px(center - 10.))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_semibold()
                    .text_size(px(11. * scale))
                    .text_color(if on { active } else { muted })
                    .child(slot.letter.to_string()),
            );
        }
        let stops_down = stops.to_vec();
        let stops_move = stops.to_vec();
        bar.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                this.index_pointer(&stops_down, total, event.position, true, cx);
            }),
        )
        .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
            let held = event.pressed_button == Some(MouseButton::Left);
            this.index_pointer(&stops_move, total, event.position, held, cx);
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.chat_list.global.index_current = None;
                this.notify_sidebar(cx);
            }),
        )
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            if !*hovered {
                this.chat_list.global.index_cursor = None;
                this.chat_list.global.index_current = None;
                this.notify_sidebar(cx);
            }
        }))
        .into_any_element()
    }

    /// Pointer over the bar: track it for the magnifier and, while held,
    /// scroll to the nearest letter's section.
    fn index_pointer(
        &mut self,
        stops: &[index::SectionStop],
        total: f32,
        position: Point<Pixels>,
        held: bool,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.chat_list.global.contacts_scroll.bounds();
        let height = f32::from(bounds.size.height);
        let left = f32::from(bounds.right()) - (BAR_WIDTH + BAR_APPROACH);
        let x = f32::from(position.x) - left;
        let y = f32::from(position.y - bounds.origin.y);
        self.chat_list.global.index_cursor = Some((x, y));
        if held {
            let layout = index::bar_layout(stops, height);
            if let Some(slot) = index::slot_at(&layout, y)
                && self.chat_list.global.index_current != Some(slot)
                && let Some(stop) = layout.slots.get(slot).and_then(|s| stops.get(s.source))
            {
                self.chat_list.global.index_current = Some(slot);
                let offset = index::jump_offset(stop, total, height);
                self.chat_list
                    .global
                    .contacts_scroll
                    .set_offset(point(px(0.), px(-offset)));
            }
        }
        self.notify_sidebar(cx);
    }
}
