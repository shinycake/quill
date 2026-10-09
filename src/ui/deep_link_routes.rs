//! UI side of the typed deep links (`parity:deeplink-internal-link-type`):
//! what happens once `getInternalLinkType` has been routed
//! (`deep_link_types::route`). The share chooser, the sticker set preview
//! hand-off, the settings pages and the proxy confirmation.
use super::app::QuillApp;
use super::navigation::NavigationAction;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::deep_link_types::{DeepLinkUi, SettingsTarget};
use quill::ids::{ChatId, MessageId};
use quill::state::DeepLinkAction;
use std::cell::RefCell;
use std::rc::Rc;

/// How many chats the share chooser lists.
const SHARE_CHOICES: usize = 14;
/// Polls (about 8 per second) to wait for a linked message to load before
/// giving up on its media timestamp.
const SEEK_POLLS: u32 = 100;

/// The navigation a settings link opens, `None` for the settings list.
fn settings_action(target: SettingsTarget) -> Option<NavigationAction> {
    match target {
        SettingsTarget::Appearance => Some(NavigationAction::Appearance),
        SettingsTarget::ChatFolders => Some(NavigationAction::ChatFolders),
        SettingsTarget::DataAndStorage => Some(NavigationAction::Storage),
        SettingsTarget::Devices => Some(NavigationAction::Sessions),
        SettingsTarget::EditProfile => Some(NavigationAction::Profile),
        SettingsTarget::Notifications => Some(NavigationAction::Notifications),
        SettingsTarget::PrivacyAndSecurity => Some(NavigationAction::Privacy),
        SettingsTarget::Contacts => Some(NavigationAction::ContactsSettings),
        SettingsTarget::Calls => Some(NavigationAction::CallSettings),
        SettingsTarget::NewGroup => Some(NavigationAction::Group),
        SettingsTarget::NewChannel => Some(NavigationAction::Channel),
        SettingsTarget::SavedMessages => Some(NavigationAction::Saved),
        SettingsTarget::Root | SettingsTarget::Unsupported => None,
    }
}

impl QuillApp {
    /// Render-time half of a `DeepLinkState::Ui` route (needs the `Window`).
    pub(super) fn run_deep_link_ui(
        &mut self,
        ui: DeepLinkUi,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ui {
            DeepLinkUi::StickerSet { set_id } => self.view_message_sticker_set(set_id, cx),
            DeepLinkUi::Share { text } => {
                self.share_link_text = Some(text);
                self.status_note = "choose a chat to share to".into();
            }
            DeepLinkUi::Proxy { link } => {
                if !self.handle_proxy_link(&link, cx) {
                    self.deep_link_dialog = Some("This proxy link is not valid.".into());
                }
            }
            DeepLinkUi::FolderInvite { link } => self.open_folder_invite(link, cx),
            DeepLinkUi::Background { name } => self.open_background_link(name, cx),
            DeepLinkUi::Settings(target) => {
                match settings_action(target) {
                    Some(action) => self.navigate(action, window, cx),
                    None => self.settings_open = true,
                }
                if target == SettingsTarget::Unsupported {
                    self.status_note = "that settings page isn't supported by Quill yet".into();
                }
            }
        }
        cx.notify();
    }

    /// Chat picked in the share chooser: open it with the text in the
    /// composer. Never sent; the user confirms (tdesktop `shareUrl`).
    fn choose_share_chat(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(text) = self.share_link_text.take() {
            self.pending_deep_link_open = Some((chat_id, DeepLinkAction::ShareDraft { text }));
        }
        cx.notify();
    }

    fn cancel_share_link(&mut self, cx: &mut Context<Self>) {
        self.share_link_text = None;
        cx.notify();
    }

