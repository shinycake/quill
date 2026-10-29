//! Folder tabs: the strip above the chat list and the column on its left
//! ("Tabs on the left", tdesktop `window/window_filters_menu.cpp`), with the
//! "Tabs appearance" choice (text, icons, both).
#![deny(clippy::indexing_slicing)]

use super::app::{ChatListFilter, QuillApp};
use super::chat_theme::{accent, accent_strong, text_on_fill};
use super::folder_glyphs::folder_glyph;
use super::folder_tabs_drag::folder_tab_drag;
use super::folder_tabs_fit::{FADE_WIDTH, RevealKey, fade_edges, needs_reveal};
use super::pressable::PressableDiv;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::folder_icons::{FolderTabsMode, FolderTabsView};
use quill::lang::{keys, tr, tr_with};
use quill::state::FolderBadge;

/// Column width (tdesktop `windowFiltersWidth` is 72px; the topic column
/// uses 68px with the same name width).
pub(super) const RAIL_WIDTH: f32 = 68.;
/// Narrower column when only icons show.
pub(super) const RAIL_WIDTH_ICONS: f32 = 52.;
const RAIL_NAME_WIDTH: f32 = 54.;

/// What a slot selects. The order of the slots is the order of the tabs:
/// the folders with All at its position among them (Premium can move it),
/// then the Unread and Archived category filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FolderSlotKind {
    All,
    Folder(i32),
    Unread,
    Archived,
}

pub(super) struct FolderSlot {
    pub kind: FolderSlotKind,
    pub name: String,
    pub glyph: IconName,
    /// Unread chats in this folder (tdesktop `setUnreadCount`).
    pub badge: Option<FolderBadge>,
}

/// The slots for a folder list `(id, name, icon name)` with All at
/// `main_position` among the folders (clamped to the end).
pub(super) fn build_slots(
    folders: &[(i32, String, String)],
    main_position: usize,
) -> Vec<FolderSlot> {
    let mut slots: Vec<FolderSlot> = folders
        .iter()
        .map(|(id, name, icon)| FolderSlot {
            kind: FolderSlotKind::Folder(*id),
            name: name.clone(),
            glyph: folder_glyph(icon),
            badge: None,
        })
        .collect();
    slots.insert(
        main_position.min(slots.len()),
        FolderSlot {
            kind: FolderSlotKind::All,
            name: tr(keys::FILTERS_ALL_SHORT).into(),
            glyph: folder_glyph(tr(keys::FILTERS_ALL_SHORT)),
            badge: None,
        },
    );
    slots.push(FolderSlot {
        kind: FolderSlotKind::Unread,
        name: tr(keys::FILTERS_NAME_UNREAD).into(),
        glyph: folder_glyph(tr(keys::FILTERS_NAME_UNREAD)),
        badge: None,
    });
    slots.push(FolderSlot {
        kind: FolderSlotKind::Archived,
        name: tr(keys::FILTERS_TYPE_NO_ARCHIVED).into(),
        glyph: IconName::Archive,
        badge: None,
    });
    slots
}

/// Which slot is selected for the current filter and folder tab.
pub(super) fn selected_slot(
    slots: &[FolderSlot],
    filter: ChatListFilter,
    folder_tab: Option<i32>,
) -> usize {
    let wanted = match (filter, folder_tab) {
        (ChatListFilter::Unread, _) => FolderSlotKind::Unread,
        (ChatListFilter::Archived, _) => FolderSlotKind::Archived,
        (_, Some(id)) => FolderSlotKind::Folder(id),
        _ => FolderSlotKind::All,
    };
    slots.iter().position(|s| s.kind == wanted).unwrap_or(0)
}

