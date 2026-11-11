//! The send button's options menu with its message effect picker, after
//! tdesktop's `SendMenu::FillSendMenu` and `AttachSendMenuEffect`
//! (`menu/menu_send.cpp`).
//!
//! A right-click on Send opens the menu (Quill's send options, the kit
//! popup menu as before). In a chat with a user, with something to send,
//! a strip of the account's reaction effects sits above it, headed "Add an
//! animated effect"; its last cell expands it into the full list (search,
//! every reaction effect, then "Effects from stickers"). Choosing an effect
//! shows the "Send with Effect" card over the menu: a sample message on the
//! chat background playing the effect, and below it the send button, or,
//! for a Premium effect without Premium, the offer to subscribe. A click on
//! the card elsewhere hides it; a right-click on its button offers sending
//! silently or on a schedule with the effect.
//!
//! The lists and the geometry are `quill::message_effects_picker`; the
//! effects themselves come from `getMessageEffect` (`load_effect_picker`).
#![deny(clippy::indexing_slicing)]

mod demo;
mod panel;
mod preview;
mod strip;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::message_effects_picker::picker_offered;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

/// The open menu, if any.
#[derive(Default)]
pub(crate) struct SendMenuUi {
    pub(super) open: Option<SendMenu>,
}

/// Where the menu's parts were painted (window coordinates), for placing
/// the expanded list and the card over them and for telling a press inside
/// them from one outside the menu.
#[derive(Default)]
pub(super) struct Areas {
    /// The strip and the menu together.
    pub(super) column: Cell<Option<Bounds<Pixels>>>,
    pub(super) strip: Cell<Option<Bounds<Pixels>>>,
    pub(super) menu: Cell<Option<Bounds<Pixels>>>,
    pub(super) panel: Cell<Option<Bounds<Pixels>>>,
    pub(super) card: Cell<Option<Bounds<Pixels>>>,
    pub(super) card_menu: Cell<Option<Bounds<Pixels>>>,
    /// Each effect cell, by effect id.
    pub(super) cells: RefCell<HashMap<i64, Bounds<Pixels>>>,
}

impl Areas {
    /// Whether `point` is on a part of the picker (not on the kit menu).
    fn holds(&self, point: Point<Pixels>) -> bool {
        [
            self.strip.get(),
            self.panel.get(),
            self.card.get(),
            self.card_menu.get(),
        ]
        .into_iter()
        .flatten()
        .any(|bounds| bounds.contains(&point))
    }
}

/// The "Send with Effect" card for one effect.
pub(super) struct Preview {
    pub(super) effect_id: i64,
    /// The chosen cell's centre: the card centres on it.
    pub(super) center: Point<Pixels>,
    pub(super) shown: Instant,
    /// Hiding since, fading out.
    pub(super) hiding: Option<Instant>,
    /// When the effect began playing (the card waits for its files).
    pub(super) playing: Cell<Option<Instant>>,
}

impl Preview {
    /// `now` is the executor's clock, so tests move the fade by advancing it.
    fn new(effect_id: i64, center: Point<Pixels>, now: Instant) -> Self {
        Self {
            effect_id,
            center,
            shown: now,
            hiding: None,
            playing: Cell::new(None),
        }
    }
}

/// The effect list under the strip: just the strip, or every effect with
/// its search field. The field, what it asks and its change subscription
/// exist only while the list is expanded.
pub(super) enum EffectList {
    Strip,
    Expanded {
        search: Entity<InputState>,
        query: String,
        _changed: Subscription,
    },
}

/// The send menu of the card's button.
pub(super) struct CardMenu {
    pub(super) position: Point<Pixels>,
    pub(super) menu: Entity<PopupMenu>,
    _dismiss: Subscription,
}

/// The "Send with Effect" card: none, or one shown (with the one it
/// replaced fading out, and its button's menu).
pub(super) enum Card {
    None,
    Shown {
        preview: Preview,
        /// The card being replaced, fading out.
        leaving: Option<Preview>,
        menu: Option<CardMenu>,
    },
}

