//! The send card's options: what is being sent and every choice about how
//! (tdesktop's send box puts the same choices in checkboxes and its top
//! menu). Each choice is a labelled control that shows its state, so none
//! hides in a menu: a segmented control per either-or choice, a menu only
//! for the timer's five values.
#![deny(clippy::indexing_slicing)]

use super::layout::{CardShape, OPTIONS_WIDTH};
use crate::ui::app::QuillApp;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{AttachmentKind, ComposerAttachment};
use quill::lang::{keys, tr, tr_with, trn};
use quill::telegram::requests::SelfDestructSend;

/// How the photos and videos of the tray go out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Quality {
    /// Photos and videos, compressed by Telegram.
    Compressed,
    /// tdesktop "High Quality": big photos keep more of their size.
    Hd,
    /// tdesktop "Send without compression": every one goes as a file.
    File,
}

impl Quality {
    /// The choices on offer: "HD" only when a photo is big enough to gain
    /// from it.
    pub(super) fn offered(large_photo: bool) -> &'static [Quality] {
        if large_photo {
            &[Quality::Compressed, Quality::Hd, Quality::File]
        } else {
            &[Quality::Compressed, Quality::File]
        }
    }

    fn label(self) -> &'static str {
        match self {
            Quality::Compressed => tr(keys::Q_CHAT_QUALITY_COMPRESSED),
            Quality::Hd => tr(keys::Q_CHAT_QUALITY_HD),
            Quality::File => tr(keys::Q_CHAT_FILE),
        }
    }
}

/// The send choices for the open tray, read once per render. A choice the
/// tray or the chat does not offer is `None`.
#[derive(Clone, Copy)]
struct Choices {
    several: bool,
    /// Sent as albums, for a tray of two or more.
    grouped: Option<bool>,
    /// Whether a group of this tray is an album: it has a photo or video.
    album: bool,
    remember: bool,
    quality: Option<(Quality, &'static [Quality])>,
    caption_above: Option<bool>,
    timer: Option<Option<SelfDestructSend>>,
    paid: Option<i64>,
}

impl QuillApp {
    fn send_card_choices(&self) -> Choices {
        let tray = self.pending_attachments();
        let several = tray.len() >= 2;
        Choices {
            several,
            grouped: several.then(|| self.composer_group_media_effective()),
            album: tray
                .iter()
                .any(|attachment| attachment.kind != AttachmentKind::Document),
            remember: self
                .session()
                .is_some_and(|s| s.settings.media_prefs.remember_media_grouping),
            quality: self.send_card_quality(),
            caption_above: (!self.open_chat_is_secret()
                && tray.iter().any(|attachment| {
                    matches!(
                        attachment.kind,
                        AttachmentKind::Photo | AttachmentKind::Video
                    )
                }))
            .then(|| self.pending_send().caption_above),
            timer: self
                .self_destruct_picker_visible()
                .then(|| self.pending_send().self_destruct),
            paid: self
                .paid_media_available()
                .then(|| self.pending_send().paid_stars),
        }
    }

    /// The quality in effect and the ones on offer; `None` for a tray with
    /// no photo or video file to choose it for.
    pub(super) fn send_card_quality(&self) -> Option<(Quality, &'static [Quality])> {
        let tray = self.pending_attachments();
        if !ComposerAttachment::can_send_as_files(tray) {
            return None;
        }
        let offered = Quality::offered(self.tray_has_large_photo());
        let as_files = tray
            .iter()
            .all(|attachment| attachment.kind == AttachmentKind::Document);
        let current = if as_files {
            Quality::File
        } else if self.pending_send().hd_photos && offered.contains(&Quality::Hd) {
            Quality::Hd
        } else {
            Quality::Compressed
        };
        Some((current, offered))
    }

    /// Send the tray as albums or as separate messages (remembered when
    /// "Remember choice" is on).
    pub(super) fn set_send_card_grouped(&mut self, grouped: bool, cx: &mut Context<Self>) {
        if grouped != self.composer_group_media_effective() {
            self.toggle_composer_group_media(cx);
        }
    }

