//! "Create Community" from the community picker (tdesktop
//! `CommunityIdentityBox`, then `ChooseVisibilityBox` with "Create"): a
//! photo and a name, then whether the chat that asked is Visible or
//! Hidden in it. `createCommunity` makes the community; the photo follows
//! with `setCommunityPhoto` once it exists.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;

/// tdesktop `defaultUserpicButton` size.
const USERPIC: f32 = 76.;

/// The "choose photo" userpic: a camera on the accent color.
fn camera_tile(size: f32) -> AnyElement {
    div()
        .size(px(size))
        .flex_none()
        .rounded(px(size * super::row::ROUNDING))
        .flex()
        .items_center()
        .justify_center()
        .bg(crate::ui::chat_theme::accent())
        .child(
            Icon::new(IconName::Camera)
                .size(px(size * 0.4))
                .text_color(crate::ui::chat_theme::text_on_fill()),
        )
        .into_any_element()
}

impl QuillApp {
    /// "Create Community" on the picker for `chat_id`.
    pub(super) fn open_new_community(
        &mut self,
        chat_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::COMMUNITY_CREATE_NAME)));
        self.show_community_box(
            CommunityBox::Create {
                chat_id,
                draft: NewCommunity { name, photo: None },
            },
            cx,
        );
    }

    /// The userpic button: pick an image for the new community.
    fn choose_new_community_photo(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::COMMUNITY_CREATE_TITLE).into()),
        });
        // A picker answered after an account switch sets nothing.
        let asked_on = self.session().map(|session| session.account.clone());
        self.spawn_screen(cx, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    if this.session().map(|session| &session.account) != asked_on.as_ref() {
                        return;
                    }
                    if !quill::profile_forms::is_profile_photo_file(&path) {
                        this.connection
                            .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_CHOOSE_A_JPEG_PNG_OR)));
                    } else if let Some(CommunityBox::Create { draft, .. }) =
                        this.admin.community.flow.open.as_mut()
                    {
                        draft.photo = Some(path);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Next: an empty name stays (tdesktop focuses the field and shakes
    /// it), anything else goes to the visibility choice.
    pub(super) fn submit_new_community_identity(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(CommunityBox::Create { chat_id, draft }) = &self.admin.community.flow.open else {
            return;
        };
        let (chat_id, draft) = (*chat_id, draft.clone());
        if draft.name.read(cx).value().trim().is_empty() {
            draft.name.update(cx, |name, cx| name.focus(window, cx));
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_PEOPLE_NAME_CANNOT_BE_EMPTY)));
            cx.notify();
            return;
        }
        self.show_community_box(
            CommunityBox::CreateVisibility {
                chat_id,
                draft,
                hidden: false,
            },
            cx,
        );
    }

    /// "Create" on the visibility box.
    pub(super) fn submit_new_community(&mut self, cx: &mut Context<Self>) {
        let Some(CommunityBox::CreateVisibility {
            chat_id,
            draft,
            hidden,
        }) = &self.admin.community.flow.open
        else {
            return;
        };
        let (chat_id, hidden) = (*chat_id, *hidden);
        let name = draft.name.read(cx).value().trim().to_string();
        let photo = draft
            .photo
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_CREATING_COMMUNITIES_NEEDS_A_LIVE,
            )));
            cx.notify();
            return;
        };
        match live.driver.create_community_with_photo(
            quill::ids::ChatId(chat_id),
            &name,
            hidden,
            photo,
        ) {
            // The box waits for the answer: it closes on `Created` and
            // stays on a refusal, as tdesktop's does.
            Ok(_) => {}
            Err(err) => self
                .connection
                .set_note(StatusNote::error(user_error(&err))),
        }
        cx.notify();
    }

    fn build_community_identity_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CommunityIdentity, |this, _, cx| {
                this.flush_and_close_community_box(cx)
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true).w(px(420.));
            let Some(CommunityBox::Create { chat_id, draft }) = &this.admin.community.flow.open
            else {
                return dialog.on_close(on_close);
            };
            let (chat_id, draft) = (*chat_id, draft.clone());
            let tile = match draft.photo.as_deref() {
                Some(path) => super::row::photo_tile(path, USERPIC),
                None => camera_tile(USERPIC),
            };
            let userpic = div()
                .id("community-create-photo")
                .flex_none()
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(tr(keys::COMMUNITY_CREATE_TITLE))
                .on_click(cx.listener(|this, _, _, cx| this.choose_new_community_photo(cx)))
                .child(super::row::with_cards(
                    tile,
                    USERPIC,
                    cx.theme().background,
                    cx,
                ));
            let body = div()
                .flex()
                .items_center()
                .gap_4()
                .child(userpic)
                .child(div().flex_1().min_w_0().child(Input::new(&draft.name)));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("community-create-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.show_community_box(CommunityBox::Pick { chat_id }, cx);
                        })),
                )
                .child(
                    Button::new("community-create-next")
                        .label(tr(keys::CREATE_GROUP_NEXT))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_new_community_identity(window, cx);
                        })),
                );
            dialog
                .title(crate::ui::shell::dialog_title(tr(
                    keys::COMMUNITY_CREATE_TITLE,
                )))
                .content(super::boxes::once(body.into_any_element()))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// "New Community": the photo and name of a community made from the
    /// picker.
    CommunityIdentity => DialogSpec::new(
        5418,
        |app| matches!(app.admin.community.flow.open, Some(CommunityBox::Create { .. })),
        QuillApp::build_community_identity_dialog,
    )
    .close(|app, _, cx| app.flush_and_close_community_box(cx)),
}
