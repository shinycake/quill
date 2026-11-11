//! The send card's header: what is being sent, the send choices that
//! differ from the defaults as clearable chips, the options menu (tdesktop
//! puts the same choices in the send box's top menu) and Remove all.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::composer::option_chip;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{AttachmentKind, ComposerAttachment};
use quill::lang::{keys, tr, tr_with, trn};
use quill::telegram::requests::SelfDestructSend;

/// The send choices for the open tray, read once per render.
#[derive(Clone, Copy)]
pub(super) struct Choices {
    several: bool,
    grouped: bool,
    remember: bool,
    can_files: bool,
    as_files: bool,
    large_photo: bool,
    hd: bool,
    captionable: bool,
    caption_above: bool,
    timer: Option<Option<SelfDestructSend>>,
    paid: Option<i64>,
}

impl QuillApp {
    fn send_card_choices(&self) -> Choices {
        let tray = self.pending_attachments();
        let can_files = ComposerAttachment::can_send_as_files(tray);
        Choices {
            several: tray.len() >= 2,
            grouped: self.composer_group_media_effective(),
            remember: self
                .session()
                .is_some_and(|s| s.settings.media_prefs.remember_media_grouping),
            can_files,
            as_files: can_files
                && tray
                    .iter()
                    .all(|attachment| attachment.kind == AttachmentKind::Document),
            large_photo: self.tray_has_large_photo(),
            hd: self.pending_send().hd_photos,
            captionable: !self.open_chat_is_secret()
                && tray.iter().any(|attachment| {
                    matches!(
                        attachment.kind,
                        AttachmentKind::Photo | AttachmentKind::Video
                    )
                }),
            caption_above: self.pending_send().caption_above,
            timer: self
                .self_destruct_picker_visible()
                .then(|| self.pending_send().self_destruct),
            paid: self
                .paid_media_available()
                .then(|| self.pending_send().paid_stars),
        }
    }

    /// What the card is waiting on before it can send, if anything.
    pub(in crate::ui) fn send_card_status(&self) -> Option<&'static str> {
        if self.tray_preparing() {
            Some(tr(keys::Q_OPS_PREPARING_THE_FILES))
        } else if self.tray_videos_checking() {
            Some(tr(keys::Q_OPS_CHECKING_THE_VIDEO))
        } else {
            None
        }
    }

    #[inline(never)]
    pub(super) fn send_card_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let choices = self.send_card_choices();
        let status = self.send_card_status();
        let muted = cx.theme().muted_foreground;
        div()
            .id("send-card-header")
            .flex()
            .items_center()
            .gap_2()
            .min_h(px(28.))
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .flex_none()
                    .child(tray_title(self.pending_attachments())),
            )
            .when_some(status, |row, status| {
                row.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(muted)
                        .child(Spinner::new().xsmall().color(muted))
                        .child(status),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .children(self.send_card_chips(choices, cx)),
            )
            .child(self.send_card_options_button(choices, cx))
            .child(
                Button::new("send-card-clear")
                    .icon(IconName::X)
                    .ghost()
                    .small()
                    .tooltip(if choices.several {
                        tr(keys::Q_CHAT_REMOVE_ALL)
                    } else {
                        tr(keys::BOX_REMOVE)
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.clear_attachment(cx))),
            )
            .into_any_element()
    }

    /// One clearable chip per choice that differs from the default, so what
    /// will be sent is visible without opening the menu.
    #[inline(never)]
    fn send_card_chips(&self, choices: Choices, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut chips = Vec::new();
        if choices.as_files {
            chips.push(clearable(
                "send-card-chip-files",
                tr(keys::Q_CHAT_SEND_WITHOUT_COMPRESSION).into(),
                cx.listener(|this, _, _, cx| this.set_send_card_as_files(false, cx)),
                cx,
            ));
        }
        if choices.several && !choices.grouped && !choices.as_files {
            chips.push(clearable(
                "send-card-chip-apart",
                tr(keys::Q_CHAT_SEND_SEPARATELY).into(),
                cx.listener(|this, _, _, cx| this.toggle_composer_group_media(cx)),
                cx,
            ));
        }
        if choices.large_photo && choices.hd && !choices.as_files {
            chips.push(clearable(
                "send-card-chip-hd",
                tr(keys::SEND_HIGH_QUALITY).into(),
                cx.listener(|this, _, _, cx| {
                    this.update_pending_send(|send| send.hd_photos = false);
                    cx.notify();
                }),
                cx,
            ));
        }
        if choices.captionable && choices.caption_above {
            chips.push(clearable(
                "send-card-chip-above",
                tr(keys::Q_CHAT_CAPTION_ABOVE_MEDIA).into(),
                cx.listener(|this, _, _, cx| {
                    this.update_pending_send(|send| send.caption_above = false);
                    cx.notify();
                }),
                cx,
            ));
        }
        if let Some(Some(_)) = choices.timer {
            chips.push(clearable(
                "send-card-chip-timer",
                format!("⏱ {}", self.self_destruct_button_label()).into(),
                cx.listener(|this, _, _, cx| this.set_composer_self_destruct(None, cx)),
                cx,
            ));
        }
        if let Some(stars) = choices.paid.filter(|stars| *stars > 0) {
            chips.push(
                Button::new("send-card-chip-paid")
                    .icon(IconName::Star)
                    .label(tr_with(
                        keys::Q_OPS_UNLOCK_FOR_STARS,
                        &[("stars", &stars.to_string())],
                    ))
                    .xsmall()
                    .ghost()
                    .selected(true)
                    .tooltip(tr(keys::Q_OPS_ASK_FOR_STARS_TO_SEE))
                    .on_click(cx.listener(|this, _, window, cx| this.open_price_dialog(window, cx)))
                    .into_any_element(),
            );
        }
        chips
    }

    fn set_send_card_as_files(&mut self, as_files: bool, cx: &mut Context<Self>) {
        self.update_pending_attachments(|tray| {
            ComposerAttachment::set_send_as_files(tray, as_files)
        });
        // Back to media: read the videos again.
        self.composer_sends.reads.sound.clear_done();
        self.check_tray_videos(cx);
        cx.notify();
    }

    #[inline(never)]
    fn send_card_options_button(&self, choices: Choices, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        Button::new("send-card-options")
            .icon(IconName::Ellipsis)
            .ghost()
            .small()
            .tooltip(tr(keys::Q_CHAT_SEND_OPTIONS))
            .dropdown_menu(move |menu, window, cx| {
                options_menu(menu, choices, owner.clone(), window, cx)
            })
            .into_any_element()
    }
}

