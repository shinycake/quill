//! The Edit box's photo (tdesktop `UserpicButton` with `ChangeType`): the
//! chosen photo, else the chat's own; a click opens its sources menu. The
//! New Group / New Channel box uses the same button.

use super::GroupEditBox;
use crate::ui::app::QuillApp;
use crate::ui::message_text::kit_avatar_element;
use crate::ui::profile_photo_sources::{ChosenPhoto, PhotoPreview, PhotoTarget, SourceMode};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;

pub(in crate::ui) const PHOTO_PX: f32 = 76.0;

impl QuillApp {
    pub(super) fn group_edit_photo(
        &self,
        edit: &GroupEditBox,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chat_id = edit.chat_id;
        let title = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let path = self
            .session()
            .and_then(|s| s.chat_photo_path(chat_id))
            .and_then(|path| sandboxed_display_path(path, &roots));
        let fallback = kit_avatar_element(&title, path.as_deref(), px(PHOTO_PX)).into_any_element();
        self.chat_photo_button(
            "group-edit-photo",
            edit.photo.as_ref(),
            fallback,
            PhotoTarget::Chat(chat_id),
            edit.photo_note.clone(),
            true,
            cx,
        )
    }

    /// The round photo button with its change badge and sources menu, for
    /// the Edit box and the New Group / New Channel box. `fallback` shows
    /// until a source made a photo; `badge` adds the camera mark.
    pub(in crate::ui) fn chat_photo_button(
        &self,
        id: &'static str,
        chosen: Option<&ChosenPhoto>,
        fallback: AnyElement,
        target: PhotoTarget,
        note: Option<String>,
        badge: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let picture = match chosen.and_then(|photo| photo.preview.clone()) {
            Some(PhotoPreview::Image(image)) => img(ImageSource::Render(image))
                .size(px(PHOTO_PX))
                .rounded_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            Some(PhotoPreview::Sticker {
                background,
                sticker,
            }) => div()
                .size(px(PHOTO_PX))
                .relative()
                .rounded_full()
                .overflow_hidden()
                .child(
                    img(ImageSource::Render(background))
                        .absolute()
                        .size(px(PHOTO_PX))
                        .object_fit(ObjectFit::Cover),
                )
                .child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(self.sticker_visual(&sticker, PHOTO_PX * 0.6)),
                )
                .into_any_element(),
            None => fallback,
        };
        let owner = cx.entity().downgrade();
        let button = Button::new(id)
            .ghost()
            .rounded(ButtonRounded::Size(px(PHOTO_PX / 2.)))
            .size(px(PHOTO_PX))
            .p_0()
            .tooltip(tr(keys::PROFILE_SET_PHOTO_BUTTON))
            .accessibility_label(tr(keys::PROFILE_SET_PHOTO_BUTTON))
            .child(
                div()
                    .relative()
                    .size(px(PHOTO_PX))
                    .child(picture)
                    // tdesktop's change-photo badge: the photo is a button.
                    .when(badge, |picture| {
                        picture.child(
                            div()
                                .absolute()
                                .right_0()
                                .bottom_0()
                                .size(px(26.))
                                .rounded_full()
                                .bg(cx.theme().primary)
                                .border_2()
                                .border_color(cx.theme().background)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::new(IconName::Camera)
                                        .size(px(14.))
                                        .text_color(cx.theme().primary_foreground),
                                ),
                        )
                    }),
            )
            .dropdown_menu(move |menu, _, cx| {
                // Built when the menu opens, like tdesktop's: "From
                // clipboard" shows only while the clipboard holds an image.
                let pasteable = crate::ui::photo_editor_profile::clipboard_image(cx).is_some();
                let item = |label: &str,
                            icon: IconName,
                            action: fn(
                    &mut QuillApp,
                    PhotoTarget,
                    &mut Window,
                    &mut Context<QuillApp>,
                )| {
                    let owner = owner.clone();
                    PopupMenuItem::new(label.to_string()).icon(icon).on_click(
                        move |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| action(this, target, window, cx));
                        },
                    )
                };
                let menu = menu
                    .item(item(
                        tr(keys::ATTACH_PHOTO_OR_VIDEO),
                        IconName::Image,
                        |this, target, window, cx| this.choose_photo_or_video(target, window, cx),
                    ))
                    .item(item(
                        tr(keys::Q_PEOPLE_CAMERA),
                        IconName::Camera,
                        |this, _, window, cx| {
                            this.open_photo_source(SourceMode::Camera, window, cx)
                        },
                    ));
                let menu = if pasteable {
                    menu.item(item(
                        tr(keys::PROFILE_PHOTO_FROM_CLIPBOARD),
                        IconName::Copy,
                        |this, target, window, cx| {
                            this.paste_photo_from_clipboard(target, window, cx)
                        },
                    ))
                } else {
                    menu
                };
                menu.item(item(
                    tr(keys::Q_PEOPLE_USE_AN_EMOJI),
                    IconName::FaceSlightlySmiling,
                    |this, _, window, cx| this.open_photo_source(SourceMode::Emoji, window, cx),
                ))
            });
        let note = note.map(|text| div().text_xs().text_color(cx.theme().danger).child(text));
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .child(button)
            .children(note)
            .into_any_element()
    }
}
