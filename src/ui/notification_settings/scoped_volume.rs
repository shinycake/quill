//! Notification sound volume per scope (private chats, groups, channels)
//! and per chat (tdesktop `AddRingtonesVolumeSlider` with
//! `DefaultRingtonesVolumeController` / `ThreadRingtonesVolumeController`).
//! Saved per account by `quill::notify_volume`; the playback rule (chat,
//! else scope, else the app-wide volume) is `Session::notification_volume_override`.

use super::*;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use quill::lang::{keys, tr};
use quill::notify::NotificationSoundKind;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "scoped_volume_tests.rs"]
mod tests;

/// What a volume slider edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum VolumeTarget {
    Scope(NotificationSettingsScope),
    Chat(ChatId),
}

fn new_slider(percent: u8, cx: &mut Context<QuillApp>) -> Entity<SliderState> {
    cx.new(|_| {
        SliderState::new()
            .min(f32::from(quill::notify_volume::MIN_PERCENT))
            .max(f32::from(quill::notify_volume::MAX_PERCENT))
            .step(1.0)
            .default_value(f32::from(percent))
    })
}

fn percent_from_slider(value: f32) -> u8 {
    value.round().clamp(
        f32::from(quill::notify_volume::MIN_PERCENT),
        f32::from(quill::notify_volume::MAX_PERCENT),
    ) as u8
}

impl QuillApp {
    fn volume_account(&self) -> Option<String> {
        self.session().map(|session| session.account.0.clone())
    }

    /// The saved volumes of the account on screen.
    pub(in crate::ui) fn account_volumes(&self) -> quill::notify_volume::AccountVolumes {
        self.volume_account()
            .map(|account| quill::notify_volume::account(&account))
            .unwrap_or_default()
    }

    /// Create the three scope sliders on first use, seeded from the saved
    /// volumes.
    pub(in crate::ui) fn ensure_scope_volume_sliders(&mut self, cx: &mut Context<Self>) {
        if !self.notify.scope_volume_sliders.is_empty() {
            return;
        }
        let volumes = self.account_volumes();
        for scope in NotificationSettingsScope::ALL {
            let slider = new_slider(volumes.scope_slider_percent(scope), cx);
            self.subscribe_volume_slider(&slider, VolumeTarget::Scope(scope), cx);
            self.notify.scope_volume_sliders.push((scope, slider));
        }
    }

    /// Keep the chat slider for the chat the notification panel shows,
    /// re-seeded from the saved volume whenever the chat changes.
    pub(in crate::ui) fn ensure_chat_volume_slider(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.session().and_then(|session| session.open_chat) else {
            return;
        };
        if self
            .notify
            .chat_volume_slider
            .as_ref()
            .is_some_and(|(chat, _)| *chat == chat_id)
        {
            return;
        }
        let slider = new_slider(self.account_volumes().chat_slider_percent(chat_id), cx);
        self.subscribe_volume_slider(&slider, VolumeTarget::Chat(chat_id), cx);
        self.notify.chat_volume_slider = Some((chat_id, slider));
    }

    fn subscribe_volume_slider(
        &mut self,
        slider: &Entity<SliderState>,
        target: VolumeTarget,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe(slider, move |this, _, event: &SliderEvent, cx| {
            this.on_volume_slider_event(target, event, cx);
        })
        .detach();
    }

    fn on_volume_slider_event(
        &mut self,
        target: VolumeTarget,
        event: &SliderEvent,
        cx: &mut Context<Self>,
    ) {
        let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
        let percent = percent_from_slider(value.end());
        self.save_volume(target, percent);
        if matches!(event, SliderEvent::Release(_)) {
            self.play_volume_sample(target, percent);
        }
        cx.notify();
    }

    /// Save a scope or chat volume for the account on screen.
    pub(in crate::ui) fn save_volume(&mut self, target: VolumeTarget, percent: u8) {
        let Some(account) = self.volume_account() else {
            return;
        };
        quill::notify_volume::update(&account, |volumes| match target {
            VolumeTarget::Scope(scope) => volumes.set_scope(scope, percent),
            VolumeTarget::Chat(chat) => volumes.set_chat(chat, percent),
        });
    }

