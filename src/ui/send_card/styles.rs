//! The send card's other two designs, kept as prototypes until one of the
//! three is chosen. `QUILL_DEMO_SEND_STYLE` picks what the `ready-send-card`
//! demo draws: `a` (the default, the card in `mod.rs`: previews with the
//! options beside them, as wide as what it holds), `b` (a modal box like
//! tdesktop's `SendFilesBox`, with its own caption field) or `c` (the
//! options as a strip above the previews, in a card as wide as the chat).
//!
//! `b` and `c` only draw: the box's caption field, Add, Cancel and Send do
//! nothing yet, and no build without `demo-capture` has either.

use super::layout::{CardRoom, CardShape, MOSAIC_WIDTH, TrayLayout};
use super::options::tray_title;
use crate::ui::app::QuillApp;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::cell::RefCell;
use std::rc::Rc;

/// Which of the three designs the demo draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SendStyle {
    /// Previews with the options beside them, in a card that hugs them.
    Beside,
    /// A modal box over the chat.
    Box,
    /// The options as a strip above the previews.
    Strip,
}

impl SendStyle {
    pub(super) fn chosen() -> Self {
        match std::env::var("QUILL_DEMO_SEND_STYLE").as_deref() {
            Ok("b") => Self::Box,
            Ok("c") => Self::Strip,
            _ => Self::Beside,
        }
    }
}

impl QuillApp {
    fn build_send_box_prototype(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SendBoxPrototype, |_, _, _| {});
        app.update(cx, |this, cx| {
            let layout = TrayLayout::of(
                this.pending_attachments(),
                this.composer_group_media_effective(),
            );
            let room = CardRoom {
                shape: CardShape::Stacked,
                preview: MOSAIC_WIDTH,
            };
            let title = tray_title(this.pending_attachments());
            let previews = div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .p_2()
                .rounded_lg()
                .bg(cx.theme().muted)
                .children(this.send_card_media(&layout, room, cx))
                .children(this.send_card_files(&layout, room, cx));
            let rows = this.send_card_choice_rows(CardShape::Beside, cx);
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(previews)
                .child(div().flex().flex_col().gap_2().children(rows))
                .into_any_element();
            // The content closure is `Fn`; the body is drawn once per build.
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .w(px(448.))
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, window, cx| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        let caption = window.use_keyed_state(
                            "send-box-prototype-caption",
                            cx,
                            |window, cx| {
                                InputState::new(window, cx)
                                    .placeholder(tr(keys::Q_CHAT_ADD_A_CAPTION))
                            },
                        );
                        content.child(body).child(
                            div().pt_3().child(
                                Input::new(&caption).suffix(
                                    Button::new("send-box-prototype-emoji")
                                        .icon(IconName::FaceSlightlySmiling)
                                        .ghost()
                                        .xsmall(),
                                ),
                            ),
                        )
                    },
                ))
                .footer(
                    div()
                        .flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("send-box-prototype-add")
                                .icon(IconName::Plus)
                                .label(tr(keys::Q_MEDIA_ADD))
                                .ghost(),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("send-box-prototype-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost(),
                        )
                        .child(
                            Button::new("send-box-prototype-send")
                                .label(tr(keys::SEND_BUTTON))
                                .primary(),
                        ),
                )
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    SendBoxPrototype => DialogSpec::new(
        5439,
        |_| SendStyle::chosen() == SendStyle::Box,
        QuillApp::build_send_box_prototype,
    )
    // The demo switch shows it; it holds nothing to drop.
    .close(|_, _, _| {}),
}
