//! The main menu (the chat list's top-left button): a popover with a
//! filter field, a grid of icon tiles and the Night Mode switch. Which
//! tiles exist, the filter and the keyboard moves are decided in
//! `quill::main_menu`; this file draws them from `MainMenuUi`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::navigation::NavigationAction;
use super::pressable::{PressableDiv, focus_ring, hover_fill};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::main_menu::{Entry, Step, entries, matches_filter, step};

/// Tiles per grid row.
const COLUMNS: usize = 3;

pub(crate) struct MainMenuUi {
    pub(super) open: bool,
    pub(super) filter: Entity<InputState>,
    /// The tile Enter opens, moved with the arrow keys; `None` until an
    /// arrow is pressed or the filter has text.
    pub(super) highlight: Option<usize>,
}

impl MainMenuUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::DLG_FILTER)));
        cx.subscribe_in(
            &filter,
            window,
            |this, state, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let typed = !state.read(cx).value().trim().is_empty();
                    this.main_menu.highlight = typed.then_some(0);
                    cx.notify();
                }
            },
        )
        .detach();
        Self {
            open: false,
            filter,
            highlight: None,
        }
    }
}

/// The menu closes with the account it was opened on.
impl super::on_screen::Farewell for MainMenuUi {}

fn icon(entry: Entry) -> IconName {
    match entry {
        Entry::MyProfile => IconName::CircleUser,
        Entry::MyStories => IconName::CircleDashed,
        Entry::EmojiStatus => IconName::Sparkles,
        Entry::NewGroup => IconName::UserPlus,
        Entry::NewChannel => IconName::RadioTower,
        Entry::Communities => IconName::Network,
        Entry::MyGroups => IconName::Users,
        Entry::MyChannels => IconName::Megaphone,
        Entry::Downloads => IconName::Download,
        Entry::Archive => IconName::Archive,
        Entry::ArchiveToList => IconName::ArchiveRestore,
        Entry::MarkRead => IconName::CheckCheck,
        Entry::Accounts => IconName::ContactRound,
        Entry::Settings => IconName::Settings,
    }
}

fn action(entry: Entry) -> NavigationAction {
    match entry {
        Entry::MyProfile => NavigationAction::MyProfile,
        Entry::MyStories => NavigationAction::MyStories,
        Entry::EmojiStatus => NavigationAction::EmojiStatus,
        Entry::NewGroup => NavigationAction::Group,
        Entry::NewChannel => NavigationAction::Channel,
        Entry::Communities => NavigationAction::Communities,
        Entry::MyGroups => NavigationAction::MyGroups,
        Entry::MyChannels => NavigationAction::MyChannels,
        Entry::Downloads => NavigationAction::Downloads,
        Entry::Archive => NavigationAction::Archive,
        Entry::ArchiveToList => NavigationAction::ArchiveToList,
        Entry::MarkRead => NavigationAction::MarkRead,
        Entry::Accounts => NavigationAction::Accounts,
        Entry::Settings => NavigationAction::Settings,
    }
}

