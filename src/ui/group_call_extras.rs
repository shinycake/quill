//! Group-call polish: push-to-talk, pinned tiles and "join as".
//!
//! tdesktop: `calls/group/calls_group_call.cpp` (push-to-talk),
//! `calls_group_viewport.cpp` (pinned video), `calls_group_panel.cpp` +
//! `calls_group_common.cpp` (join-as box). The decisions live in the pure
//! modules `quill::calls::{ptt, tile_pin}`; this file only wires them to
//! the window and to the call driver.

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;
use quill::calls::ptt::{PttAction, PttConfig, PushToTalk};
use quill::calls::tile_pin::TileKey;
use quill::telegram::envelope::MessageSender;
use std::time::Duration;

impl QuillApp {
    fn ptt_config(&self) -> PttConfig {
        self.session()
            .map(|s| s.call_prefs.push_to_talk.sanitized())
            .unwrap_or_default()
    }

    fn ptt_now_ms(&self) -> u64 {
        u64::try_from(self.ptt_clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn group_call_joined(&self) -> bool {
        self.session()
            .and_then(|s| s.active_group_call.as_ref())
            .is_some_and(|call| call.is_joined)
    }

    /// Mute or unmute the microphone without a toggle round trip.
    pub(super) fn set_group_call_self_mute_to(&mut self, muted: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.set_group_call_self_mute(muted);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.set_group_call_self_muted(muted);
        }
        cx.notify();
    }

    fn apply_ptt_action(&mut self, action: PttAction, cx: &mut Context<Self>) {
        self.set_group_call_self_mute_to(matches!(action, PttAction::Mute), cx);
    }

    /// A key went down in the voice chat window. Returns whether it was the
    /// push-to-talk key (so the caller stops other handling of it).
    pub(super) fn group_call_key_down(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let config = self.ptt_config();
        if !PushToTalk::matches(&config, key) || !self.group_call_joined() {
            return false;
        }
        // An admin-muted participant cannot open the microphone by key.
        let forced = self
            .session()
            .and_then(|s| s.active_group_call.as_ref())
            .and_then(|call| call.participants.iter().find(|p| p.is_current_user))
            .is_some_and(|me| me.is_muted_for_all_users && !me.can_unmute_self);
        if forced {
            return false;
        }
        if let Some(action) = self.group_call_ptt.key_down() {
            self.apply_ptt_action(action, cx);
        }
        true
    }

    /// The matching key came up: mute after the configured release delay.
    pub(super) fn group_call_key_up(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let config = self.ptt_config();
        if !PushToTalk::matches(&config, key) {
            return false;
        }
        let delay = config.release_delay_ms;
        let now = self.ptt_now_ms();
        if let Some(action) = self.group_call_ptt.key_up(now, delay) {
            self.apply_ptt_action(action, cx);
        } else if let Some(deadline) = self.group_call_ptt.mute_deadline() {
            let wait = Duration::from_millis(deadline.saturating_sub(now));
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |this, cx| this.group_call_ptt_tick(cx));
            })
            .detach();
        }
        true
    }

    fn group_call_ptt_tick(&mut self, cx: &mut Context<Self>) {
        let now = self.ptt_now_ms();
        if let Some(action) = self.group_call_ptt.tick(now) {
            self.apply_ptt_action(action, cx);
        }
    }

    /// The voice chat window lost focus (the key-up would never arrive), or
    /// push-to-talk was switched off: drop held state, close the microphone.
    pub(super) fn group_call_ptt_release_all(&mut self, cx: &mut Context<Self>) {
        if let Some(action) = self.group_call_ptt.reset()
            && self.group_call_joined()
        {
            self.apply_ptt_action(action, cx);
        }
    }

    /// Settings switched push-to-talk on or off while a call may be live:
    /// on, the microphone closes until the key is held; off, the state resets.
    pub(super) fn sync_ptt_with_call(&mut self, cx: &mut Context<Self>) {
        let enabled = self.ptt_config().enabled;
        if enabled {
            if self.group_call_joined() && !self.group_call_ptt.is_talking() {
                self.set_group_call_self_mute_to(true, cx);
            }
        } else {
            let _ = self.group_call_ptt.reset();
        }
    }

    /// Bind the next pressed key as the push-to-talk key (Settings).
    pub(super) fn capture_ptt_key(&mut self, key: &str, cx: &mut Context<Self>) {
        self.ptt_capture = false;
        if key == "escape" {
            cx.notify();
            return;
        }
        let key = key.to_ascii_lowercase();
        self.set_call_pref(|prefs| prefs.push_to_talk.key = key, cx);
    }

    /// Pin or unpin a video tile (local only, like tdesktop).
    pub(super) fn toggle_group_call_tile_pin(&mut self, key: TileKey, cx: &mut Context<Self>) {
        self.group_call_pin.toggle(key);
        cx.notify();
    }

    /// Choose the identity to join as, and remember it for the chat.
    pub(super) fn choose_group_call_join_as(
        &mut self,
        sender: MessageSender,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.choose_group_call_join_as(sender).is_err() {
                self.status_note = "Couldn't change who you join as.".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.set_group_call_join_as(Some(sender));
        }
        cx.notify();
    }

    /// Display name of the identity the join will use.
    pub(super) fn join_as_label(&self, sender: Option<MessageSender>) -> String {
        match sender {
            Some(sender) => self.group_call_participant_name(&sender),
            None => "Yourself".into(),
        }
    }

    /// Settings rows for push-to-talk: switch, shortcut, release delay.
    pub(super) fn push_to_talk_settings(
        &self,
        config: &PttConfig,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let delays = quill::calls::ptt::RELEASE_DELAYS_MS;
        let config = config.sanitized();
        let selected_delay = delays.iter().position(|d| *d == config.release_delay_ms);
        let capturing = self.ptt_capture;
        let mut section = div().flex().flex_col().gap_1().child(
            div()
                .id("call-pref-ptt")
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .child(
                    Switch::new("call-pref-ptt-switch")
                        .checked(config.enabled)
                        .accessibility_label("Push-to-talk")
                        .on_click(cx.listener(|this, &on, _, cx| {
                            this.ptt_capture = false;
                            this.set_call_pref(|prefs| prefs.push_to_talk.enabled = on, cx);
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().text_sm().child("Push-to-talk"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child("Hold a key to talk in voice chats"),
                        ),
                ),
        );
        if config.enabled {
            section = section
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("Shortcut"))
                        .child(
                            Button::new("call-pref-ptt-key")
                                .label(if capturing {
                                    "Press a key…".to_string()
                                } else {
                                    ptt_key_label(&config.key)
                                })
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.ptt_capture = !this.ptt_capture;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .px_2()
                        .pt_1()
                        .child("Release delay"),
                )
                .child(
                    RadioGroup::vertical("call-pref-ptt-delay")
                        .selected_index(selected_delay)
                        .children(delays.iter().map(|ms| {
                            Radio::new(format!("call-pref-ptt-delay-{ms}")).label(format!("{ms} ms"))
                        }))
                        .on_click(cx.listener(move |this, &ix, _, cx| {
                            let ms = delays[ix];
                            this.set_call_pref(|prefs| prefs.push_to_talk.release_delay_ms = ms, cx);
                        })),
                )
                .child(div().text_xs().text_color(muted).px_2().child(
                    "Works while the voice chat window is in front. System-wide shortcuts are not available yet.",
                ));
        }
        section.into_any_element()
    }

    /// "Hold Space to talk" while push-to-talk is on.
    pub(super) fn ptt_hint(&self) -> Option<String> {
        let config = self.ptt_config();
        config
            .enabled
            .then(|| format!("Hold {} to talk", ptt_key_label(&config.key)))
    }

    /// The pre-join "Join as" picker (tdesktop's join-as box): shown on an
    /// unjoined chat-bound call that offers more than one identity.
    pub(super) fn group_call_join_as_row(
        &self,
        call: &quill::state::ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if call.is_joined || call.join_as_options.len() < 2 {
            return None;
        }
        let selected = call.join_as;
        let owner = cx.entity().downgrade();
        let names: Vec<(MessageSender, String)> = call
            .join_as_options
            .iter()
            .map(|sender| (*sender, self.group_call_participant_name(sender)))
            .collect();
        // Nothing picked yet means the server joins as the current user.
        let current = match selected {
            Some(sender) => self.group_call_participant_name(&sender),
            None => names
                .iter()
                .find(|(s, _)| matches!(s, MessageSender::User { .. }))
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| self.join_as_label(None)),
        };
        Some(
            div()
                .mx(px(16.))
                .mb(px(8.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(0x91979e))
                        .child("Join as"),
                )
                .child(
                    Button::new("group-call-join-as")
                        .label(current)
                        .icon(IconName::ChevronDown)
                        .ghost()
                        .small()
                        .text_color(rgb(0x4db8ff))
                        .dropdown_menu(move |mut menu, _, _| {
                            for (sender, name) in &names {
                                let (sender, owner) = (*sender, owner.clone());
                                menu = menu.item(
                                    PopupMenuItem::new(name.clone())
                                        .checked(selected == Some(sender))
                                        .on_click(move |_, _, cx| {
                                            let _ = owner.update(cx, |this, cx| {
                                                this.choose_group_call_join_as(sender, cx)
                                            });
                                        }),
                                );
                            }
                            menu
                        }),
                )
                .into_any_element(),
        )
    }
}

/// Human label for a GPUI key name.
pub(super) fn ptt_key_label(key: &str) -> String {
    let is_function_key =
        key.len() > 1 && key.starts_with('f') && key[1..].chars().all(|c| c.is_ascii_digit());
    if key.chars().count() <= 1 || is_function_key {
        return key.to_uppercase();
    }
    match key {
        "escape" => "Esc".into(),
        _ => {
            let mut chars = key.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ptt_key_label;

    #[test]
    fn key_names_read_naturally() {
        assert_eq!(ptt_key_label("space"), "Space");
        assert_eq!(ptt_key_label("f13"), "F13");
        assert_eq!(ptt_key_label("t"), "T");
        assert_eq!(ptt_key_label("backspace"), "Backspace");
    }
}