    /// Prefill the composer of the chat a link just opened. Existing
    /// unsent text stays: the link's text is appended on a new line.
    pub(super) fn prefill_link_draft(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if text.is_empty() {
            return;
        }
        let current = self.composer.read(cx).value().to_string();
        let value = if current.trim().is_empty() {
            text.to_string()
        } else {
            format!("{current}\n{text}")
        };
        self.composer
            .update(cx, |input, cx| input.set_value(&value, window, cx));
    }

    /// Seek a linked media timestamp once the message has loaded.
    pub(super) fn tick_media_seek(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, message_id, seconds, polls)) = self.pending_media_seek else {
            return;
        };
        let loaded = self.session().is_some_and(|session| {
            session
                .histories
                .get(&chat_id.0)
                .is_some_and(|history| history.messages.contains_key(&message_id.0))
        });
        if loaded {
            self.pending_media_seek = None;
            self.seek_media_timestamp(chat_id, message_id, seconds, cx);
        } else if polls >= SEEK_POLLS {
            self.pending_media_seek = None;
        } else {
            self.pending_media_seek = Some((chat_id, message_id, seconds, polls + 1));
        }
    }

    /// Chat-open extras for the typed links: draft prefill, linked
    /// message, thread and media timestamp.
    pub(super) fn finish_typed_link(
        &mut self,
        chat_id: ChatId,
        action: &DeepLinkAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            DeepLinkAction::OpenPublicChatDraft { draft, .. }
            | DeepLinkAction::OpenUserDraft { draft, .. } => {
                self.prefill_link_draft(draft, window, cx);
            }
            DeepLinkAction::ShareDraft { text } => self.prefill_link_draft(text, window, cx),
            DeepLinkAction::OpenChatById {
                message_id,
                media_timestamp,
                thread_id,
                ..
            } => {
                if let Some(seconds) = media_timestamp
                    && *message_id > 0
                {
                    self.pending_media_seek = Some((chat_id, MessageId(*message_id), *seconds, 0));
                }
                if let Some(thread) = thread_id {
                    self.open_thread_view(chat_id, MessageId(*thread), window, cx);
                }
            }
            _ => {}
        }
    }

    pub(super) fn build_deep_link_share_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::DeepLinkShare, |this, _, cx| {
                this.cancel_share_link(cx);
            });
        app.update(cx, |this, cx| {
            let text = this.share_link_text.clone().unwrap_or_default();
            let choices: Vec<(ChatId, String)> = this
                .session()
                .map(|session| {
                    session
                        .forward_destinations("")
                        .into_iter()
                        .take(SHARE_CHOICES)
                        .map(|chat| (chat.id, chat.title.clone()))
                        .collect()
                })
                .unwrap_or_default();
            let mut list = div().flex().flex_col().gap_1();
            if choices.is_empty() {
                list = list.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("No chats loaded yet."),
                );
            }
            for (id, title) in choices {
                list = list.child(
                    Button::new(SharedString::from(format!("share-link-{}", id.0)))
                        .label(title)
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.choose_share_chat(id, cx);
                            this.close_kit_dialog_if_done(DialogKind::DeepLinkShare, window, cx);
                        })),
                );
            }
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().muted.opacity(0.5))
                        .text_sm()
                        .child(text),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("The text goes into the message box. Nothing is sent."),
                )
                .child(list)
                .into_any_element();
            let footer = div().flex().justify_end().gap_2().child(
                Button::new("share-link-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.cancel_share_link(cx);
                        this.close_kit_dialog_if_done(DialogKind::DeepLinkShare, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Share to…"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

#[cfg(test)]
mod routes_tests {
    use super::settings_action;
    use quill::deep_link_types::SettingsTarget;

    #[test]
    fn every_settings_target_names_its_page_or_the_list() {
        assert!(settings_action(SettingsTarget::Appearance).is_some());
        assert!(settings_action(SettingsTarget::PrivacyAndSecurity).is_some());
        assert!(settings_action(SettingsTarget::Devices).is_some());
        assert!(settings_action(SettingsTarget::Root).is_none());
        assert!(settings_action(SettingsTarget::Unsupported).is_none());
    }
}