    /// tdesktop plays the chosen sound at the new volume when the slider is
    /// let go.
    fn play_volume_sample(&mut self, target: VolumeTarget, percent: u8) {
        let Some(session) = self.session() else {
            return;
        };
        let scope_sound = |scope| {
            session
                .settings
                .scope_notification_settings
                .get(&scope)
                .map_or(-1, |settings| settings.sound_id)
        };
        let sound_id = match target {
            VolumeTarget::Scope(scope) => scope_sound(scope),
            VolumeTarget::Chat(chat_id) => match session.chats.get(&chat_id.0) {
                Some(chat) if !chat.notification_settings.use_default_sound => {
                    chat.notification_settings.sound_id
                }
                Some(chat) => scope_sound(quill::state::scope_for_chat_kind(&chat.kind)),
                None => -1,
            },
        };
        let kind = match sound_id {
            0 => return,
            -1 => NotificationSoundKind::Default,
            id => NotificationSoundKind::Custom(id),
        };
        let sound = match self.live.as_mut() {
            Some(live) => {
                crate::ui::audio::notification_sound(live.driver.resolve_notification_sound(kind))
            }
            None => Some(crate::ui::audio::NotificationSound::DefaultTone),
        };
        if let Some(sound) = sound {
            self.notify
                .notification_sounds
                .play_at(sound, Some(percent));
        }
    }

    /// One scope's volume row, or `None` while its settings load or when
    /// the scope's sound is off.
    pub(in crate::ui) fn scope_volume_row(
        &self,
        scope: NotificationSettingsScope,
        sound_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if sound_id == 0 {
            return None;
        }
        let slider = self
            .notify
            .scope_volume_sliders
            .iter()
            .find(|(candidate, _)| *candidate == scope)
            .map(|(_, slider)| slider)?;
        let title = match scope {
            NotificationSettingsScope::PrivateChats => tr(keys::NOTIFICATION_VOLUME_PRIVATE_CHATS),
            NotificationSettingsScope::GroupChats => tr(keys::NOTIFICATION_VOLUME_GROUPS),
            NotificationSettingsScope::ChannelChats => tr(keys::NOTIFICATION_VOLUME_CHANNEL),
        };
        let percent = self.account_volumes().scope_slider_percent(scope);
        Some(volume_row(
            format!("scope-volume-{scope:?}"),
            title,
            percent,
            slider,
            cx,
        ))
    }

    /// The open chat's volume row in the notification panel, hidden while
    /// the chat's own sound is off.
    pub(in crate::ui) fn chat_volume_row(
        &self,
        chat_id: ChatId,
        sound_disabled: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if sound_disabled {
            return None;
        }
        let (slider_chat, slider) = self.notify.chat_volume_slider.as_ref()?;
        if *slider_chat != chat_id {
            return None;
        }
        let percent = self.account_volumes().chat_slider_percent(chat_id);
        Some(volume_row(
            "chat-volume".to_string(),
            tr(keys::RINGTONES_BOX_VOLUME),
            percent,
            slider,
            cx,
        ))
    }
}

/// A subtitle, the percent and a kit slider (tdesktop `Ui::AddSubsectionTitle`
/// plus `MakeSliderWithLabel`).
fn volume_row(
    id: String,
    title: &str,
    percent: u8,
    slider: &Entity<SliderState>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .id(format!("{id}-row"))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{percent}%")),
                ),
        )
        .child(
            div()
                .id(id)
                .role(Role::Group)
                .aria_label(title.to_string())
                .child(Slider::new(slider)),
        )
        .into_any_element()
}

#[cfg(test)]
mod unit_tests {
    use super::percent_from_slider;

    #[test]
    fn slider_positions_round_into_one_to_hundred() {
        assert_eq!(percent_from_slider(0.0), 1);
        assert_eq!(percent_from_slider(49.6), 50);
        assert_eq!(percent_from_slider(100.0), 100);
        assert_eq!(percent_from_slider(180.0), 100);
        assert_eq!(percent_from_slider(-4.0), 1);
    }
}