impl QuillApp {
    fn main_menu_label(&self, entry: Entry) -> &'static str {
        match entry {
            Entry::MyProfile => tr(keys::MENU_MY_PROFILE),
            Entry::MyStories => tr(keys::MENU_MY_STORIES),
            Entry::EmojiStatus => self.emoji_status_menu_label(),
            Entry::NewGroup => tr(keys::CREATE_GROUP_TITLE),
            Entry::NewChannel => tr(keys::CREATE_CHANNEL_TITLE),
            Entry::Communities => tr(keys::Q_NAVIGATION_COMMUNITIES),
            Entry::MyGroups => tr(keys::MENU_MY_GROUPS),
            Entry::MyChannels => tr(keys::MENU_MY_CHANNELS),
            Entry::Downloads => tr(keys::DOWNLOADS_SECTION),
            Entry::Archive => tr(keys::ARCHIVED_NAME),
            Entry::ArchiveToList => tr(keys::CONTEXT_ARCHIVE_TO_LIST),
            Entry::MarkRead => tr(keys::CONTEXT_MARK_READ_ALL),
            Entry::Accounts => tr(keys::Q_NAVIGATION_ACCOUNTS),
            Entry::Settings => tr(keys::MENU_SETTINGS),
        }
    }

    /// The tiles the filter leaves, in grid order.
    fn main_menu_tiles(&self, cx: &App) -> Vec<Entry> {
        let archive_in_menu = quill::chatlist_archive::show_in_main_menu(
            self.session().is_some_and(|s| s.has_archived_chats()),
            self.appearance.archive_in_main_menu,
        );
        let query = self.main_menu.filter.read(cx).value().to_string();
        entries(archive_in_menu)
            .into_iter()
            .filter(|e| matches_filter(self.main_menu_label(*e), &query))
            .collect()
    }

    fn set_main_menu_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.main_menu.open == open {
            return;
        }
        self.main_menu.open = open;
        self.main_menu.highlight = None;
        if open {
            self.main_menu
                .filter
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
    }

    fn activate_main_menu(&mut self, entry: Entry, window: &mut Window, cx: &mut Context<Self>) {
        self.set_main_menu_open(false, window, cx);
        self.navigate(action(entry), window, cx);
    }

    fn main_menu_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let move_to = match event.keystroke.key.as_str() {
            "left" => Some(Step::Left),
            "right" => Some(Step::Right),
            "up" => Some(Step::Up),
            "down" => Some(Step::Down),
            "enter" => None,
            _ => return,
        };
        let tiles = self.main_menu_tiles(cx);
        match move_to {
            Some(dir) => {
                self.main_menu.highlight =
                    step(self.main_menu.highlight, dir, tiles.len(), COLUMNS);
                cx.notify();
            }
            None => {
                let Some(entry) = self.main_menu.highlight.and_then(|i| tiles.get(i).copied())
                else {
                    return;
                };
                self.activate_main_menu(entry, window, cx);
            }
        }
        cx.stop_propagation();
    }

    pub(super) fn main_navigation_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity().downgrade();
        let content_view = view.clone();
        let focus = self.main_menu.filter.focus_handle(cx);
        let popover = Popover::new("main-menu-popover")
            .open(self.main_menu.open)
            .on_open_change(move |open, window, cx| {
                let open = *open;
                let _ = view.update(cx, |this, cx| this.set_main_menu_open(open, window, cx));
            })
            .track_focus(&focus)
            // The menu floats over the chat list, which shows through it.
            .bg(cx.theme().popover.opacity(0.94))
            .w(rems(20.))
            .trigger(
                Button::new("main-menu")
                    .icon(IconName::Menu)
                    .ghost()
                    .tooltip(tr(keys::MAIN_MENU))
                    .accessibility_label(tr(keys::MAIN_MENU)),
            )
            .content(move |_, window, cx| {
                content_view
                    .update(cx, |this, cx| this.main_menu_panel(window, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
            .into_any_element();
        self.with_other_accounts_dot(popover)
    }

    fn main_menu_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let tiles = self.main_menu_tiles(cx);
        let highlight = self.main_menu.highlight;
        let mut grid = div().grid().grid_cols(COLUMNS as u16).gap_1();
        for (i, entry) in tiles.iter().copied().enumerate() {
            grid = grid.child(self.main_menu_tile(entry, highlight == Some(i), cx));
        }
        let body = if tiles.is_empty() {
            div()
                .py_6()
                .flex()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_MEDIA_NO_RESULTS))
                .into_any_element()
        } else {
            grid.into_any_element()
        };
        div()
            .id("main-menu-panel")
            .flex()
            .flex_col()
            .gap_2()
            .capture_key_down(cx.listener(|this, event, window, cx| {
                this.main_menu_key(event, window, cx);
            }))
            .child(
                Input::new(&self.main_menu.filter)
                    .prefix(IconName::Search)
                    .cleanable(true),
            )
            .child(body)
            .child(div().h_px().bg(cx.theme().border))
            .child(self.night_mode_row(cx))
            .into_any_element()
    }

    fn main_menu_tile(
        &self,
        entry: Entry,
        highlighted: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.main_menu_label(entry);
        let theme = cx.theme();
        let badge = match entry {
            Entry::Accounts => self
                .other_accounts_unread()
                .map(|unread| super::background_accounts::chrome::accounts_unread_pill(unread, cx)),
            _ => None,
        };
        let ring = focus_ring(theme);
        let over = hover_fill(theme);
        div()
            .id(("main-menu-tile", entry as usize))
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .gap_1p5()
            .px_1()
            .pt_2p5()
            .pb_2()
            .rounded(theme.radius_lg)
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .pressable(theme)
            .when(highlighted, |tile| tile.bg(over).shadow(vec![ring]))
            .child(
                div()
                    .size(rems(2.5))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radius_lg)
                    .bg(theme.foreground.opacity(0.07))
                    .text_color(theme.foreground)
                    .child(Icon::new(icon(entry)).size(rems(1.25))),
            )
            .child(
                div()
                    .w_full()
                    .text_xs()
                    .text_center()
                    .line_clamp(2)
                    .text_color(theme.foreground)
                    .child(label),
            )
            .when_some(badge, |tile, badge| {
                tile.child(div().absolute().top_1().right_1().child(badge))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate_main_menu(entry, window, cx);
            }))
            .into_any_element()
    }

    /// Night Mode stays a switch under the grid: it changes a state the
    /// window shows at once, so the menu stays open to show it.
    fn night_mode_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let label = tr(keys::MENU_NIGHT_MODE);
        let theme = cx.theme();
        div()
            .id("main-menu-night-mode")
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .child(
                Icon::new(IconName::Moon)
                    .size(rems(1.))
                    .text_color(theme.muted_foreground),
            )
            .child(div().flex_1().text_sm().child(label))
            .child(
                Switch::new("main-menu-night-switch")
                    .checked(self.night_mode_on())
                    .accessibility_label(label)
                    .on_click(cx.listener(|this, _: &bool, window, cx| {
                        this.navigate(NavigationAction::NightMode, window, cx);
                    })),
            )
            .into_any_element()
    }
}
