//! The phone sheets of the passkey ceremony (tdesktop
//! `webauthn/cable_box.cpp`):
//!
//! - Choose: the register or login title, the about line, the QR code on
//!   a white card with the Quill mark in its centre, "Keep Bluetooth
//!   on…", the "Use a security key" pill and "Insert and touch your
//!   security key." Without Bluetooth the about line says so and the QR
//!   and its hint are left out.
//! - Connecting: "Connecting to your phone…" with the passkey icon.
//! - Continue: "Continue on your phone", "Follow the instructions on your
//!   phone."
//!
//! Every phone sheet, and the error sheet, has one full-width button at
//! the bottom: Cancel, or Close on the error.
#![deny(clippy::indexing_slicing)]

use super::{CeremonyMode, CeremonySheet};
use crate::ui::app::QuillApp;
use crate::ui::owned_image::OwnedImage;
use crate::ui::shell::DialogKind;
use gpui_kit::TestSupportExt;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{Key, keys, tr};
use smallvec::SmallVec;
use std::sync::{Arc, OnceLock};

/// tdesktop `passkeyCableQrSize`: the QR code's side.
const QR_SIZE: f32 = 260.;
/// tdesktop `introQrBackgroundSkip`: the white card around the code.
const QR_SKIP: f32 = 12.;
/// tdesktop `passkeyCableQrCenter`: the white square with the mark.
const QR_CENTER: f32 = 64.;
/// Image pixels per logical pixel, so the modules stay sharp on HiDPI.
const QR_SCALE: u32 = 3;

static LOGO_PNG: &[u8] = include_bytes!("../../../assets/icons/hicolor/128x128/apps/quill.png");

/// The rendered QR code and its side in logical pixels.
pub(super) struct CableQr {
    image: OwnedImage,
    side: f32,
}

/// tdesktop `RenderQr`: quartile redundancy (the centre is covered by the
/// mark), whole pixels per module, at most `QR_SIZE` wide.
pub(super) fn render_cable_qr(text: &str) -> Option<CableQr> {
    let code =
        qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::Q).ok()?;
    let modules = u32::try_from(code.width()).ok()?.max(1);
    let module = (QR_SIZE as u32 / modules).max(1);
    let luma = code
        .render::<image::Luma<u8>>()
        .quiet_zone(false)
        .module_dimensions(module * QR_SCALE, module * QR_SCALE)
        .build();
    let rgba = image::DynamicImage::ImageLuma8(luma).into_rgba8();
    Some(CableQr {
        image: OwnedImage::new(Arc::new(RenderImage::new(SmallVec::from_buf([
            image::Frame::new(rgba),
        ])))),
        side: (module * modules) as f32,
    })
}

/// Demos and tests: a `FIDO:/` code as long as a real one (about 70
/// bytes of CBOR, 17 digits per 7 bytes), from no real key.
pub(super) fn demo_qr() -> String {
    let digits: String = (0..170u32)
        .map(|i| char::from(b'0' + u8::try_from((i * 7 + 3) % 10).unwrap_or(0)))
        .collect();
    format!("FIDO:/{digits}")
}

fn logo() -> Arc<Image> {
    static LOGO: OnceLock<Arc<Image>> = OnceLock::new();
    LOGO.get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO_PNG.to_vec())))
        .clone()
}

/// The code on its white card, the mark on a white rounded square in the
/// middle (tdesktop `QrCenterImage`: radius a fifth of the side, the mark
/// at 94%).
fn qr_card((image, side): (Arc<RenderImage>, f32), label: SharedString) -> impl IntoElement {
    let mark = QR_CENTER * 0.94;
    div().flex().justify_center().child(
        div()
            .id("passkey-cable-qr")
            .test_support()
            .role(Role::Image)
            .aria_label(label)
            .p(px(QR_SKIP))
            .rounded_lg()
            .bg(gpui_kit::white())
            .child(
                div()
                    .relative()
                    .size(px(side))
                    .child(
                        img(ImageSource::from(image))
                            .size(px(side))
                            .object_fit(ObjectFit::Contain),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .size(px(QR_CENTER))
                                    .rounded(px(QR_CENTER / 5.))
                                    .bg(gpui_kit::white())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(img(ImageSource::Image(logo())).size(px(mark))),
                            ),
                    ),
            ),
    )
}

/// The description line under the title (tdesktop `boxLabel`).
fn description(text: SharedString, cx: &App) -> impl IntoElement {
    div()
        .id("passkey-ceremony-about")
        .test_support()
        .role(Role::Status)
        .aria_label(text.clone())
        .text_sm()
        .text_color(cx.theme().foreground)
        .child(text)
}