pub(super) struct SendMenu {
    pub(super) chat: ChatId,
    pub(super) position: Point<Pixels>,
    pub(super) viewport: Size<Pixels>,
    pub(super) menu: Entity<PopupMenu>,
    _dismiss: Subscription,
    /// The effect picker is offered (fixed when the menu opens).
    pub(super) effects: bool,
    pub(super) list: EffectList,
    /// The strip cell under the pointer plays its animation.
    pub(super) hovered: Option<i64>,
    pub(super) card: Card,
    pub(super) areas: Rc<Areas>,
}

impl SendMenu {
    pub(super) fn expanded(&self) -> bool {
        matches!(self.list, EffectList::Expanded { .. })
    }

    /// The list's search field.
    pub(super) fn search(&self) -> Option<&Entity<InputState>> {
        match &self.list {
            EffectList::Expanded { search, .. } => Some(search),
            EffectList::Strip => None,
        }
    }

    /// What the search field asks for (trimmed).
    pub(super) fn query(&self) -> &str {
        match &self.list {
            EffectList::Expanded { query, .. } => query,
            EffectList::Strip => "",
        }
    }

    /// The card shown.
    pub(super) fn preview(&self) -> Option<&Preview> {
        match &self.card {
            Card::Shown { preview, .. } => Some(preview),
            Card::None => None,
        }
    }

    /// The cards to draw: the one fading out, then the one shown.
    pub(super) fn cards(&self) -> impl Iterator<Item = &Preview> {
        let (leaving, shown) = match &self.card {
            Card::Shown {
                preview, leaving, ..
            } => (leaving.as_ref(), Some(preview)),
            Card::None => (None, None),
        };
        leaving.into_iter().chain(shown)
    }

    pub(super) fn card_menu(&self) -> Option<&CardMenu> {
        match &self.card {
            Card::Shown { menu, .. } => menu.as_ref(),
            Card::None => None,
        }
    }

    /// The card's button menu closes.
    pub(super) fn drop_card_menu(&mut self) {
        if let Card::Shown { menu, .. } = &mut self.card {
            *menu = None;
        }
    }
}

