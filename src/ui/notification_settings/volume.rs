//! Notification sound volume slider (tdesktop "Volume" under notification
//! sounds, `Core::Settings::notificationsVolume`). App-wide, persisted by
//! `quill::notify_prefs`, applied in `audio::NotificationSounds::play`.

use super::*;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};

impl QuillApp {
    /// Create the slider on first use, seeded from the saved volume.
    pub(in crate::ui) fn ensure_notification_volume_slider(&mut self, cx: &mut Context<Self>) {
        if self.notify.volume_slider.is_some() {
            return;
        }
        let start = f32::from(quill::notify_prefs::current().volume_percent());
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(f32::from(quill::notify_prefs::MAX_VOLUME))
                .step(1.0)
                .default_value(start)
        });
        cx.subscribe(&slider, |this, _, event: &SliderEvent, cx| {
            this.on_notification_volume_event(event, cx);
        })
        .detach();
        self.notify.volume_slider = Some(slider);
    }

    fn on_notification_volume_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        match event {
            SliderEvent::Change(value) => {
                quill::notify_prefs::set_volume(volume_from_slider(value.end()));
                cx.notify();
            }
            SliderEvent::Release(value) => {
                quill::notify_prefs::set_volume(volume_from_slider(value.end()));
                // tdesktop plays the sound at the new volume on release.
                self.notify
                    .notification_sounds
                    .play(crate::ui::audio::NotificationSound::DefaultTone);
                cx.notify();
            }
        }
    }

    pub(in crate::ui) fn notification_volume_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let percent = quill::notify_prefs::current().volume_percent();
        div()
            .id("notification-volume-section")
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().font_semibold().text_sm().child("Sound volume"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{percent}%")),
                    ),
            )
            .when_some(self.notify.volume_slider.clone(), |this, slider| {
                this.child(
                    div()
                        .id("notification-volume-slider")
                        .role(Role::Group)
                        .aria_label("Notification sound volume")
                        .child(Slider::new(&slider)),
                )
            })
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("How loud Quill plays message and call alert sounds"),
            )
            .into_any_element()
    }
}

/// Slider position to a whole percent in 0-100.
fn volume_from_slider(value: f32) -> u8 {
    value
        .round()
        .clamp(0.0, f32::from(quill::notify_prefs::MAX_VOLUME)) as u8
}

#[cfg(test)]
mod tests {
    use super::volume_from_slider;

    #[test]
    fn slider_positions_round_and_clamp() {
        assert_eq!(volume_from_slider(0.0), 0);
        assert_eq!(volume_from_slider(49.6), 50);
        assert_eq!(volume_from_slider(100.0), 100);
        assert_eq!(volume_from_slider(180.0), 100);
        assert_eq!(volume_from_slider(-4.0), 0);
    }
}
