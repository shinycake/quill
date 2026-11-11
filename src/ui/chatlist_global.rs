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
/// The open "Clear calls" box: whether "Delete for everyone" is ticked.
#[derive(Clone, Copy, Default)]
pub(super) struct ClearCallsBox {
    pub revoke: bool,
}

/// The pointer on the contacts index bar: away, over it (the magnifier
/// follows `cursor`, relative to the bar's top-left) or held down on it,
/// scrubbing to `slot`. One value, so every way off the bar is one reset.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum IndexBar {
    #[default]
    Away,
    Over {
        cursor: (f32, f32),
    },
    Scrubbing {
        cursor: (f32, f32),
        /// The letter slot jumped to last; `None` until the pointer is on one.
        slot: Option<usize>,
    },
}

impl IndexBar {
    pub(super) fn cursor(self) -> Option<(f32, f32)> {
        match self {
            Self::Away => None,
            Self::Over { cursor } | Self::Scrubbing { cursor, .. } => Some(cursor),
        }
    }

    /// The slot being scrubbed.
    pub(super) fn slot(self) -> Option<usize> {
        match self {
            Self::Scrubbing { slot, .. } => slot,
            _ => None,
        }
    }

    /// The pointer moved to `cursor`; `held` is the left button.
    pub(super) fn moved(self, cursor: (f32, f32), held: bool) -> Self {
        if held {
            Self::Scrubbing {
                cursor,
                slot: self.slot(),
            }
        } else {
            Self::Over { cursor }
        }
    }

    /// The scrub reached `slot`.
    pub(super) fn scrubbed_to(self, slot: usize) -> Self {
        match self.cursor() {
            Some(cursor) => Self::Scrubbing {
                cursor,
                slot: Some(slot),
            },
            None => self,
        }
    }

    /// The button went up (or the search text changed): the pointer stays
    /// over the bar, the scrub ends.
    pub(super) fn released(self) -> Self {
        match self {
            Self::Scrubbing { cursor, .. } => Self::Over { cursor },
            other => other,
        }
    }
}

pub(super) struct ChatlistGlobal {
    pub contacts_sort: SortMode,
    pub contacts_search: Entity<TextareaState>,
    pub contacts_scroll: ScrollHandle,
    /// The pointer on the index bar.
    pub index_bar: IndexBar,
    /// The "Clear calls" confirm box and its "Delete for everyone" box.
    /// `Some` while the box is open; closing drops its checkbox with it.
    pub clear_calls: Option<ClearCallsBox>,
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
                    let bar = this.chat_list.global.index_bar;
                    this.chat_list.global.index_bar = bar.released();
                    this.notify_sidebar(cx);
                }
            },
        )
        .detach();
        Self {
            contacts_sort: SortMode::default(),
            contacts_search,
            contacts_scroll: ScrollHandle::new(),
            index_bar: IndexBar::Away,
            clear_calls: None,
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
                                this.chat_list.global.index_bar = IndexBar::Away;
                                cx.notify();
                            })),
                    ),
            )
            .child(contacts_action(
                "contacts-add",
                IconName::UserPlus,
                tr(keys::Q_PEOPLE_ADD_CONTACT),
                cx.listener(|this, _, window, cx| this.open_new_contact_dialog(window, cx)),
                cx,
            ))
            .child(contacts_action(
                "contacts-invite",
                IconName::Share2,
                tr(keys::Q_CHATLIST_INVITE_FRIENDS),
                cx.listener(|this, _, _, cx| this.copy_invitation(cx)),
                cx,
            ));
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
        self.set_note(StatusNote::info(tr(
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
        let cursor = self.chat_list.global.index_bar.cursor();
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
                self.chat_list.global.index_bar.slot() == Some(i) || visible.contains(&slot.letter);
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
                let bar = this.chat_list.global.index_bar;
                this.chat_list.global.index_bar = bar.released();
                this.notify_sidebar(cx);
            }),
        )
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            if !*hovered {
                this.chat_list.global.index_bar = IndexBar::Away;
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
        let bar = self.chat_list.global.index_bar.moved((x, y), held);
        self.chat_list.global.index_bar = bar;
        if held {
            let layout = index::bar_layout(stops, height);
            if let Some(slot) = index::slot_at(&layout, y)
                && bar.slot() != Some(slot)
                && let Some(stop) = layout.slots.get(slot).and_then(|s| stops.get(s.source))
            {
                self.chat_list.global.index_bar = bar.scrubbed_to(slot);
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

/// An action at the top of the Contacts tab (tdesktop's "Add contact"
/// and invite rows): an accent icon and label on a full-width ghost
/// button, lined up with the contact rows below.
fn contacts_action(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Button {
    let primary = cx.theme().primary;
    Button::new(id)
        .ghost()
        .w_full()
        .h(px(40.))
        .accessibility_label(label)
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .w_full()
                .text_color(primary)
                .child(
                    div()
                        .w(px(42.))
                        .flex()
                        .justify_center()
                        .child(Icon::new(icon).size_5().text_color(primary)),
                )
                .child(div().font_medium().child(label)),
        )
        .on_click(on_click)
}

#[cfg(test)]
mod tests {
    use super::IndexBar;

    #[test]
    fn a_press_scrubs_and_a_release_leaves_the_pointer_over_the_bar() {
        let bar = IndexBar::Away.moved((1., 2.), true).scrubbed_to(4);
        assert_eq!(bar.slot(), Some(4));
        assert_eq!(bar.cursor(), Some((1., 2.)));
        let held = bar.moved((1., 9.), true);
        assert_eq!(held.slot(), Some(4), "the last slot stays while held");
        let up = held.released();
        assert_eq!(up.slot(), None);
        assert_eq!(up.cursor(), Some((1., 9.)));
    }

    #[test]
    fn moving_without_the_button_ends_a_scrub() {
        let bar = IndexBar::Away.moved((0., 0.), true).scrubbed_to(1);
        assert_eq!(
            bar.moved((0., 3.), false),
            IndexBar::Over { cursor: (0., 3.) }
        );
    }

    #[test]
    fn scrubbing_needs_a_cursor() {
        assert_eq!(IndexBar::Away.scrubbed_to(2), IndexBar::Away);
    }
}
