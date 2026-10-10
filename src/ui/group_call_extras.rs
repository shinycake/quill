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
use quill::calls::ptt_global::{HookState, KeyEdge, Sink};
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
        u64::try_from(self.group_call.ptt_clock.elapsed().as_millis()).unwrap_or(u64::MAX)
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
        if !PushToTalk::matches(&config, key) {
            return false;
        }
        self.ptt_press(cx)
    }

    /// The push-to-talk key went down (window or system-wide). Returns
    /// whether it was handled.
    fn ptt_press(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.group_call_joined() {
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
        if let Some(action) = self.group_call.ptt.key_down() {
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
        self.ptt_release(cx);
        true
    }

    /// The push-to-talk key came up (window or system-wide).
    fn ptt_release(&mut self, cx: &mut Context<Self>) {
        let delay = self.ptt_config().release_delay_ms;
        let now = self.ptt_now_ms();
        if let Some(action) = self.group_call.ptt.key_up(now, delay) {
            self.apply_ptt_action(action, cx);
        } else if let Some(deadline) = self.group_call.ptt.mute_deadline() {
            let wait = Duration::from_millis(deadline.saturating_sub(now));
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |this, cx| this.group_call_ptt_tick(cx));
            })
            .detach();
        }
    }

    fn group_call_ptt_tick(&mut self, cx: &mut Context<Self>) {
        let now = self.ptt_now_ms();
        if let Some(action) = self.group_call.ptt.tick(now) {
            self.apply_ptt_action(action, cx);
        }
    }

    /// The voice chat window lost focus (the key-up would never arrive), or
    /// push-to-talk was switched off: drop held state, close the microphone.
    pub(super) fn group_call_ptt_release_all(&mut self, cx: &mut Context<Self>) {
        // With the system-wide hook the key-up still arrives in the background.
        if self.group_call.global_ptt.is_active() {
            return;
        }
        if let Some(action) = self.group_call.ptt.reset()
            && self.group_call_joined()
        {
            self.apply_ptt_action(action, cx);
        }
    }

    /// Settings switched push-to-talk on or off while a call may be live:
    /// on, the microphone closes until the key is held; off, the state resets.
    pub(super) fn sync_ptt_with_call(&mut self, cx: &mut Context<Self>) {
        self.sync_global_ptt(cx);
        let enabled = self.ptt_config().enabled;
        if enabled {
            if self.group_call_joined() && !self.group_call.ptt.is_talking() {
                self.set_group_call_self_mute_to(true, cx);
            }
        } else {
            let _ = self.group_call.ptt.reset();
        }
    }

    /// Run the system-wide key hook exactly while a real group call is
    /// joined with push-to-talk on; tear it down otherwise. Cheap when
    /// nothing changed, so it is safe to call from the render sync.
    pub(super) fn sync_global_ptt(&mut self, cx: &mut Context<Self>) {
        let config = self.ptt_config();
        let want = (self.live.is_some() && config.enabled && self.group_call_joined())
            .then_some(config.key);
        let mut events = None;
        let changed = self.group_call.global_ptt.sync(want.as_deref(), |key| {
            let (tx, rx) = std::sync::mpsc::channel::<KeyEdge>();
            let sink: Sink = std::sync::Arc::new(move |edge| {
                let _ = tx.send(edge);
            });
            let handle = quill::calls::ptt_global::start_platform(key, sink)?;
            events = Some(rx);
            Ok(handle)
        });
        if let Some(rx) = events {
            self.listen_global_ptt(rx, cx);
        }
        if want.is_none() && changed {
            let _ = self.group_call.ptt.reset();
        }
        if matches!(
            self.group_call.global_ptt.state(),
            HookState::NeedsPermission
        ) {
            self.arm_global_ptt_permission_poll(cx);
        }
        if changed {
            cx.notify();
        }
    }

    /// Forward hook-thread events to the app. Blocks one background thread
    /// per event instead of polling; ends when the hook stops.
    fn listen_global_ptt(
        &mut self,
        rx: std::sync::mpsc::Receiver<KeyEdge>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let mut rx = rx;
            loop {
                let (back, edge) = cx
                    .background_executor()
                    .spawn(async move {
                        let edge = rx.recv().ok();
                        (rx, edge)
                    })
                    .await;
                rx = back;
                let Some(edge) = edge else { break };
                if this
                    .update(cx, |this, cx| this.global_ptt_edge(edge, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn global_ptt_edge(&mut self, edge: KeyEdge, cx: &mut Context<Self>) {
        if !self.ptt_config().enabled {
            return;
        }
        match edge {
            KeyEdge::Down => {
                self.ptt_press(cx);
            }
            KeyEdge::Up => self.ptt_release(cx),
        }
    }

    /// Input Monitoring changes need no restart to be noticed by a fresh
    /// check, so look again every couple of seconds while it is missing.
    fn arm_global_ptt_permission_poll(&mut self, cx: &mut Context<Self>) {
        if self.group_call.global_ptt_polling {
            return;
        }
        self.group_call.global_ptt_polling = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let _ = this.update(cx, |this, cx| {
                this.group_call.global_ptt_polling = false;
                if quill::calls::ptt_global::permission_granted() {
                    this.recheck_global_ptt(cx);
                } else if matches!(
                    this.group_call.global_ptt.state(),
                    HookState::NeedsPermission
                ) {
                    this.arm_global_ptt_permission_poll(cx);
                }
            });
        })
        .detach();
    }

    /// "Check again" in Settings: allow a failed start to be retried.
    pub(super) fn recheck_global_ptt(&mut self, cx: &mut Context<Self>) {
        self.group_call.global_ptt.retry();
        self.sync_global_ptt(cx);
        cx.notify();
    }

    /// Bind the next pressed key as the push-to-talk key (Settings).
    pub(super) fn capture_ptt_key(&mut self, key: &str, cx: &mut Context<Self>) {
        self.group_call.ptt_capture = false;
        if key == "escape" {
            cx.notify();
            return;
        }
        let key = key.to_ascii_lowercase();
        self.set_call_pref(|prefs| prefs.push_to_talk.key = key, cx);
    }

    /// Pin or unpin a video tile (local only, like tdesktop).
    pub(super) fn toggle_group_call_tile_pin(&mut self, key: TileKey, cx: &mut Context<Self>) {
        self.group_call.pin.toggle(key);
        cx.notify();
    }

    /// Show the pinned stream across the whole window (the tile's
    /// full-screen button).
    pub(super) fn enter_group_call_stage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        cx.notify();
    }

    /// Leave full screen (the stage's button, or Esc).
    pub(super) fn leave_group_call_stage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        self.demo_ui.group_stage = false;
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
                self.connection.status_note = "Couldn't change who you join as.".into();
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
        let capturing = self.group_call.ptt_capture;
        let needs_permission = config.enabled
            && (matches!(
                self.group_call.global_ptt.state(),
                HookState::NeedsPermission
            ) || !quill::calls::ptt_global::permission_granted());
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
                            this.group_call.ptt_capture = false;
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
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Shortcut"),
                        )
                        .child(
                            Button::new("call-pref-ptt-key")
                                .label(if capturing {
                                    "Press a key…".to_string()
                                } else {
                                    ptt_key_label(&config.key)
                                })
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.group_call.ptt_capture = !this.group_call.ptt_capture;
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
                            Radio::new(format!("call-pref-ptt-delay-{ms}"))
                                .label(format!("{ms} ms"))
                        }))
                        .on_click(cx.listener(move |this, &ix, _, cx| {
                            let ms = delays[ix];
                            this.set_call_pref(
                                |prefs| prefs.push_to_talk.release_delay_ms = ms,
                                cx,
                            );
                        })),
                )
                .child(div().text_xs().text_color(muted).px_2().child(
                    quill::calls::ptt_global::status_note(
                        self.group_call.global_ptt.state(),
                        quill::calls::ptt_global::static_limit().as_deref(),
                    ),
                ));
            // tdesktop shows this box when enabling the shortcut without
            // access; here it sits under the shortcut instead of a dialog.
            if needs_permission {
                section = section.child(
                    div()
                        .flex()
                        .gap_2()
                        .px_2()
                        .child(
                            Button::new("call-pref-ptt-open-settings")
                                .label("Open Settings")
                                .small()
                                .on_click(cx.listener(|_, _, _, _| {
                                    quill::calls::ptt_global::open_permission_settings();
                                })),
                        )
                        .child(
                            Button::new("call-pref-ptt-recheck")
                                .label("Check again")
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.recheck_global_ptt(cx);
                                })),
                        ),
                );
            }
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
