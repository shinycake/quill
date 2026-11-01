//! The Location screen (tdesktop `settings_location.cpp`). tdesktop picks
//! the map point with a map widget that needs Telegram's map tokens; Quill
//! has no map picker, so it edits the address and can keep or drop a point
//! set on another device, with tdesktop's fallback note for the map.

use super::{Leave, LocationDraft};
use crate::ui::app::QuillApp;
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::business_info::BusinessLocation;
use quill::business_settings::{ADDRESS_MAX, location_to_save};
use quill::lang::{keys, tr};

fn about() -> &'static str {
    tr(keys::LOCATION_ABOUT)
}
fn map_fallback() -> &'static str {
    tr(keys::LOCATION_FALLBACK)
}

impl QuillApp {
    pub(super) fn new_location_draft(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> LocationDraft {
        let saved = self.my_business().location.unwrap_or_default();
        let address = super::live_input(window, cx, tr(keys::LOCATION_ADDRESS));
        address.update(cx, |input, cx| {
            input.set_value(saved.address.clone(), window, cx)
        });
        LocationDraft {
            address,
            point_e6: saved.point_e6,
        }
    }

    pub(super) fn business_location_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some((address, point)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.location.as_ref())
            .map(|l| (l.address.clone(), l.point_e6))
        else {
            return div();
        };
        let len = address.read(cx).value().trim().chars().count();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(about()))
            .child(Input::new(&address).aria_label(tr(keys::Q_OPS_ADDRESS)))
            .child(
                div()
                    .text_xs()
                    .text_color(if len > ADDRESS_MAX {
                        cx.theme().danger
                    } else {
                        muted
                    })
                    .child(format!("{len}/{ADDRESS_MAX}")),
            );
        if let Some(map_url) = point.and_then(|p| {
            BusinessLocation {
                address: String::new(),
                point_e6: Some(p),
            }
            .map_url()
        }) {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().child(tr(keys::LOCATION_SET_MAP)))
                            .child(
                                gpui_kit::component::link::Link::new("business-location-map")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_web_link(
                                            &map_url,
                                            tr(keys::Q_PEOPLE_COULDNT_OPEN_THE_MAP),
                                            cx,
                                        );
                                    }))
                                    .text_xs()
                                    .child(tr(keys::Q_OPS_OPEN_ON_MAP)),
                            ),
                    )
                    .child(
                        Switch::new("business-location-point")
                            .checked(true)
                            .accessibility_label(tr(keys::LOCATION_SET_MAP))
                            .on_click(cx.listener(|this, &on: &bool, _, cx| {
                                if !on
                                    && let Some(l) =
                                        this.business_mut().and_then(|d| d.location.as_mut())
                                {
                                    l.point_e6 = None;
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        body.when(point.is_none(), |this| {
            this.child(div().text_xs().text_color(muted).child(map_fallback()))
        })
    }

    /// Save of the location screen (when it is left).
    pub(super) fn save_business_location(&mut self, cx: &mut Context<Self>) -> Leave {
        let Some((address, point)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.location.as_ref())
            .map(|l| (l.address.read(cx).value().to_string(), l.point_e6))
        else {
            return Leave::Go(None);
        };
        let wanted = location_to_save(&address, point);
        if wanted == self.my_business().location {
            return Leave::Go(None);
        }
        self.business_send("location", |driver| {
            driver.save_business_location(wanted.as_ref()).is_ok()
        })
    }
}
