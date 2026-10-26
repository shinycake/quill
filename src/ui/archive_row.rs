//! The "Archived chats" row of the chat list, as in Telegram Desktop
//! (`Data::Folder` fixed on top of the main list): the archive avatar, the
//! names of the newest archived chats as the preview, a muted unread badge,
//! a slim collapsed bar, and the context menu with Collapse / Move to main
//! menu. State and rules live in `quill::chatlist_archive`.
#![deny(clippy::indexing_slicing)]

use super::app::{ChatListFilter, QuillApp};
use super::chat_list_keys::ListRow;
use super::chat_row::chat_row_height;
use super::context_menu_keys::context_menu_item;
use super::pressable::PressableDiv;
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::chatlist_archive::{ArchiveRowSummary, COLLAPSED_BAR_HEIGHT};
use quill::lang::{keys, tr, tr_with};

/// `lng_archived_name`.
fn title() -> &'static str {
    tr(keys::ARCHIVED_NAME)
}

/// Avatar edge, same as a chat row's.
const AVATAR: f32 = 46.;

/// Muted unread counter (`PaintUnreadBadge` with `muted = true`).
fn muted_badge(text: String, cx: &App) -> AnyElement {
    div()
        .id("archive-unread")
        .flex_none()
        .h(px(20.))
        .min_w(px(20.))
        .px(px(6.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().muted_foreground.opacity(0.55))
        .text_color(text_on_fill())
        .text_xs()
        .font_semibold()
        .aria_label(tr_with(
            keys::Q_CHATLIST_UNREAD_CHATS,
            &[("text", &text.to_string())],
        ))
        .child(text)
        .into_any_element()
}

/// The archive userpic (`dialogsArchiveUserpic` on a grey disc).
fn archive_avatar() -> AnyElement {
    div()
        .size(px(AVATAR))
        .flex_none()
        .rounded_full()
        .bg(bg_badge_muted())
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(IconName::Archive)
                .size(px(AVATAR * 0.5))
                .text_color(text_on_fill()),
        )
        .into_any_element()
}

/// `ComposeFolderListEntryText`: newest archived chats' names, those with
/// unread messages in semibold and the full text colour, the rest muted,
/// then "and N more chats".
fn names_line(summary: &ArchiveRowSummary, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let strong = cx.theme().foreground;
    // One text run so an overlong list ends in an ellipsis; unread names
    // are highlighted ranges.
    let text = summary.text();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut at = 0;
    for name in &summary.names {
        let end = at + name.title.len();
        if name.unread {
            highlights.push((
                at..end,
                HighlightStyle {
                    color: Some(strong),
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                },
            ));
        }
        at = end + 2; // ", "
    }
    div()
        .min_w_0()
        .flex_1()
        .truncate()
        .text_xs()
        .text_color(muted)
        .child(super::bidi_line::one_line(text, highlights, Vec::new()))
        .into_any_element()
}

impl QuillApp {
    /// Show the archive: the Archived category is Quill's opened-folder
    /// view (tdesktop `openFolder`).
    pub(super) fn open_archive_folder(&mut self, cx: &mut Context<Self>) {
        self.set_chat_list_view(ChatListFilter::Archived, None);
        self.chat_list.contacts_tab_open = false;
        self.chat_list.calls_tab_open = false;
        cx.notify();
    }

    /// Context-menu action: collapse / expand the row
    /// (`setArchiveCollapsed`).
    pub(super) fn toggle_archive_collapsed(&mut self, cx: &mut Context<Self>) {
        let collapsed = !self.appearance.archive_collapsed;
        self.set_appearance(cx, |a| a.archive_collapsed = collapsed);
    }

