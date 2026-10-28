//! The top-people strip of the empty chat search (tdesktop
//! `Dialogs::TopPeersStrip`, dialogs/ui/top_peers_strip.cpp): avatars with
//! a first name, an unread badge and an online dot, in one row that scrolls
//! sideways and expands into a grid, with the arrow keys and a per-person
//! menu. What it decides without a window is in `quill::top_people`.

use super::*;
use crate::ui::accent_strong;
use crate::ui::chat_list_keys::{NavKey, SearchRow};
use crate::ui::menu_states::ChatMenuState;
use crate::ui::search_state::TopDrag;
use gpui_kit::component::badge::Badge;
use quill::lang::{keys, tr};
use quill::top_people::{self as model, StripKey, StripMove, StripView, TopPerson};

/// What the sidebar takes from the list's width before the results: its
/// `p_3` padding on both sides (24) and the right border (1).
const LIST_CHROME: f32 = 25.;
/// `QApplication::startDragDistance`.
const START_DRAG: f32 = 4.;
/// The presence dot on a tile's avatar.
const DOT: f32 = 12.;

impl QuillApp {
    /// The width the strip lays out in: the list as laid out (narrowed
    /// beside the chat, the whole window in one column) less the sidebar's
    /// own padding.
    fn top_people_width(&self) -> f32 {
        (f32::from(self.list_width()) - LIST_CHROME).max(model::TILE_WIDTH)
    }

    /// The strip's people, its columns when expanded, and whether it is
    /// expanded: the facts the keys and the render share.
    fn top_people_layout(&self) -> Option<(Vec<ChatId>, usize, bool)> {
        // Only while the last frame drew the strip (an empty query).
        if !self
            .chat_list
            .keys
            .search_order()
            .iter()
            .any(|row| matches!(row, SearchRow::Top(_)))
        {
            return None;
        }
        let session = self.session()?;
        let people = model::top_people(session);
        if people.is_empty() {
            return None;
        }
        let width = self.top_people_width();
        let expanded =
            model::overflows(people.len(), width) && session.search.search.top_view.expanded();
        let ids = people.iter().map(|person| person.chat_id).collect();
        Some((ids, model::columns(width), expanded))
    }

    /// How far the strip is toward its expanded form: 0 collapsed, 1
    /// expanded, between while it animates.
    fn top_expand_progress(&self, expanded: bool, cx: &mut Context<Self>) -> f32 {
        let target = if expanded { 1. } else { 0. };
        let Some(started) = self.search_ui.top_toggled_at.get() else {
            return target;
        };
        let elapsed = started.elapsed().as_millis() as u64;
        if !model::expand_running(elapsed) {
            return target;
        }
        self.request_animation_tick(60, cx);
        let eased = model::expand_progress(elapsed);
        if expanded { eased } else { 1. - eased }
    }

