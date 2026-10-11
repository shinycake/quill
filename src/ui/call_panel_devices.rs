//! Device settings inside the call window (tdesktop
//! `Calls::Panel::initMediaDeviceToggles` / `showDevicesMenu`): a small
//! chevron on the corner of the mute and camera buttons opens a menu of the
//! engine's devices over the button row. Audio lists microphones and
//! speakers, video lists cameras; a pick goes through the same
//! `select_call_device` that the main window's call card uses. The grouping
//! and the open/close routing are plain functions so they can be tested
//! without a window.

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::{MediaDevice, MediaDeviceKind};
use std::rc::Rc;

/// Which button's menu is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeviceMenuTarget {
    /// Microphone and speaker (the mute button).
    Audio,
    /// Camera (the video button).
    Camera,
}

impl DeviceMenuTarget {
    /// The device kinds the menu lists, in order.
    pub(super) fn kinds(self) -> &'static [MediaDeviceKind] {
        match self {
            Self::Audio => &[MediaDeviceKind::Microphone, MediaDeviceKind::Speaker],
            Self::Camera => &[MediaDeviceKind::Camera],
        }
    }
}

/// A click on a corner button: opens its menu, closes it when it is the
/// one already open, and switches when the other one is open.
pub(super) fn toggled(
    open: Option<DeviceMenuTarget>,
    clicked: DeviceMenuTarget,
) -> Option<DeviceMenuTarget> {
    (open != Some(clicked)).then_some(clicked)
}

fn kind_title(kind: MediaDeviceKind) -> &'static str {
    match kind {
        MediaDeviceKind::Microphone => "Microphone",
        MediaDeviceKind::Speaker => "Speaker",
        MediaDeviceKind::Camera => "Camera",
        MediaDeviceKind::Screen => "Screen",
    }
}

/// One device row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DeviceItem {
    pub id: String,
    pub name: String,
    pub selected: bool,
}

/// One titled group of devices of the same kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DeviceSection {
    pub kind: MediaDeviceKind,
    pub title: &'static str,
    pub items: Vec<DeviceItem>,
}

/// The sections of a menu: the target's kinds in order, each with the
/// devices of that kind and the chosen one marked. A kind with no device
/// still gets its section (empty), so the menu says what is missing
/// instead of dropping the row.
pub(super) fn sections(
    target: DeviceMenuTarget,
    devices: &[MediaDevice],
    microphone: Option<&str>,
    speaker: Option<&str>,
    camera: Option<&str>,
) -> Vec<DeviceSection> {
    target
        .kinds()
        .iter()
        .map(|&kind| {
            let chosen = match kind {
                MediaDeviceKind::Microphone => microphone,
                MediaDeviceKind::Speaker => speaker,
                MediaDeviceKind::Camera => camera,
                MediaDeviceKind::Screen => None,
            };
            DeviceSection {
                kind,
                title: kind_title(kind),
                items: devices
                    .iter()
                    .filter(|device| device.kind == kind)
                    .map(|device| DeviceItem {
                        id: device.id.clone(),
                        name: device.name.clone(),
                        selected: Some(device.id.as_str()) == chosen,
                    })
                    .collect(),
            }
        })
        .collect()
}

impl QuillApp {
    /// The menu content for `target` from the live driver, or the demo's
    /// devices in the screenshot build.
    pub(super) fn call_device_sections(&self, target: DeviceMenuTarget) -> Vec<DeviceSection> {
        if let Some(live) = self.live.as_ref() {
            let (microphone, speaker) = live.driver.selected_call_devices();
            sections(
                target,
                live.driver.call_devices(),
                microphone,
                speaker,
                live.driver.selected_call_camera(),
            )
        } else {
            let (microphone, speaker) = &self.demo_ui.selected_devices;
            let devices = self.demo_ui.call_devices.clone().unwrap_or_default();
            sections(
                target,
                &devices,
                microphone.as_deref(),
                speaker.as_deref(),
                self.demo_ui.selected_camera.as_deref(),
            )
        }
    }
}

/// A button with a small chevron on its disc's corner that toggles the
/// button's device menu.
pub(super) fn with_corner<V: 'static>(
    button: AnyElement,
    id: &'static str,
    target: DeviceMenuTarget,
    open: bool,
    cx: &mut Context<V>,
    on_toggle: impl Fn(&mut V, DeviceMenuTarget, &mut Window, &mut Context<V>) + 'static,
) -> AnyElement {
    let label = match target {
        DeviceMenuTarget::Audio => "Audio devices",
        DeviceMenuTarget::Camera => "Camera devices",
    };
    div()
        .relative()
        .child(button)
        .child(
            div()
                .id(id)
                .absolute()
                .top(px(-2.))
                .right(px(4.))
                .size(px(18.))
                .rounded_full()
                .bg(hsla(0., 0., 0., 0.55))
                .border_1()
                .border_color(hsla(0., 0., 1., 0.25))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(label)
                .hover(|style| style.bg(hsla(0., 0., 0., 0.75)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    on_toggle(this, target, window, cx);
                }))
                .child(
                    Icon::new(if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronUp
                    })
                    .with_size(px(12.))
                    .text_color(white()),
                ),
        )
        .into_any_element()
}