    /// Context-menu action: move the archive between the chat list and the
    /// main menu (`setArchiveInMainMenu`).
    pub(super) fn toggle_archive_in_main_menu(&mut self, cx: &mut Context<Self>) {
        let moved = !self.appearance.archive_in_main_menu;
        self.set_appearance(cx, |a| a.archive_in_main_menu = moved);
        if moved {
            // Quill's menu is a dropdown (no right click on its entries),
            // so the toast names the way back instead.
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_CHATLIST_ARCHIVE_MOVED_TO_THE_MAIN,
            )));
        }
        // Leaving the folder view when the row it came from is gone would
        // strand nobody: the Archived tab stays available.
    }

    /// The full "Archived chats" row.
    pub(super) fn archive_row_element(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let summary = self
            .session()
            .and_then(|s| s.archive_row_summary())
            .unwrap_or_default();
        let badge = summary.badge();
        let height = chat_row_height(&[], self.appearance.preview_lines);
        div()
            .id("archive-row")
            .a11y_tag(quill::agent::ui_driver::redact::ARCHIVE)
            .w_full()
            .px_2()
            .h(height)
            .flex()
            .flex_col()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .pressable(cx.theme())
            .role(Role::Button)
            .aria_label(format!("{} — {}", title(), summary.text()))
            .bg(cx.theme().sidebar)
            .on_click(cx.listener(|this, _, _, cx| this.open_archive_folder(cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.chat_list.archive_menu = Some(event.position);
                    cx.notify();
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(archive_avatar())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .min_w_0()
                            .flex_1()
                            .child(div().font_semibold().min_w_0().truncate().child(title()))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(names_line(&summary, cx))
                                    .when_some(badge, |this, text| {
                                        this.child(muted_badge(text, cx))
                                    }),
                            ),
                    ),
            )
            .map(|row| self.chat_list_nav_row(row, ListRow::Archive, false, cx))
            .into_any_element()
    }

    /// The collapsed archive: a slim bar with the title and the badge
    /// (`PaintCollapsedRow`, `st::dialogsImportantBarHeight`).
    pub(super) fn archive_bar_element(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let badge = self
            .session()
            .and_then(|s| s.archive_row_summary())
            .and_then(|summary| summary.badge());
        div()
            .id("archive-bar")
            .w_full()
            .px_3()
            .h(px(COLLAPSED_BAR_HEIGHT))
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .pressable(cx.theme())
            .role(Role::Button)
            .aria_label(title())
            .bg(cx.theme().sidebar)
            .on_click(cx.listener(|this, _, _, cx| this.open_archive_folder(cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.chat_list.archive_menu = Some(event.position);
                    cx.notify();
                }),
            )
            .child(div().text_sm().font_semibold().child(title()))
            .when_some(badge, |this, text| this.child(muted_badge(text, cx)))
            .map(|row| self.chat_list_nav_row(row, ListRow::Archive, false, cx))
            .into_any_element()
    }

    /// Right-click menu of the archive row, in tdesktop's order
    /// (`Filler::fillArchiveActions`): Collapse / Expand (unless the
    /// archive lives in the main menu), Move to main menu / chat list,
    /// Mark all as read, then Archive settings.
    pub(super) fn archive_menu_overlay(
        &self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collapsed = self.appearance.archive_collapsed;
        let in_menu = self.appearance.archive_in_main_menu;
        let any_unread = self
            .session()
            .and_then(|s| s.archive_row_summary())
            .is_some_and(|summary| summary.unread_chats > 0);
        let item = |id: &'static str,
                    icon: IconName,
                    label: &'static str,
                    on_click: Box<dyn Fn(&mut QuillApp, &mut Context<QuillApp>)>,
                    cx: &mut Context<QuillApp>| {
            context_menu_item(
                id,
                icon,
                label,
                text_menu().into(),
                true,
                &self.frame.context_menu_items,
                cx,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.chat_list.archive_menu = None;
                on_click(this, cx);
                cx.notify();
            }))
            .into_any_element()
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        if !in_menu {
            rows.push(item(
                "archive-menu-collapse",
                IconName::ChevronsUpDown,
                if collapsed {
                    tr(keys::CONTEXT_ARCHIVE_EXPAND)
                } else {
                    tr(keys::CONTEXT_ARCHIVE_COLLAPSE)
                },
                Box::new(|this, cx| this.toggle_archive_collapsed(cx)),
                cx,
            ));
        }
        rows.push(item(
            "archive-menu-main-menu",
            IconName::Menu,
            if in_menu {
                tr(keys::CONTEXT_ARCHIVE_TO_LIST)
            } else {
                tr(keys::CONTEXT_ARCHIVE_TO_MENU)
            },
            Box::new(|this, cx| this.toggle_archive_in_main_menu(cx)),
            cx,
        ));
        if any_unread {
            rows.push(item(
                "archive-menu-read",
                IconName::CircleCheck,
                tr(keys::Q_CHATLIST_MARK_ALL_AS_READ),
                Box::new(|this, cx| this.mark_all_chats_as_read(true, cx)),
                cx,
            ));
        }
        rows.push(item(
            "archive-menu-how",
            IconName::Info,
            tr(keys::CONTEXT_ARCHIVE_HOW_DOES_IT_WORK),
            Box::new(|this, cx| this.open_archive_hint(cx)),
            cx,
        ));
        rows.push(item(
            "archive-menu-settings",
            IconName::Settings,
            tr(keys::SETTINGS_ARCHIVE_TITLE),
            Box::new(|this, cx| this.open_archive_settings(cx)),
            cx,
        ));
        let panel = div()
            .id("archive-menu-panel")
            .role(gpui_kit::Role::Group)
            // Cut with the "Archived chats" row: the archive tag sits
            // inside the menu's, so it is read first.
            .a11y_tag(quill::agent::ui_driver::redact::ARCHIVE)
            .occlude()
            .flex()
            .flex_col()
            .min_w(px(200.))
            .px_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .children(rows);
        div()
            .id("archive-menu-overlay")
            .track_focus(&self.frame.context_menu_focus)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .child(
                div()
                    .id("archive-menu-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.chat_list.archive_menu = None;
                        cx.notify();
                    })),
            )
            .child(
                anchored()
                    .position(position)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id("archive-menu-region")
                            .role(gpui_kit::Role::Group)
                            .a11y_tag(quill::agent::ui_driver::redact::REGION_MENU)
                            .child(panel),
                    ),
            )
            .focus_trap("archive-menu-focus", &self.frame.context_menu_focus)
            .into_any_element()
    }
}
