//! The Links to Chat screens (tdesktop `settings_chat_links.cpp`): the
//! list with its menu (Copy, Get QR Code, Edit, Delete), the new / edit
//! link box and a link's QR code. Rules and requests:
//! `quill::business_chat_links`.

use super::{LinkEditDraft, View};
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::business_chat_links::{ChatLink, plain_links};

const ABOUT: &str = "Give your customers short links that start a chat with you – and suggest \
the first message from them to you.";
const MESSAGE_ABOUT: &str = "Add a message that will be entered in the message field for anyone \
who starts a chat with you using this link.";
const NAME_ABOUT: &str = "Add a name for this link that only you will see.";
const COPIED: &str = "Chat link copied to clipboard.";

impl QuillApp {
    fn business_chat_links(&self) -> Option<Vec<ChatLink>> {
        self.session()
            .and_then(|s| s.settings.business_extra.chat_links.clone())
    }

    fn business_links_max(&self) -> usize {
        self.session()
            .map_or(quill::business_chat_links::LINKS_MAX, |s| {
                s.settings.business_extra.chat_links_max()
            })
    }

    fn copy_business_link(&mut self, link: &str, cx: &mut Context<Self>) {
        let full = if link.starts_with("https://") {
            link.to_string()
        } else {
            format!("https://{link}")
        };
        cx.write_to_clipboard(ClipboardItem::new_string(full));
        self.business_note(COPIED, cx);
    }