    /// The header link: "Show more", then "Collapse" and "Show all".
    pub(super) fn toggle_top_people(&mut self, cx: &mut Context<Self>) {
        let search = if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.search.search)
        } else {
            self.demo_session.as_mut().map(|s| &mut s.search.search)
        };
        let Some(search) = search else {
            return;
        };
        search.top_view = search.top_view.toggled();
        self.search_ui.top_drag.set(None);
        let expanding = search.top_view.expanded();
        self.search_ui
            .top_toggled_at
            .set(Some(std::time::Instant::now()));
        if !expanding {
            self.search_ui.top_scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }

    /// The unread badge over an avatar: grey while the chat is muted.
    fn top_tile_badge(
        anchor: AnyElement,
        person: &TopPerson,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((count, dot)) = person.unread() else {
            return anchor;
        };
        let badge = if dot {
            Badge::new().dot()
        } else {
            Badge::new().count(count.max(0) as usize).max(99)
        };
        let color = if person.muted {
            cx.theme().muted_foreground
        } else {
            accent_strong().into()
        };
        badge.color(color).child(anchor).into_any_element()
    }

    fn top_tile(
        &self,
        person: &TopPerson,
        title: &str,
        width: f32,
        menu_target: Option<ChatId>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = person.chat_id;
        let avatar = if person.saved_messages {
            super::super::chat_row::saved_messages_avatar(model::PHOTO)
        } else {
            let photo = self.chat_photo_for_row(id);
            super::super::chat_row::chat_avatar(title, photo.as_deref(), model::PHOTO)
                .into_any_element()
        };
        let avatar = super::super::chat_row::with_presence_dot(avatar, person.online, DOT, cx)
            .into_any_element();
        let avatar = Self::top_tile_badge(avatar, person, cx);
        let tile = div()
            .id(("search-frequent", id.0 as u64))
            .a11y_tag(quill::agent::ui_driver::redact::chat_tag(id.0))
            .debug_selector(move || format!("top-tile-{}", id.0))
            .flex_none()
            .w(px(width))
            .h(px(model::TILE_HEIGHT))
            .pt(px(8.))
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .rounded_md()
            .role(gpui_kit::Role::Button)
            .aria_label(person.name.clone())
            .tab_index(0)
            .cursor_pointer()
            .pressable(cx.theme())
            .when(menu_target == Some(id), |this| {
                this.bg(cx.theme().selection)
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                // A drag that scrolled the row is not a click.
                if this
                    .search_ui
                    .top_drag
                    .take()
                    .is_some_and(|drag| drag.moved)
                {
                    return;
                }
                // Ctrl (Cmd on macOS) opens the chat in its own window
                // (`chosenRow`'s `newWindow`).
                let new_window = event.modifiers().secondary(cx)
                    && this
                        .session()
                        .and_then(|s| s.chats.get(&id.0))
                        .is_some_and(|chat| this.can_open_chat_window(chat));
                if new_window {
                    this.open_chat_in_new_window(id, window, cx);
                } else {
                    this.select_search_chat(id, window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.chat_list.menu = Some(ChatMenuState::top_person(id, event.position));
                    cx.notify();
                }),
            )
            // Press and hold peeks at the chat (`handlePressForChatPreview`).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    // A new press starts clean; the row, when the strip is
                    // collapsed, records its own drag after this.
                    this.search_ui.top_drag.set(None);
                    this.begin_chat_preview_press(id, event.position, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.chat_list.preview_press = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.chat_list.preview_press = None),
            )
            .child(avatar)
            .child(
                div()
                    .w(px(width - 12.))
                    .text_xs()
                    .text_center()
                    .truncate()
                    .child(super::super::bidi_line::one_line_plain(person.name.clone())),
            );
        self.chat_search_nav_row(tile, SearchRow::Top(id), cx)
    }

    /// tdesktop's "Frequent contacts" strip on an empty search.
    pub(super) fn frequent_contacts(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let people = model::top_people(session);
        if people.is_empty() {
            return None;
        }
        let width = self.top_people_width();
        let count = people.len();
        let overflow = model::overflows(count, width);
        let view = session.search.search.top_view;
        let expanded = overflow && view.expanded();
        let progress = self.top_expand_progress(expanded, cx);
        let columns = model::columns(width);
        let grid_rows = model::rows(count, columns);
        let height = model::TILE_HEIGHT * (1. + (grid_rows as f32 - 1.) * progress);
        let single = model::TILE_WIDTH + (width / columns as f32 - model::TILE_WIDTH) * progress;
        let menu_target = self
            .chat_list
            .menu
            .filter(|menu| menu.top_person)
            .map(|menu| menu.chat_id);
        let tiles: Vec<Stateful<Div>> = people
            .iter()
            .map(|person| {
                let title = session
                    .chats
                    .get(&person.chat_id.0)
                    .map_or("", |chat| chat.title.as_str());
                self.top_tile(person, title, single, menu_target, cx)
            })
            .collect();
        let tile_rows: Vec<SearchRow> = people
            .iter()
            .map(|person| SearchRow::Top(person.chat_id))
            .collect();
        let as_grid = expanded || progress > 0.;
        let body = if as_grid {
            let scrolled = f32::from(self.search_ui.top_scroll.offset().x).min(0.);
            div()
                .on_children_prepainted(self.chat_list.keys.tiles_bounds_recorder(tile_rows))
                .id("search-frequent-grid")
                .flex()
                .flex_wrap()
                .items_start()
                .w_full()
                .ml(px(scrolled * (1. - progress)))
                .children(tiles)
                .into_any_element()
        } else {
            div()
                .on_children_prepainted(self.chat_list.keys.tiles_bounds_recorder(tile_rows))
                .id("search-frequent-row")
                .flex()
                .items_start()
                .w_full()
                .h(px(model::TILE_HEIGHT))
                .overflow_x_scroll()
                .track_scroll(&self.search_ui.top_scroll)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _, _| {
                        this.search_ui.top_drag.set(Some(TopDrag {
                            start_x: event.position.x,
                            start_scroll: this.search_ui.top_scroll.offset().x,
                            moved: false,
                        }));
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.search_ui.top_drag.set(None)),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.search_ui.top_drag.set(None)),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    if event.pressed_button != Some(MouseButton::Left) {
                        return;
                    }
                    let Some(mut drag) = this.search_ui.top_drag.get() else {
                        return;
                    };
                    let delta = event.position.x - drag.start_x;
                    if !drag.moved && f32::from(delta).abs() >= START_DRAG {
                        drag.moved = true;
                    }
                    if drag.moved {
                        this.chat_list.preview_press = None;
                        let scroll = &this.search_ui.top_scroll;
                        let floor = -scroll.max_offset().x;
                        let x = (drag.start_scroll + delta).clamp(floor, px(0.));
                        scroll.set_offset(point(x, scroll.offset().y));
                        cx.notify();
                    }
                    this.search_ui.top_drag.set(Some(drag));
                }))
                .children(tiles)
                .into_any_element()
        };
        let link = overflow.then(|| {
            let label = match view {
                StripView::Initial => tr(keys::CHANNELS_YOUR_MORE),
                StripView::Expanded => tr(keys::RECENT_FREQUENT_COLLAPSE),
                StripView::Collapsed => tr(keys::RECENT_FREQUENT_ALL),
            };
            div().debug_selector(|| "top-toggle".to_string()).child(
                Button::new("search-frequent-toggle")
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_top_people(cx))),
            )
        });
        Some(
            div()
                .id("search-frequent")
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .child(tr(keys::Q_CHAT_FREQUENT_CONTACTS)),
                        )
                        .children(link),
                )
                .child(
                    div()
                        .id("search-frequent-strip")
                        .debug_selector(|| "top-strip".to_string())
                        .relative()
                        .w_full()
                        .h(px(height))
                        .overflow_hidden()
                        .child(body),
                )
                .into_any_element(),
        )
    }

    /// Put the keyboard selection on one tile and bring it into view.
    fn select_top_person(
        &mut self,
        id: ChatId,
        index: usize,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.chat_list
            .keys
            .set_search_keyboard(Some(SearchRow::Top(id)));
        // Scroll the results only as far as it takes to show the tile
        // (`scrollToRequests`), so a tall grid keeps its place.
        self.reveal_search_row(SearchRow::Top(id));
        if !expanded {
            let scroll = &self.search_ui.top_scroll;
            let current = -f32::from(scroll.offset().x);
            let next = model::reveal_scroll(
                current,
                f32::from(scroll.bounds().size.width),
                index,
                f32::from(scroll.max_offset().x),
            );
            scroll.set_offset(point(px(-next), scroll.offset().y));
        }
        self.notify_sidebar(cx);
    }

    /// Up or Down in the search with the strip showing
    /// (`Suggestions::selectJumpChats`). `false` leaves the key to the
    /// result list.
    pub(in crate::ui) fn nav_top_people(&mut self, key: NavKey, cx: &mut Context<Self>) -> bool {
        let Some((ids, columns, expanded)) = self.top_people_layout() else {
            return false;
        };
        let keyboard = self.chat_list.keys.search_keyboard();
        let on_tile = keyboard.and_then(|row| match row {
            SearchRow::Top(id) => ids.iter().position(|candidate| *candidate == id),
            _ => None,
        });
        let others: Vec<SearchRow> = self
            .chat_list
            .keys
            .search_order()
            .into_iter()
            .filter(|row| !matches!(row, SearchRow::Top(_)))
            .collect();
        let other_selected = self
            .chat_list
            .keys
            .search_selection()
            .is_some_and(|row| !matches!(row, SearchRow::Top(_)));
        let go = |this: &mut Self, from: Option<usize>, key: StripKey, cx: &mut Context<Self>| {
            match model::step(from, key, ids.len(), columns, expanded) {
                StripMove::Select(index) => {
                    if let Some(id) = ids.get(index) {
                        this.select_top_person(*id, index, expanded, cx);
                    }
                    true
                }
                StripMove::Leave => false,
                StripMove::Unchanged => true,
            }
        };
        match key {
            NavKey::Down => match on_tile {
                Some(index) => {
                    if !go(self, Some(index), StripKey::Down, cx)
                        && let Some(first) = others.first()
                    {
                        self.chat_list.keys.set_search_keyboard(Some(*first));
                        self.reveal_search_row(*first);
                        self.notify_sidebar(cx);
                    }
                    true
                }
                None if !other_selected => {
                    // Entering from the field shows the top of the results
                    // (`_chatsScroll->scrollTo(0)`).
                    self.search_ui
                        .results_scroll
                        .set_offset(point(px(0.), px(0.)));
                    go(self, None, StripKey::Down, cx)
                }
                None => false,
            },
            NavKey::Up => match on_tile {
                Some(index) => {
                    if !go(self, Some(index), StripKey::Up, cx) {
                        self.chat_list.keys.set_search_keyboard(None);
                        self.notify_sidebar(cx);
                    }
                    true
                }
                None => {
                    let selection = self.chat_list.keys.search_selection();
                    // A pointer resting on a tile is not a selection the
                    // keys can move from: nothing happens.
                    if matches!(selection, Some(SearchRow::Top(_))) {
                        return true;
                    }
                    if selection.is_some() && selection == others.first().copied() {
                        self.chat_list.keys.set_search_keyboard(None);
                        go(self, None, StripKey::Up, cx)
                    } else {
                        false
                    }
                }
            },
            NavKey::PageUp | NavKey::PageDown | NavKey::Home | NavKey::End => {
                if on_tile.is_some() {
                    self.chat_list.keys.set_search_keyboard(None);
                }
                false
            }
        }
    }

    /// Left or Right with the strip showing: walk the tiles unless a
    /// result below has the selection. `false` leaves the key to the field.
    pub(in crate::ui) fn sideways_top_people(
        &mut self,
        key: StripKey,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((ids, columns, expanded)) = self.top_people_layout() else {
            return false;
        };
        let on_tile = self
            .chat_list
            .keys
            .search_keyboard()
            .and_then(|row| match row {
                SearchRow::Top(id) => ids.iter().position(|candidate| *candidate == id),
                _ => None,
            });
        let other_selected = self
            .chat_list
            .keys
            .search_selection()
            .is_some_and(|row| !matches!(row, SearchRow::Top(_)));
        if other_selected {
            return false;
        }
        match model::step(on_tile, key, ids.len(), columns, expanded) {
            StripMove::Select(index) => {
                if let Some(id) = ids.get(index) {
                    self.select_top_person(*id, index, expanded, cx);
                }
                true
            }
            StripMove::Unchanged => true,
            StripMove::Leave => false,
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "top_people_tests.rs"]
mod tests;