/// The unread-chats counter on a folder tab: accent when something unmuted
/// is unread, muted gray when every counted chat is muted.
fn folder_badge_pill(badge: FolderBadge, cx: &App) -> AnyElement {
    let bg = if badge.muted {
        cx.theme().muted_foreground.opacity(0.55)
    } else {
        Hsla::from(accent_strong())
    };
    let label = if badge.count > 999 {
        format!("{}K", badge.count / 1000)
    } else {
        badge.count.to_string()
    };
    div()
        .id(("folder-badge", badge.count as usize))
        .flex_none()
        .h(px(16.))
        .min_w(px(16.))
        .px(px(4.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .text_color(text_on_fill())
        .text_size(px(10.))
        .font_semibold()
        .aria_label(tr_with(
            keys::Q_CHATLIST_UNREAD_CHATS_2,
            &[("count", &badge.count.to_string())],
        ))
        .child(label)
        .into_any_element()
}

/// One folder tab. The kit draws an icon tab as the icon alone and names
/// it by its label in the overflow menu, so icons-only tabs keep the folder
/// name as their label. Icon plus text puts the icon in the tab's prefix
/// instead.
fn folder_tab(
    slot: &FolderSlot,
    show_icon: bool,
    show_text: bool,
    premium: bool,
    weak: &WeakEntity<QuillApp>,
    cx: &App,
) -> Tab {
    let mut tab = Tab::new().label(slot.name.clone());
    tab = match (show_icon, show_text) {
        (true, false) => tab.icon(slot.glyph).aria_label(slot.name.clone()),
        (true, true) => tab.prefix(
            Icon::new(slot.glyph)
                .size(px(14.))
                .text_color(cx.theme().muted_foreground),
        ),
        _ => tab,
    };
    if let Some(badge) = slot.badge {
        tab = tab.suffix(folder_badge_pill(badge, cx));
    }
    // Right-click: Edit / Mark as read / Remove (the All tab:
    // Mark all as read / Edit folders).
    let menu_folder = match slot.kind {
        FolderSlotKind::All => Some(None),
        FolderSlotKind::Folder(id) => Some(Some(id)),
        FolderSlotKind::Unread | FolderSlotKind::Archived => None,
    };
    if let Some(folder) = menu_folder {
        let menu_weak = weak.clone();
        tab = tab.on_mouse_down(MouseButton::Right, move |event, _, cx| {
            let position = event.position;
            let _ = menu_weak.update(cx, |this, cx| {
                this.open_folder_tab_menu(folder, position, cx)
            });
        });
    }
    folder_tab_drag(tab, slot.kind, slot.name.clone().into(), premium, weak)
}

/// The fade over an edge of the strip that hides tabs.
fn folder_tabs_fade(start: bool, color: Hsla) -> Div {
    let (from, to) = (color, color.opacity(0.));
    let (left, right) = if start { (from, to) } else { (to, from) };
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(FADE_WIDTH))
        .when(start, |this| this.left_0())
        .when(!start, |this| this.right_0())
        .bg(linear_gradient(
            90.,
            linear_color_stop(left, 0.),
            linear_color_stop(right, 1.),
        ))
}

/// The chevron beside the strip: every tab in a menu, the selected one
/// checked.
#[inline(never)]
fn folder_tabs_more(
    slots: &[FolderSlot],
    selected: usize,
    weak: &WeakEntity<QuillApp>,
) -> AnyElement {
    let menu_kinds: Vec<FolderSlotKind> = slots.iter().map(|s| s.kind).collect();
    let menu_labels: Vec<SharedString> = slots
        .iter()
        .map(|s| SharedString::from(s.name.clone()))
        .collect();
    let menu_weak = weak.clone();
    Button::new("folder-tabs-more")
        .xsmall()
        .ghost()
        .dropdown_caret(true)
        .dropdown_menu(move |mut menu, _, _| {
            menu = menu.scrollable(true);
            for (ix, label) in menu_labels.iter().enumerate() {
                let weak = menu_weak.clone();
                let kind = menu_kinds.get(ix).copied();
                menu = menu.item(
                    PopupMenuItem::new(label.clone())
                        .checked(ix == selected)
                        .on_click(move |_, _, cx| {
                            if let Some(kind) = kind {
                                let _ =
                                    weak.update(cx, |this, cx| this.select_folder_slot(kind, cx));
                            }
                        }),
                );
            }
            menu
        })
        .anchor(Anchor::TopRight)
        .into_any_element()
}

/// The gear that opens Settings > Folders.
#[inline(never)]
fn folder_manage_button(weak: WeakEntity<QuillApp>) -> AnyElement {
    Button::new("folder-manage")
        .icon(IconName::Settings)
        .small()
        .tooltip(tr(keys::Q_CHATLIST_CHAT_FOLDERS))
        .accessibility_label(tr(keys::Q_CHATLIST_MANAGE_CHAT_FOLDERS))
        .ghost()
        .on_click(move |_, _, cx| {
            let _ = weak.update(cx, |this, cx| this.open_folder_manage(cx));
        })
        .into_any_element()
}

impl QuillApp {
    pub(super) fn folder_slots(&self) -> Vec<FolderSlot> {
        let folders: Vec<(i32, String, String)> = self
            .session()
            .map(|s| {
                s.chat_list
                    .chat_folders
                    .iter()
                    .map(|f| (f.id, f.name.clone(), f.icon_name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let main_position = self.session().map_or(0, |s| s.chat_list.main_list_position);
        let mut slots = build_slots(&folders, main_position);
        if let Some(session) = self.session() {
            let include_muted = session.settings.badge_prefs.include_muted_folders;
            for slot in &mut slots {
                let pair = match slot.kind {
                    FolderSlotKind::All => session.chat_list.unread_totals.main.chats,
                    FolderSlotKind::Folder(id) => {
                        session.chat_list.folder_unread_chats.get(&id).copied()
                    }
                    FolderSlotKind::Unread | FolderSlotKind::Archived => None,
                };
                slot.badge = pair.and_then(|pair| pair.folder_badge(include_muted));
            }
        }
        slots
    }

    /// Whether the folders sit in a column left of the chat list.
    pub(super) fn folder_rail_active(&self) -> bool {
        self.appearance.folder_tabs_view == FolderTabsView::Left
            && self.pane_mode() == super::app::PaneMode::Ready
    }

    /// Width the column takes from the chat-list column (0 when the tabs
    /// are on top).
    pub(super) fn folder_rail_width(&self) -> f32 {
        if !self.folder_rail_active() {
            return 0.;
        }
        let (icon, text) = self
            .appearance
            .folder_tabs_mode
            .resolved(self.appearance.folder_tabs_view);
        if icon && !text {
            RAIL_WIDTH_ICONS
        } else {
            RAIL_WIDTH
        }
    }

    fn select_folder_slot(&mut self, kind: FolderSlotKind, cx: &mut Context<Self>) {
        match kind {
            FolderSlotKind::Unread => {
                self.set_chat_list_view(ChatListFilter::Unread, self.folders.tab);
                cx.notify();
            }
            FolderSlotKind::Archived => {
                self.set_chat_list_view(ChatListFilter::Archived, None);
                cx.notify();
            }
            FolderSlotKind::All => self.open_folder_tab(None, cx),
            FolderSlotKind::Folder(id) => self.open_folder_tab(Some(id), cx),
        }
    }

    /// kit Phase 3: folder tabs as a kit `TabBar` — `Main` plus the
    /// `updateChatFolders` folders, with the settings button as the bar's
    /// suffix and the Unread/Archived category filters as trailing tabs.
    /// Selecting a folder filters the chat list to `chatListFolder` chats
    /// and fires a single-shot `loadChats(chatListFolder)` when live
    /// (`open_folder_tab`). Slice CL2: the filters filter the loaded model,
    /// never the server query; `Archived` is global, so it leaves any
    /// folder tab. The manage entry stays always present so folders can be
    /// created even when the account has none yet. Tabs show text, icons
    /// or both per the "Tabs appearance" setting.
    ///
    /// The tab bar, the chevron menu and the gear are built in their own
    /// functions: a debug build gives every by-value builder step its own
    /// stack slot (`TabBar` is 11 KB, `Button` 7 KB), and one function
    /// building all three took 315 KB of the main thread's stack
    /// (docs/decisions/main-thread-stack.md).
    pub(super) fn folder_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let slots = self.folder_slots();
        let selected = selected_slot(&slots, self.chat_list.filter, self.folders.tab);
        let scroll = self.folders.tabs_scroll.clone();
        let key = RevealKey {
            selected,
            tab_count: slots.len(),
            strip_width: f32::from(self.list_width()) as i32,
        };
        if needs_reveal(self.folders.tabs_revealed.get(), key) {
            self.folders.tabs_revealed.set(Some(key));
            scroll.scroll_to_item(selected);
        }
        let fades = fade_edges(
            f32::from(scroll.offset().x),
            f32::from(scroll.max_offset().x),
        );
        let fade_color = cx.theme().sidebar;
        let weak = cx.weak_entity();
        // The strip clips at its own edge, so the chevron and the gear sit
        // beside it as flex_none siblings and can never overlap a tab. A
        // fade marks an edge that hides tabs; wheel and trackpad scroll it.
        div()
            .id("folder-tabs-row")
            .role(gpui_kit::Role::Group)
            .a11y_tag(quill::agent::ui_driver::redact::PART_FOLDERS)
            .flex()
            .items_center()
            .gap_0p5()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .id("folder-tabs-strip")
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                    .child(self.folder_tab_bar(&slots, selected, &scroll, cx))
                    .when(fades.start, |this| {
                        this.child(folder_tabs_fade(true, fade_color))
                    })
                    .when(fades.end, |this| {
                        this.child(folder_tabs_fade(false, fade_color))
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .child(folder_tabs_more(&slots, selected, &weak)),
            )
            .child(div().flex_none().child(folder_manage_button(weak)))
            .into_any_element()
    }

    /// The kit `TabBar` of the folder strip.
    #[inline(never)]
    fn folder_tab_bar(
        &self,
        slots: &[FolderSlot],
        selected: usize,
        scroll: &ScrollHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (show_icon, show_text) = self
            .appearance
            .folder_tabs_mode
            .resolved(self.appearance.folder_tabs_view);
        let weak = cx.weak_entity();
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        let tabs: Vec<Tab> = slots
            .iter()
            .map(|slot| folder_tab(slot, show_icon, show_text, premium, &weak, cx))
            .collect();
        let kinds: Vec<FolderSlotKind> = slots.iter().map(|s| s.kind).collect();
        TabBar::new("folder-tabs")
            .children(tabs)
            .selected_index(selected)
            .underline()
            .max_width(px(140.))
            .track_scroll(scroll)
            .w_full()
            .on_click(move |ix, _window, cx| {
                if let Some(kind) = kinds.get(*ix).copied() {
                    let _ = weak.update(cx, |this, cx| this.select_folder_slot(kind, cx));
                }
            })
            .into_any_element()
    }

    /// The Left layout's column (tdesktop `FiltersMenu`): one item per
    /// slot, the folders' icon over its name, an accent bar on the active
    /// one, and the folder settings button at the foot.
    pub(super) fn folder_rail(&self, cx: &mut Context<Self>) -> AnyElement {
        let slots = self.folder_slots();
        let (show_icon, show_text) = self
            .appearance
            .folder_tabs_mode
            .resolved(self.appearance.folder_tabs_view);
        let selected = selected_slot(&slots, self.chat_list.filter, self.folders.tab);
        let width = self.folder_rail_width();
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        let weak = cx.weak_entity();
        let mut list = div()
            .id("folder-rail-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll();
        for (ix, slot) in slots.iter().enumerate() {
            let active = ix == selected;
            let color = if active {
                Hsla::from(accent())
            } else {
                cx.theme().muted_foreground
            };
            let kind = slot.kind;
            let name = slot.name.clone();
            let menu_folder = match kind {
                FolderSlotKind::All => Some(None),
                FolderSlotKind::Folder(id) => Some(Some(id)),
                FolderSlotKind::Unread | FolderSlotKind::Archived => None,
            };
            let item = div()
                .id(("folder-rail-item", ix))
                .relative()
                .flex()
                .flex_col()
                .flex_none()
                .items_center()
                .justify_center()
                .w_full()
                .py(px(8.))
                .gap(px(4.))
                .cursor_pointer()
                .pressable(cx.theme())
                .role(Role::Tab)
                .aria_selected(active)
                .aria_label(name.clone())
                .tab_index(0)
                .on_click(cx.listener(move |this, _, _, cx| this.select_folder_slot(kind, cx)))
                .when_some(menu_folder, |this, folder| {
                    this.on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.open_folder_tab_menu(folder, event.position, cx);
                        }),
                    )
                })
                .when(show_icon, |this| {
                    this.child(Icon::new(slot.glyph).size(px(22.)).text_color(color))
                })
                .when(show_text, |this| {
                    this.child(
                        div()
                            .w(px(RAIL_NAME_WIDTH.min(width - 6.)))
                            .text_size(px(10.))
                            .line_height(px(12.))
                            .text_center()
                            .line_clamp(2)
                            .text_ellipsis()
                            .text_color(color)
                            .child(name),
                    )
                })
                .when_some(slot.badge, |this, badge| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(2.))
                            .right(px(6.))
                            .child(folder_badge_pill(badge, cx)),
                    )
                })
                .when(active, |this| {
                    this.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(8.))
                            .bottom(px(8.))
                            .w(px(4.))
                            .rounded_r(px(2.))
                            .bg(accent()),
                    )
                });
            list = list.child(
                folder_tab_drag(item, kind, slot.name.clone().into(), premium, &weak)
                    .test_support(),
            );
        }
        div()
            .id("folder-rail")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(width))
            .h_full()
            .min_h_0()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .role(Role::TabList)
            .aria_label(tr(keys::SETTINGS_SECTION_FILTERS))
            .child(list)
            .child(
                div().flex().flex_none().justify_center().py_2().child(
                    Button::new("folder-manage")
                        .icon(IconName::Settings)
                        .small()
                        .tooltip(tr(keys::Q_CHATLIST_CHAT_FOLDERS))
                        .accessibility_label(tr(keys::Q_CHATLIST_MANAGE_CHAT_FOLDERS))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| this.open_folder_manage(cx))),
                ),
            )
            .into_any_element()
    }

    /// "Tabs view" and "Tabs appearance" (tdesktop Settings > Folders).
    pub(super) fn folder_tabs_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        use gpui_kit::component::radio::{Radio, RadioGroup};
        let muted = cx.theme().muted_foreground;
        let view = self.appearance.folder_tabs_view;
        let mode = self.appearance.folder_tabs_mode;
        let heading = |text: &'static str| {
            div()
                .text_xs()
                .font_semibold()
                .text_color(muted)
                .child(text)
        };
        let views = [
            (FolderTabsView::Left, tr(keys::FILTERS_VERTICAL)),
            (FolderTabsView::Top, tr(keys::FILTERS_HORIZONTAL)),
        ];
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading(tr(keys::FILTERS_VIEW_SUBTITLE)))
            .child(
                RadioGroup::vertical("folder-tabs-view")
                    .selected_index(views.iter().position(|(v, _)| *v == view))
                    .children(views.iter().map(|(v, label)| {
                        Radio::new(("folder-tabs-view", *v as usize)).label(*label)
                    }))
                    .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                        if let Some((v, _)) = views.get(ix) {
                            let v = *v;
                            this.set_appearance(cx, |a| a.folder_tabs_view = v);
                        }
                    })),
            )
            .child(heading(tr(keys::FILTERS_TABS_SUBTITLE)))
            .child(
                RadioGroup::vertical("folder-tabs-mode")
                    .selected_index(FolderTabsMode::ALL.iter().position(|m| *m == mode))
                    .children(
                        FolderTabsMode::ALL.iter().map(|m| {
                            Radio::new(("folder-tabs-mode", *m as usize)).label(m.label())
                        }),
                    )
                    .on_click(cx.listener(|this, &ix: &usize, _, cx| {
                        if let Some(m) = FolderTabsMode::ALL.get(ix).copied() {
                            this.set_appearance(cx, |a| a.folder_tabs_mode = m);
                        }
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{FolderSlotKind, build_slots, selected_slot};
    use crate::ui::app::ChatListFilter;

    fn folders() -> Vec<(i32, String, String)> {
        vec![
            (4, "Work".into(), "Work".into()),
            (9, "News".into(), "Channels".into()),
        ]
    }

    #[test]
    fn slots_are_all_folders_unread_archived() {
        let slots = build_slots(&folders(), 0);
        let kinds: Vec<_> = slots.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![
                FolderSlotKind::All,
                FolderSlotKind::Folder(4),
                FolderSlotKind::Folder(9),
                FolderSlotKind::Unread,
                FolderSlotKind::Archived,
            ]
        );
        assert_eq!(slots[1].name, "Work");
    }

    #[test]
    fn selection_follows_filter_then_folder() {
        let slots = build_slots(&folders(), 0);
        assert_eq!(selected_slot(&slots, ChatListFilter::All, None), 0);
        assert_eq!(selected_slot(&slots, ChatListFilter::All, Some(9)), 2);
        assert_eq!(selected_slot(&slots, ChatListFilter::Unread, Some(9)), 3);
        assert_eq!(selected_slot(&slots, ChatListFilter::Archived, None), 4);
        // A folder that vanished falls back to All.
        assert_eq!(selected_slot(&slots, ChatListFilter::All, Some(77)), 0);
    }

    #[test]
    fn all_sits_at_the_main_list_position() {
        let kinds = |position| -> Vec<_> {
            build_slots(&folders(), position)
                .iter()
                .map(|s| s.kind)
                .collect()
        };
        assert_eq!(
            kinds(1),
            vec![
                FolderSlotKind::Folder(4),
                FolderSlotKind::All,
                FolderSlotKind::Folder(9),
                FolderSlotKind::Unread,
                FolderSlotKind::Archived,
            ]
        );
        assert_eq!(kinds(9).get(2), Some(&FolderSlotKind::All));
        let slots = build_slots(&folders(), 2);
        assert_eq!(selected_slot(&slots, ChatListFilter::All, None), 2);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "folder_tabs_tests.rs"]
mod indicator_tests;