    /// "Create a Link to Chat" or Edit: the box with the message and name.
    pub(super) fn open_business_link_edit(
        &mut self,
        link: Option<ChatLink>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if link.is_none() && !self.business_premium() {
            self.open_premium(cx);
            return;
        }
        let text = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Add Preset Message")
                .auto_grow(2, 6)
                .submit_on_enter(false)
        });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Link Name (optional)"));
        if let Some(link) = &link {
            let (markup, title) = (link.markup.clone(), link.title.clone());
            text.update(cx, |input, cx| input.set_value(markup, window, cx));
            name.update(cx, |input, cx| input.set_value(title, window, cx));
        }
        self.business_go(View::LinkEdit, window, cx);
        if let Some(d) = self.business_mut() {
            d.link_edit = Some(LinkEditDraft {
                link: link.map(|l| l.link),
                text,
                name,
            });
        }
    }

    /// Save of the link box: create or edit, then back to the list
    /// ("Chat link saved." once TDLib answers).
    pub(super) fn save_business_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((link, markup, title)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.link_edit.as_ref())
            .map(|e| {
                (
                    e.link.clone(),
                    e.text.read(cx).value().to_string(),
                    e.name.read(cx).value().to_string(),
                )
            })
        else {
            return;
        };
        let sent = match (self.live.as_mut(), &link) {
            (None, _) => {
                self.business_error(Self::DEMO_LINE, cx);
                return;
            }
            (Some(live), Some(link)) => live.driver.edit_chat_link(link, &markup, &title),
            (Some(live), None) => live.driver.create_chat_link(&markup, &title),
        };
        if sent.is_err() {
            self.business_error("Could not send the link.", cx);
            return;
        }
        self.business_go(View::ChatLinks, window, cx);
        self.business_note("Chat link saved.", cx);
    }

    fn delete_business_link(&mut self, link: &str, cx: &mut Context<Self>) {
        let sent = match self.live.as_mut() {
            None => {
                self.business_error(Self::DEMO_LINE, cx);
                return;
            }
            Some(live) => live.driver.delete_chat_link(link),
        };
        if let Some(d) = self.business_mut() {
            d.confirm_delete = None;
        }
        if sent.is_err() {
            self.business_error("Could not delete the link.", cx);
        }
        cx.notify();
    }

    fn show_business_link_qr(&mut self, link: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(image) = crate::ui::auth_ui::render_qr_image(&link) else {
            return;
        };
        self.business_go(View::LinkQr, window, cx);
        if let Some(d) = self.business_mut() {
            d.link_qr = Some((link, image));
        }
    }

    fn business_link_row(&self, index: usize, link: &ChatLink, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let owner = cx.entity();
        let for_menu = link.clone();
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .py_1p5()
            .child(
                div()
                    .size(px(32.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(cx.theme().primary)
                    .child(
                        Icon::new(IconName::Link)
                            .size(px(16.))
                            .text_color(cx.theme().primary_foreground),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().truncate().child(link.name()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .child(link.text.clone()),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(muted)
                    .child(link.clicks_label()),
            )
            .child(
                Button::new(("business-link-menu", index as u64))
                    .icon(IconName::EllipsisVertical)
                    .ghost()
                    .small()
                    .accessibility_label(format!("{} options", link.name()))
                    .dropdown_menu(move |menu, _, _| {
                        let item = |label: &'static str,
                                    action: fn(
                            &mut QuillApp,
                            &ChatLink,
                            &mut Window,
                            &mut Context<QuillApp>,
                        )| {
                            let owner = owner.clone();
                            let link = for_menu.clone();
                            PopupMenuItem::new(label).on_click(move |_, window, cx| {
                                owner.update(cx, |this, cx| action(this, &link, window, cx));
                            })
                        };
                        menu.item(item("Copy", |this, link, _, cx| {
                            this.copy_business_link(&link.link, cx);
                        }))
                        .item(item("Get QR Code", |this, link, window, cx| {
                            this.show_business_link_qr(link.link.clone(), window, cx);
                        }))
                        .item(item("Edit", |this, link, window, cx| {
                            this.open_business_link_edit(Some(link.clone()), window, cx);
                        }))
                        .item(item("Delete", |this, link, _, cx| {
                            if let Some(d) = this.business_mut() {
                                d.confirm_delete = Some(link.link.clone());
                            }
                            cx.notify();
                        }))
                    }),
            )
    }

    pub(super) fn business_chat_links_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let links = self.business_chat_links();
        let confirm = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.confirm_delete.clone());
        let full = links
            .as_ref()
            .is_some_and(|l| l.len() >= self.business_links_max());
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().text_color(muted).child(ABOUT));
        if !full {
            body = body.child(
                crate::ui::pressable::action_row(
                    "business-link-create",
                    Some(IconName::Plus),
                    "Create a Link to Chat",
                    false,
                    cx,
                )
                .text_color(cx.theme().primary)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_business_link_edit(None, window, cx);
                })),
            );
        }
        if let Some(link) = confirm {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .child("Are you sure you want to delete this chat link?"),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("business-link-delete-cancel")
                                    .label("Cancel")
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(d) = this.business_mut() {
                                            d.confirm_delete = None;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("business-link-delete")
                                    .label("Delete")
                                    .danger()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_business_link(&link, cx);
                                    })),
                            ),
                    ),
            );
        }
        match links {
            None => {
                body = body.child(div().text_sm().text_color(muted).child("Loading…"));
            }
            Some(links) => {
                let mut list = div().flex().flex_col();
                for (index, link) in links.iter().enumerate() {
                    list = list.child(self.business_link_row(index, link, cx));
                }
                body = body.child(list);
            }
        }
        let (username, phone) = self
            .session()
            .and_then(|s| s.my_user_id.and_then(|me| s.users.get(&me)))
            .map(|u| (u.username.clone(), u.phone_number.clone()))
            .unwrap_or_default();
        let plain = plain_links(&username, &phone);
        if !plain.is_empty() {
            let mut footer = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(muted)
                .child("You can also use a simple link for a chat with you –");
            for (index, link) in plain.into_iter().enumerate() {
                if index > 0 {
                    footer = footer.child("or");
                }
                let copy = link.clone();
                footer = footer.child(
                    Button::new(("business-plain-link", index as u64))
                        .label(link)
                        .ghost()
                        .xsmall()
                        .tooltip("Copy")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.copy_business_link(&copy, cx);
                        })),
                );
            }
            body = body.child(footer);
        }
        body
    }

    pub(super) fn business_link_edit_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some((text, name)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.link_edit.as_ref())
            .map(|e| (e.text.clone(), e.name.clone()))
        else {
            return div();
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_xs().text_color(muted).child(MESSAGE_ABOUT))
            .child(Textarea::new(&text).aria_label("Preset message"))
            .child(Input::new(&name).aria_label("Link name"))
            .child(div().text_xs().text_color(muted).child(NAME_ABOUT))
    }

    pub(super) fn business_link_qr_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some((link, image)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.link_qr.clone())
        else {
            return div();
        };
        let copy = link.clone();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(
                div().p_2().rounded_lg().bg(gpui_kit::white()).child(
                    img(ImageSource::from(image))
                        .size(px(200.))
                        .aspect_square()
                        .object_fit(ObjectFit::Contain),
                ),
            )
            .child(
                div()
                    .text_sm()
                    .child(link.trim_start_matches("https://").to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .text_center()
                    .child("Everyone on Telegram can scan this code to contact you."),
            )
            .child(
                Button::new("business-link-qr-copy")
                    .label("Copy Link")
                    .icon(IconName::Copy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.copy_business_link(&copy, cx);
                    })),
            )
    }
}