/// A centred hint (tdesktop `passkeyCableHintLabel`).
fn hint(id: &'static str, key: Key, cx: &App) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .text_xs()
        .text_center()
        .text_color(cx.theme().muted_foreground)
        .child(tr(key))
}

impl QuillApp {
    /// The title and body of a phone sheet; `None` for the key sheets.
    pub(super) fn cable_sheet(
        &mut self,
        mode: CeremonyMode,
        sheet: CeremonySheet,
        cx: &mut Context<Self>,
    ) -> Option<(Key, AnyElement)> {
        let body = div().flex().flex_col().gap_3();
        match sheet {
            CeremonySheet::Choose { bluetooth } => {
                let qr = self
                    .privacy
                    .passkeys
                    .ceremony
                    .as_ref()
                    .and_then(|ui| ui.qr.as_ref())
                    .map(|qr| (Arc::clone(&qr.image), qr.side))
                    .filter(|_| bluetooth);
                let (title, about) = match mode {
                    CeremonyMode::Register => (
                        keys::PASSKEY_CABLE_REGISTER_TITLE,
                        keys::PASSKEY_CABLE_REGISTER_ABOUT,
                    ),
                    CeremonyMode::Login => (
                        keys::PASSKEY_CABLE_LOGIN_TITLE,
                        keys::PASSKEY_CABLE_LOGIN_ABOUT,
                    ),
                };
                let body = match qr {
                    Some(qr) => body
                        .child(description(tr(about).into(), cx))
                        .child(qr_card(qr, tr(about).into()))
                        .child(hint(
                            "passkey-cable-bluetooth",
                            keys::PASSKEY_CABLE_BLUETOOTH,
                            cx,
                        )),
                    None => {
                        body.child(description(tr(keys::PASSKEY_CABLE_NO_BLUETOOTH).into(), cx))
                    }
                };
                let body = body
                    .child(
                        Button::new("passkey-cable-use-key")
                            .label(tr(keys::PASSKEY_METHOD_KEY))
                            .secondary()
                            .rounded(ButtonRounded::Size(px(999.)))
                            .w_full()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.use_passkey_security_key(cx);
                            })),
                    )
                    .child(hint(
                        "passkey-cable-key-about",
                        keys::PASSKEY_CABLE_KEY_ABOUT,
                        cx,
                    ));
                Some((title, body.into_any_element()))
            }
            CeremonySheet::Connecting | CeremonySheet::Continue => {
                let (title, about) = if sheet == CeremonySheet::Continue {
                    (
                        keys::PASSKEY_CABLE_CONTINUE,
                        Some(keys::PASSKEY_CABLE_FOLLOW),
                    )
                } else {
                    (keys::PASSKEY_CABLE_CONNECTING, None)
                };
                let mut body =
                    body.child(super::super::passkeys::header_icon(IconName::UserKey, cx));
                if let Some(about) = about {
                    body = body.child(description(tr(about).into(), cx));
                }
                Some((title, body.into_any_element()))
            }
            _ => None,
        }
    }

    /// "Use a security key": the worker looks for a key; with none the
    /// QR stays and a toast says so (`CeremonyEvent::NoKey`).
    fn use_passkey_security_key(&mut self, cx: &mut Context<Self>) {
        if let Some(ceremony) = self
            .privacy
            .passkeys
            .ceremony
            .as_ref()
            .and_then(|ui| ui.ceremony.as_ref())
        {
            ceremony.use_security_key();
        }
        cx.notify();
    }
}

/// The one bottom button of the phone and error sheets (tdesktop: a
/// full-width light button, Close on the error, Cancel otherwise).
pub(super) fn bottom_button(sheet: CeremonySheet, cx: &mut Context<QuillApp>) -> Option<Button> {
    let (id, label) = match sheet {
        CeremonySheet::Error(_) => ("passkey-ceremony-close", keys::CLOSE),
        CeremonySheet::Choose { .. } | CeremonySheet::Connecting | CeremonySheet::Continue => {
            ("passkey-ceremony-cancel", keys::CANCEL)
        }
        _ => return None,
    };
    Some(
        Button::new(id)
            .label(tr(label))
            .ghost()
            .w_full()
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_passkey_ceremony(window, cx);
                this.close_kit_dialog_if_done(DialogKind::PasskeyCeremony, window, cx);
            })),
    )
}