    pub(super) fn set_send_card_quality(&mut self, quality: Quality, cx: &mut Context<Self>) {
        let was_files = self
            .pending_attachments()
            .iter()
            .all(|attachment| attachment.kind == AttachmentKind::Document);
        let files = quality == Quality::File;
        if files != was_files {
            self.update_pending_attachments(|tray| {
                ComposerAttachment::set_send_as_files(tray, files)
            });
            // Back to media: read the videos again.
            self.composer_sends.reads.sound.clear_done();
            self.check_tray_videos(cx);
        }
        if !files {
            self.update_pending_send(|send| send.hd_photos = quality == Quality::Hd);
        }
        cx.notify();
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

    /// The options column: the title with Remove all, then one row per
    /// choice. Above the previews (`Stacked`) the rows flow side by side.
    #[inline(never)]
    pub(super) fn send_card_options(&self, shape: CardShape, cx: &mut Context<Self>) -> AnyElement {
        let choices = self.send_card_choices();
        let rows = self.send_card_option_rows(choices, shape, cx);
        div()
            .id("send-card-options")
            .debug_selector(|| "send-card-options".to_string())
            .flex()
            .flex_col()
            .gap_2()
            .map(|column| match shape {
                CardShape::Beside => column.flex_none().w(px(OPTIONS_WIDTH)),
                CardShape::Stacked => column.w_full(),
            })
            .child(self.send_card_title(choices, cx))
            .when(!rows.is_empty(), |column| {
                column.child(
                    div()
                        .flex()
                        .map(|rows| match shape {
                            CardShape::Beside => rows.flex_col().gap_2(),
                            CardShape::Stacked => rows.flex_wrap().gap_x_4().gap_y_2(),
                        })
                        .children(rows),
                )
            })
            .into_any_element()
    }

    /// The choices as rows, without the title: what the prototype box
    /// (`styles.rs`) lists under its previews.
    #[cfg(feature = "demo-capture")]
    pub(super) fn send_card_choice_rows(
        &self,
        shape: CardShape,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.send_card_option_rows(self.send_card_choices(), shape, cx)
    }

    fn send_card_title(&self, choices: Choices, cx: &mut Context<Self>) -> AnyElement {
        let status = self.send_card_status();
        let muted = cx.theme().muted_foreground;
        div()
            .id("send-card-title")
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .flex_none()
                    .child(tray_title(self.pending_attachments())),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(muted)
                    .when_some(status, |row, status| {
                        row.child(Spinner::new().xsmall().color(muted))
                            .child(div().truncate().child(status))
                    }),
            )
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

    #[inline(never)]
    fn send_card_option_rows(
        &self,
        choices: Choices,
        shape: CardShape,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        if let Some(grouped) = choices.grouped {
            rows.push(option_row(
                tr(keys::Q_CHAT_SEND_AS),
                segmented(
                    "send-card-grouping",
                    [
                        // Files go out together, but not as an album.
                        if choices.album {
                            tr(keys::Q_CHAT_ALBUM)
                        } else {
                            tr(keys::Q_CHAT_SEND_TOGETHER)
                        },
                        tr(keys::Q_CHAT_SEND_SEPARATELY),
                    ],
                    usize::from(!grouped),
                    cx.listener(|this, picked: &usize, _, cx| {
                        this.set_send_card_grouped(*picked == 0, cx);
                    }),
                ),
                cx,
            ));
            let remember = Checkbox::new("send-card-remember")
                .label(tr(keys::Q_CHAT_REMEMBER_CHOICE))
                .small()
                .checked(choices.remember)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_remember_media_grouping(cx);
                }));
            rows.push(match shape {
                // Under the control it is about, on the controls' spine.
                CardShape::Beside => option_row("", remember, cx),
                CardShape::Stacked => div()
                    .flex()
                    .items_center()
                    .child(remember)
                    .into_any_element(),
            });
        }
        if let Some((current, offered)) = choices.quality {
            rows.push(option_row(
                tr(keys::Q_CHAT_QUALITY),
                segmented(
                    "send-card-quality",
                    offered.iter().map(|quality| quality.label()),
                    offered
                        .iter()
                        .position(|quality| *quality == current)
                        .unwrap_or(0),
                    cx.listener(move |this, picked: &usize, _, cx| {
                        if let Some(quality) = offered.get(*picked) {
                            this.set_send_card_quality(*quality, cx);
                        }
                    }),
                ),
                cx,
            ));
        }
        if let Some(above) = choices.caption_above {
            rows.push(option_row(
                tr(keys::Q_MEDIA_CAPTION),
                segmented(
                    "send-card-caption",
                    [
                        tr(keys::Q_CHAT_CAPTION_BELOW),
                        tr(keys::Q_CHAT_CAPTION_ABOVE),
                    ],
                    usize::from(above),
                    cx.listener(|this, picked: &usize, _, cx| {
                        let above = *picked == 1;
                        this.update_pending_send(|send| send.caption_above = above);
                        cx.notify();
                    }),
                ),
                cx,
            ));
        }
        if let Some(current) = choices.timer {
            rows.push(option_row(
                tr(keys::Q_CHAT_TIMER),
                self.send_card_timer_button(current, cx),
                cx,
            ));
        }
        if let Some(stars) = choices.paid {
            rows.push(option_row(
                tr(keys::Q_MONEY_PRICE),
                Button::new("send-card-price")
                    .icon(IconName::Star)
                    .label(if stars > 0 {
                        tr_with(
                            keys::Q_OPS_UNLOCK_FOR_STARS,
                            &[("stars", &stars.to_string())],
                        )
                    } else {
                        tr(keys::Q_OPS_MAKE_PAID).to_string()
                    })
                    .outline()
                    .small()
                    .tooltip(tr(keys::Q_OPS_ASK_FOR_STARS_TO_SEE))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.open_price_dialog(window, cx)),
                    ),
                cx,
            ));
        }
        rows
    }

    /// The self-destruct timer: its value on a button that opens the five
    /// choices.
    fn send_card_timer_button(
        &self,
        current: Option<SelfDestructSend>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        Button::new("send-card-timer")
            .icon(IconName::Timer)
            .label(timer_label(current))
            .outline()
            .small()
            .dropdown_caret(true)
            .tooltip(tr(keys::Q_CHAT_SELF_DESTRUCT_TIMER))
            .dropdown_menu(move |mut menu, _, _| {
                for choice in QuillApp::SELF_DESTRUCT_CHOICES {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(timer_label(choice))
                            .checked(choice == current)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.set_composer_self_destruct(choice, cx);
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }
}

/// One choice: its name in the label column, its control next to it. An
/// empty name keeps the control on the same spine (a follow-up to the row
/// above).
fn option_row(
    name: &'static str,
    control: impl IntoElement,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_none()
                .w_16()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .truncate()
                .child(name),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

/// An either-or choice as one segmented control.
fn segmented(
    id: &'static str,
    labels: impl IntoIterator<Item = &'static str>,
    selected: usize,
    pick: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> TabBar {
    labels
        .into_iter()
        .fold(TabBar::new(id).segmented().small(), |bar, label| {
            bar.child(Tab::new().label(label))
        })
        .selected_index(selected)
        .on_click(pick)
}

fn timer_label(choice: Option<SelfDestructSend>) -> String {
    match choice {
        None => tr(keys::Q_CHAT_OFF).to_string(),
        Some(SelfDestructSend::Timer(secs)) if secs < 60 => trn(keys::SECONDS, i64::from(secs)),
        Some(SelfDestructSend::Timer(secs)) => trn(keys::MINUTES, i64::from(secs / 60)),
        Some(SelfDestructSend::Immediately) => tr(keys::Q_CHAT_VIEW_ONCE).to_string(),
    }
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