/// The menu over the button row, with a transparent backdrop that closes
/// it on an outside click.
pub(super) fn menu_overlay<V: 'static>(
    sections: Vec<DeviceSection>,
    cx: &mut Context<V>,
    on_pick: impl Fn(&mut V, MediaDeviceKind, String, &mut Window, &mut Context<V>) + 'static,
    on_close: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> AnyElement {
    let on_pick = Rc::new(on_pick);
    let mut menu = div()
        .id("call-device-menu")
        .w(px(300.))
        .max_h(px(320.))
        .overflow_y_scroll()
        .py(px(6.))
        .rounded(px(10.))
        .bg(rgb(0x2a2f35))
        .border_1()
        .border_color(hsla(0., 0., 1., 0.12))
        .shadow_lg()
        .flex()
        .flex_col()
        .text_color(white())
        // A press inside the menu is not an outside click.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    for section in sections {
        menu = menu.child(
            div()
                .px(px(12.))
                .pt(px(6.))
                .pb(px(2.))
                .text_size(px(11.))
                .text_color(rgb(0xaaabac))
                .child(section.title),
        );
        if section.items.is_empty() {
            menu = menu.child(
                div()
                    .px(px(12.))
                    .py(px(6.))
                    .text_size(px(13.))
                    .text_color(rgb(0xaaabac))
                    .child(format!("No {} found.", section.title.to_lowercase())),
            );
        }
        for item in section.items {
            let kind = section.kind;
            let pick = on_pick.clone();
            let id = item.id.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!(
                        "call-device-{}-{}",
                        section.title, item.id
                    )))
                    .px(px(12.))
                    .py(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_label(item.name.clone())
                    .hover(|style| style.bg(hsla(0., 0., 1., 0.1)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        pick(this, kind, id.clone(), window, cx);
                    }))
                    .child(div().size(px(14.)).when(item.selected, |this| {
                        this.child(Icon::new(IconName::Check).with_size(px(14.)))
                    }))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(13.))
                            .child(item.name),
                    ),
            );
        }
    }
    div()
        .id("call-device-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .justify_end()
        .items_center()
        .pb(px(104.))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, window, cx| on_close(this, window, cx)),
        )
        .child(menu)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{DeviceMenuTarget, sections, toggled};
    use quill::calls::engine::{MediaDevice, MediaDeviceKind};

    fn device(id: &str, kind: MediaDeviceKind) -> MediaDevice {
        MediaDevice {
            id: id.into(),
            name: format!("{id} name"),
            kind,
        }
    }

    #[test]
    fn corner_clicks_open_close_and_switch() {
        use DeviceMenuTarget::{Audio, Camera};
        assert_eq!(toggled(None, Audio), Some(Audio));
        assert_eq!(toggled(Some(Audio), Audio), None);
        assert_eq!(toggled(Some(Audio), Camera), Some(Camera));
        assert_eq!(toggled(Some(Camera), Camera), None);
    }

    #[test]
    fn audio_menu_lists_mics_then_speakers_and_marks_the_chosen() {
        let devices = [
            device("cam", MediaDeviceKind::Camera),
            device("mic1", MediaDeviceKind::Microphone),
            device("mic2", MediaDeviceKind::Microphone),
            device("spk", MediaDeviceKind::Speaker),
        ];
        let out = sections(
            DeviceMenuTarget::Audio,
            &devices,
            Some("mic2"),
            None,
            Some("cam"),
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kind, MediaDeviceKind::Microphone);
        let mics: Vec<_> = out[0].items.iter().map(|i| (&*i.id, i.selected)).collect();
        assert_eq!(mics, [("mic1", false), ("mic2", true)]);
        assert_eq!(out[1].kind, MediaDeviceKind::Speaker);
        assert!(out[1].items.iter().all(|i| !i.selected));
    }

    #[test]
    fn camera_menu_lists_only_cameras_and_keeps_an_empty_section() {
        let devices = [device("mic1", MediaDeviceKind::Microphone)];
        let out = sections(DeviceMenuTarget::Camera, &devices, None, None, None);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "Camera");
        assert!(out[0].items.is_empty());
    }
}