/// tdesktop's send box menu, with the choices that are checkboxes in
/// Telegram Desktop's box.
fn options_menu(
    mut menu: PopupMenu,
    choices: Choices,
    owner: WeakEntity<QuillApp>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let item = |label: &str, checked: bool, act: fn(&mut QuillApp, &mut Context<QuillApp>)| {
        let owner = owner.clone();
        PopupMenuItem::new(label.to_string())
            .checked(checked)
            .on_click(move |_, _, cx| {
                let _ = owner.update(cx, act);
            })
    };
    if choices.several && !choices.as_files {
        menu = menu
            .item(item(
                tr(keys::Q_CHAT_SEND_AS_ALBUM),
                choices.grouped,
                |this, cx| this.toggle_composer_group_media(cx),
            ))
            .item(item(
                tr(keys::Q_CHAT_REMEMBER_CHOICE),
                choices.remember,
                |this, cx| this.toggle_remember_media_grouping(cx),
            ));
    }
    if choices.can_files {
        if choices.several && !choices.as_files {
            menu = menu.separator();
        }
        menu = menu.item(item(
            tr(keys::Q_CHAT_SEND_WITHOUT_COMPRESSION),
            choices.as_files,
            |this, cx| {
                let as_files = this
                    .pending_attachments()
                    .iter()
                    .all(|attachment| attachment.kind == AttachmentKind::Document);
                this.set_send_card_as_files(!as_files, cx)
            },
        ));
    }
    if choices.large_photo && !choices.as_files {
        menu = menu.item(item(tr(keys::SEND_HIGH_QUALITY), choices.hd, |this, cx| {
            this.update_pending_send(|send| send.hd_photos = !send.hd_photos);
            cx.notify();
        }));
    }
    if choices.captionable {
        menu = menu.item(item(
            tr(keys::Q_CHAT_CAPTION_ABOVE_MEDIA),
            choices.caption_above,
            |this, cx| {
                this.update_pending_send(|send| send.caption_above = !send.caption_above);
                cx.notify();
            },
        ));
    }
    if let Some(current) = choices.timer {
        let timer_owner = owner.clone();
        menu = menu.separator().submenu(
            tr(keys::Q_CHAT_SELF_DESTRUCT_TIMER),
            window,
            cx,
            move |mut sub, _, _| {
                for choice in QuillApp::SELF_DESTRUCT_CHOICES {
                    let owner = timer_owner.clone();
                    sub = sub.item(
                        PopupMenuItem::new(timer_label(choice))
                            .checked(choice == current)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.set_composer_self_destruct(choice, cx);
                                });
                            }),
                    );
                }
                sub
            },
        );
    }
    if let Some(stars) = choices.paid {
        let owner = owner.clone();
        menu = menu.separator().item(
            PopupMenuItem::new(if stars > 0 {
                tr(keys::Q_OPS_CHANGE_THE_PRICE).to_string()
            } else {
                tr(keys::Q_OPS_MAKE_PAID).to_string()
            })
            .icon(IconName::Star)
            .on_click(move |_, window, cx| {
                let _ = owner.update(cx, |this, cx| this.open_price_dialog(window, cx));
            }),
        );
    }
    menu
}

fn timer_label(choice: Option<SelfDestructSend>) -> String {
    match choice {
        None => tr(keys::Q_CHAT_OFF).to_string(),
        Some(SelfDestructSend::Timer(secs)) if secs < 60 => trn(keys::SECONDS, i64::from(secs)),
        Some(SelfDestructSend::Timer(secs)) => trn(keys::MINUTES, i64::from(secs / 60)),
        Some(SelfDestructSend::Immediately) => tr(keys::Q_CHAT_VIEW_ONCE).to_string(),
    }
}

/// A chip that clears its choice.
fn clearable(
    id: &'static str,
    label: SharedString,
    clear: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    option_chip(id, label.to_string(), cx)
        .child(
            Button::new((id, 1u64))
                .icon(IconName::X)
                .xsmall()
                .ghost()
                .accessibility_label(tr_with(keys::Q_CHAT_TURN_OFF_OPTION, &[("option", &label)]))
                .on_click(clear),
        )
        .into_any_element()
}

/// "2 Photos", "3 Videos", "4 Media", "2 Files" (tdesktop
/// `lng_media_selected_*`).
pub(super) fn tray_title(tray: &[ComposerAttachment]) -> String {
    let count = tray.len();
    let all = |kind: AttachmentKind| tray.iter().all(|attachment| attachment.kind == kind);
    let key = if all(AttachmentKind::Photo) {
        keys::MEDIA_SELECTED_PHOTO
    } else if all(AttachmentKind::Video) || all(AttachmentKind::VideoNote) {
        keys::MEDIA_SELECTED_VIDEO
    } else if all(AttachmentKind::Document) {
        keys::MEDIA_SELECTED_FILE
    } else {
        keys::MEDIA_SELECTED_MEDIA
    };
    trn(key, count as i64)
}