impl QuillApp {
    /// Right-click on Send: open the options menu at `position`, with the
    /// effect picker when the chat and the composer allow it.
    pub(super) fn open_send_menu(
        &mut self,
        position: Point<Pixels>,
        sendable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = self.viewed_chat() else {
            return;
        };
        let private = self
            .session()
            .is_some_and(|session| session.chat_takes_message_effects(chat));
        if private && let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_effect_picker();
        }
        let reaction_effects = self.session().map_or(0, |session| {
            quill::message_effects_picker::picker_lists(
                &session.stickers.effects,
                session.premium_possible(),
                None,
            )
            .reactions
            .len()
        });
        let owner = cx.entity().downgrade();
        // Read now: the menu is built while this app is being updated, so
        // its builder must not read it (the #747 crash).
        let state = self.send_options_state();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            QuillApp::send_options_menu(owner.clone(), state, menu)
        });
        // The menu is its window's: its events act on that window's state.
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.in_window_of(window, cx, |this, window, cx| {
                this.send_menu_dismissed(window, cx)
            });
        });
        let focus = menu.focus_handle(cx);
        window.focus(&focus, cx);
        self.composer_ui.send_menu.open = Some(SendMenu {
            chat,
            position,
            viewport: window.viewport_size(),
            menu,
            _dismiss: dismiss,
            effects: picker_offered(private, sendable, reaction_effects),
            list: EffectList::Strip,
            hovered: None,
            card: Card::None,
            areas: Rc::default(),
        });
        cx.notify();
    }

    pub(super) fn close_send_menu(&mut self, cx: &mut Context<Self>) {
        if self.composer_ui.send_menu.open.take().is_some() {
            cx.notify();
        }
    }

    /// The kit menu closes on a press outside it, on Escape and after an
    /// item ran. A press on the picker's own parts keeps it open (the menu
    /// and the picker are one popup in tdesktop).
    fn send_menu_dismissed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mouse = window.mouse_position();
        let Some(open) = self.composer_ui.send_menu.open.as_ref() else {
            return;
        };
        if !open.areas.holds(mouse) {
            self.close_send_menu(cx);
            return;
        }
        // The kit gave focus back to the composer: keep the keyboard on
        // the menu (Escape closes it) unless the press went to the list's
        // search field or the card.
        let on_strip = open.areas.strip.get().is_some_and(|b| b.contains(&mouse));
        if on_strip && !open.expanded() {
            let focus = open.menu.focus_handle(cx);
            window.focus(&focus, cx);
        }
    }

    /// The strip's last cell: show every effect, with search.
    pub(super) fn expand_send_effects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH))
                .submit_on_enter(false)
        });
        let changed = cx.subscribe_in(
            &search,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = state.read(cx).value().to_string();
                    this.in_window_of(window, cx, |this, _, cx| {
                        this.send_effects_search(query, cx)
                    });
                }
            },
        );
        let focus = search.focus_handle(cx);
        let Some(open) = self.composer_ui.send_menu.open.as_mut() else {
            return;
        };
        open.hovered = None;
        open.list = EffectList::Expanded {
            search,
            query: String::new(),
            _changed: changed,
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    fn send_effects_search(&mut self, query: String, cx: &mut Context<Self>) {
        let trimmed = query.trim().to_string();
        if let Some(open) = self.composer_ui.send_menu.open.as_mut()
            && let EffectList::Expanded { query, .. } = &mut open.list
        {
            *query = trimmed.clone();
        }
        // Words in other languages and synonyms (tdesktop's keyword
        // dictionaries); the catalog's names answer at once.
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.search_keyword_emojis(&trimmed);
        }
        cx.notify();
    }

    /// An effect was chosen: show its card (the one shown fades out).
    pub(super) fn choose_send_effect(&mut self, effect_id: i64, cx: &mut Context<Self>) {
        let Some(open) = self.composer_ui.send_menu.open.as_mut() else {
            return;
        };
        let center = open
            .areas
            .cells
            .borrow()
            .get(&effect_id)
            .map(|bounds| bounds.center())
            .unwrap_or(open.position);
        let now = cx.background_executor().now();
        let next = Preview::new(effect_id, center, now);
        open.card = match std::mem::replace(&mut open.card, Card::None) {
            Card::Shown { mut preview, .. } => {
                preview.hiding.get_or_insert(now);
                Card::Shown {
                    preview: next,
                    leaving: Some(preview),
                    menu: None,
                }
            }
            Card::None => Card::Shown {
                preview: next,
                leaving: None,
                menu: None,
            },
        };
        cx.notify();
    }

    /// A click on the card outside its button.
    pub(super) fn hide_effect_preview(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.composer_ui.send_menu.open.as_mut() else {
            return;
        };
        let now = cx.background_executor().now();
        if let Card::Shown { preview, menu, .. } = &mut open.card {
            preview.hiding.get_or_insert(now);
            *menu = None;
        }
        cx.notify();
    }

    /// "Send with Effect" (and its silent variant): send the composer's
    /// message now with the effect, then close the menu.
    pub(super) fn send_with_effect(
        &mut self,
        effect_id: i64,
        silent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_send_menu(cx);
        let once = super::composer_sends::SendOnce {
            silent,
            effect: effect_id,
        };
        self.submit_from_send_button(once, window, cx);
    }

    /// The card's "Schedule": pick the time, then send with the effect.
    pub(super) fn schedule_with_effect(
        &mut self,
        effect_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_send_menu(cx);
        self.open_schedule_picker(super::scheduled::ScheduleTarget::Composer, window, cx);
        if let Some(picker) = self.chat_ui.schedule_picker.as_mut() {
            picker.send_after = true;
            picker.send_effect = effect_id;
        }
    }

    /// Right-click on the card's button: send silently or on a schedule
    /// with the effect (tdesktop `SetupMenuAndShortcuts` on `_send`, its
    /// details without effects).
    pub(super) fn open_card_menu(
        &mut self,
        effect_id: i64,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = self.schedule_kind();
        let silent = owner_action(cx, move |this, window, cx| {
            this.send_with_effect(effect_id, true, window, cx)
        });
        let schedule = owner_action(cx, move |this, window, cx| {
            this.schedule_with_effect(effect_id, window, cx)
        });
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            // Saved Messages takes reminders, never silent sends
            // (`Type::Reminder`).
            let menu = if kind == quill::schedule::ScheduleKind::Reminder {
                menu
            } else {
                let silent = silent.clone();
                menu.item(
                    PopupMenuItem::new(tr(keys::SEND_SILENT_MESSAGE))
                        .on_click(move |_, window, cx| silent(window, cx)),
                )
            };
            let schedule = schedule.clone();
            menu.item(
                PopupMenuItem::new(kind.menu_label(false))
                    .on_click(move |_, window, cx| schedule(window, cx)),
            )
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.in_window_of(window, cx, |this, _, cx| {
                if let Some(open) = this.composer_ui.send_menu.open.as_mut() {
                    open.drop_card_menu();
                    open.areas.card_menu.set(None);
                }
                cx.notify();
            });
        });
        let focus = menu.focus_handle(cx);
        window.focus(&focus, cx);
        if let Some(open) = self.composer_ui.send_menu.open.as_mut()
            && let Card::Shown { menu: slot, .. } = &mut open.card
        {
            *slot = Some(CardMenu {
                position,
                menu,
                _dismiss: dismiss,
            });
        }
        cx.notify();
    }

    /// The open menu over everything (drawn from the send button), for the
    /// chat being built.
    pub(super) fn send_menu_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.composer_ui.send_menu.open.as_ref()?;
        if self.viewed_chat() != Some(open.chat) {
            return None;
        }
        let areas = open.areas.clone();
        // The strip stays under the expanded list, which covers it.
        let strip = open.effects.then(|| self.effects_strip(open, cx)).flatten();
        let width = strip.as_ref().map(|(_, width)| px(*width));
        let menu_areas = areas.clone();
        let column = div()
            .id("send-menu-column")
            .relative()
            .flex()
            .flex_col()
            .gap_1()
            .when_some(width, |column, width| column.min_w(width))
            .children(strip.map(|(strip, _)| strip))
            .child(
                div()
                    .relative()
                    .child(open.menu.clone())
                    .child(tracker(move |bounds| menu_areas.menu.set(Some(bounds)))),
            )
            .child(tracker({
                let areas = areas.clone();
                move |bounds| areas.column.set(Some(bounds))
            }));
        let mut layer = div()
            .w(open.viewport.width)
            .h(open.viewport.height)
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .child(
                anchored()
                    .position(open.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(column),
            );
        if open.expanded() {
            layer = layer.children(self.effects_panel(open, cx));
        } else {
            areas.panel.set(None);
        }
        for preview in open.cards() {
            layer = layer.children(self.effect_card(open, preview, cx));
        }
        if open.preview().is_none() {
            areas.card.set(None);
        }
        if let Some(card_menu) = open.card_menu() {
            let menu_areas = areas.clone();
            layer = layer.child(
                anchored()
                    .position(card_menu.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .relative()
                            .child(card_menu.menu.clone())
                            .child(tracker(move |bounds| {
                                menu_areas.card_menu.set(Some(bounds))
                            })),
                    ),
            );
        }
        Some(
            deferred(anchored().position(point(px(0.), px(0.))).child(layer))
                .with_priority(gpui_base::POPUP_PRIORITY)
                .into_any_element(),
        )
    }
}

/// A listener that runs `f` on the app, for menu items built away from it.
fn owner_action(
    cx: &mut Context<QuillApp>,
    f: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
) -> Rc<dyn Fn(&mut Window, &mut App)> {
    let owner = cx.entity().downgrade();
    let f = Rc::new(f);
    Rc::new(move |window: &mut Window, cx: &mut App| {
        let f = f.clone();
        let _ = owner.update(cx, |this, cx| f(this, window, cx));
    })
}

/// An empty layer over its parent that reports the parent's painted
/// bounds.
pub(super) fn tracker(note: impl Fn(Bounds<Pixels>) + 'static) -> impl IntoElement {
    canvas(move |bounds, _, _| note(bounds), |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
}
