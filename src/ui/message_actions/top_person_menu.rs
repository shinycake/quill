//! The menu of a person in the search's top-people strip. tdesktop builds
//! it with `FillEntryMenu` (dialogs/ui/dialogs_suggestions.cpp): Open in
//! new window, Send Message, Add to folder, View profile, then the two
//! removal entries. The popup frame is shared with the chat-row menu.

use super::chat_menu_submenus::{MENU_WIDTH, menu_row};
use super::*;
use crate::ui::menu_states::ChatMenuState;
use gpui_kit::assets::IconName as Lucide;
use quill::lang::{keys, tr};
use quill::state::SearchConfirm;

/// Gives a row a selector the UI tests find it by (a no-op outside them).
fn selectable(row: AnyElement, selector: &'static str) -> AnyElement {
    div()
        .debug_selector(move || selector.to_string())
        .child(row)
        .into_any_element()
}

impl QuillApp {
    /// The popup frame of a chat menu: a backdrop that closes it and the
    /// panel at `position`, kept inside the window.
    pub(super) fn chat_menu_shell(
        &self,
        position: Point<Pixels>,
        rows: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel = div()
            .id("chat-menu-panel")
            .role(gpui_kit::Role::Group)
            .a11y_tag(quill::agent::ui_driver::redact::REGION_MENU)
            .occlude()
            .flex()
            .flex_col()
            .w(px(MENU_WIDTH))
            .px_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .children(rows);
        div()
            .id("chat-menu-overlay")
            .track_focus(&self.frame.context_menu_focus)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .child(
                div()
                    .id("chat-menu-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.chat_list.menu = None;
                        cx.notify();
                    })),
            )
            .child(
                anchored()
                    .position(position)
                    .snap_to_window_with_margin(px(8.))
                    .child(panel),
            )
            .focus_trap("chat-menu-focus", &self.frame.context_menu_focus)
            .into_any_element()
    }

    pub(in crate::ui) fn top_person_menu_overlay(
        &self,
        menu: ChatMenuState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat_id = menu.chat_id;
        let Some(chat) = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .cloned()
        else {
            return div().into_any_element();
        };
        let hover = cx.theme().accent;
        let border = cx.theme().border;
        let separator = move || div().h(px(1.)).mx_2().my_1().bg(border).into_any_element();
        let mut rows: Vec<AnyElement> = Vec::new();
        if self.can_open_chat_window(&chat) {
            rows.push(selectable(
                menu_row(
                    "top-menu-new-window",
                    Some(Lucide::ExternalLink),
                    tr(keys::CONTEXT_NEW_WINDOW).to_string(),
                    false,
                    hover,
                    row_key(5),
                    &self.frame.context_menu_items,
                    cx,
                )
                .on_hover(cx.listener(|this, hovered, _, cx| {
                    this.chat_menu_row_hovered(*hovered, cx);
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.chat_list.menu = None;
                    this.open_chat_in_new_window(chat_id, window, cx);
                    // `descriptor.closeCallback`: the search closes.
                    this.close_search_ui(window, cx);
                    cx.notify();
                }))
                .into_any_element(),
                "top-menu-new-window",
            ));
            rows.push(separator());
        }
        rows.push(
            menu_row(
                "top-menu-open",
                Some(Lucide::MessageCircle),
                tr(keys::PROFILE_SEND_MESSAGE).to_string(),
                false,
                hover,
                row_key(10),
                &self.frame.context_menu_items,
                cx,
            )
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.chat_menu_row_hovered(*hovered, cx);
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.chat_list.menu = None;
                this.select_search_chat(chat_id, window, cx);
                cx.notify();
            }))
            .into_any_element(),
        );
        if let Some(row) = self.chat_menu_folder_row(menu, &chat, hover, cx) {
            rows.push(row);
        }
        // Every peer has a profile, Saved Messages included.
        rows.push(selectable(
            menu_row(
                "top-menu-profile",
                Some(Lucide::CircleUser),
                tr(keys::CONTEXT_VIEW_PROFILE).to_string(),
                false,
                hover,
                row_key(50),
                &self.frame.context_menu_items,
                cx,
            )
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.chat_menu_row_hovered(*hovered, cx);
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.chat_list.menu = None;
                if let Some(target) = this
                    .session()
                    .and_then(|session| session.chat_info_target(chat_id))
                {
                    this.open_info_panel_target(target, window, cx);
                }
                cx.notify();
            }))
            .into_any_element(),
            "top-menu-profile",
        ));
        rows.push(separator());
        rows.push(selectable(
            menu_row(
                "top-menu-remove",
                Some(Lucide::Trash),
                tr(keys::Q_CHAT_REMOVE_FROM_RECENT_2).to_string(),
                true,
                hover,
                row_key(60),
                &self.frame.context_menu_items,
                cx,
            )
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.chat_menu_row_hovered(*hovered, cx);
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.chat_list.menu = None;
                this.remove_top_chat(chat_id, cx);
            }))
            .into_any_element(),
            "top-menu-remove",
        ));
        rows.push(selectable(
            menu_row(
                "top-menu-disable",
                Some(Lucide::Ban),
                tr(keys::Q_CHAT_REMOVE_ALL_DISABLE).to_string(),
                true,
                hover,
                row_key(70),
                &self.frame.context_menu_items,
                cx,
            )
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.chat_menu_row_hovered(*hovered, cx);
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                this.chat_list.menu = None;
                this.set_search_prompt(Some(SearchConfirm::DisableTopChats), cx);
            }))
            .into_any_element(),
            "top-menu-disable",
        ));
        self.chat_menu_shell(menu.position, rows, cx)
    }
}
