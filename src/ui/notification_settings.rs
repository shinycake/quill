//! mute menu, notification sounds, badge prefs, scope/reaction settings.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_media_allowlist};
use super::notifications::notification_settings_json;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::connect::SoundResolution;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::notify::NotificationSoundKind;
use quill::settings::BadgePrefs;
use quill::state::{ChatSummary, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    ChatKind, ChatNotificationSettings, MUTE_FOR_1_HOUR, MUTE_FOR_2_DAYS, MUTE_FOR_8_HOURS,
    MUTE_FOREVER, NotificationSettingsScope, NotificationSound, ReactionNotificationSettings,
    ReactionNotificationSource, ScopeNotificationSettings,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// Phase 8.1: cap on concurrent OS-notification worker threads (`notify-send
/// --wait` blocks until dismissal). Excess bursts are dropped, not stacked.
pub(super) const MAX_OS_NOTIFICATION_THREADS: usize = 8;

/// Parity slice: cap for concurrent `quill-sound` player threads.
pub(super) const MAX_OS_NOTIFICATION_SOUND_THREADS: usize = 2;

/// Parity slice: which settings object a sound-picker choice applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SoundPickerTarget {
    Chat(ChatId),
    Scope(NotificationSettingsScope),
    /// Parity slice: `reactionNotificationSettings` (TDLib 1.8.67, line 3396).
    Reaction,
}

/// Parity slice: a sound choice in the picker UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SoundChoice {
    /// App default tone (chat `use_default_sound`, scope `sound_id = -1`).
    Default,
    /// No sound (chat/scope `sound_id = 0`).
    Disabled,
    /// A saved notification sound id.
    Custom(i64),
}

/// Parity slice: which reaction-notification source a preset row applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReactionSourceKind {
    Message,
    Story,
    PollVote,
}

/// Parity slice: notification-sounds screenshot fixture — saved sounds
/// (`getSavedNotificationSounds` answer), all three scope defaults
/// (`getScopeNotificationSettings` answers, correlated via
/// `request_for_scope`), and chat 11 on a custom saved sound.
pub(super) fn apply_ready_notification_sound(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let sound_path = demo_media_allowlist()
        .join("demo-voice.ogg")
        .to_string_lossy()
        .into_owned();
    let sound_json = |id: i64, file_id: i32, title: &str, duration: i32| {
        format!(
            r#"{{"@type":"notificationSound","id":{id},"duration":{duration},"date":0,"title":{title},"data":"","sound":{file}}}"#,
            file = demo_file_json(file_id, &sound_path, true),
            title = serde_json::to_string(title).unwrap(),
        )
    };
    let sounds_extra = session.request(RequestPurpose::GetSavedNotificationSounds, None);
    let mut jsons = vec![
        format!(
            r#"{{"@type":"notificationSounds","notification_sounds":[{a},{b}],"@extra":"{extra}"}}"#,
            a = sound_json(1, 91, "Ding", 2),
            b = sound_json(2, 92, "Chime", 3),
            extra = sounds_extra.0,
        ),
        {
            let chat_settings = ChatNotificationSettings {
                use_default_mute_for: true,
                mute_for: 0,
                use_default_sound: false,
                sound_id: 1,
                use_default_show_preview: false,
                show_preview: true,
                ..Default::default()
            };
            format!(
                r#"{{"@type":"updateChatNotificationSettings","chat_id":11,"notification_settings":{}}}"#,
                notification_settings_json(&chat_settings)
            )
        },
    ];
    for scope in NotificationSettingsScope::ALL {
        let extra = session.request_for_scope(RequestPurpose::GetScopeNotificationSettings, scope);
        jsons.push(format!(
            r#"{{"@type":"scopeNotificationSettings","mute_for":0,"sound_id":"-1","show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"-1","use_default_show_story_poster":true,"show_story_poster":true,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false,"@extra":"{extra}"}}"#,
            extra = extra.0,
        ));
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    // Parity slice: answer `getChatNotificationSettingsExceptions` per
    // scope from the demo chats' own settings, through the real
    // request/response correlation — chat 11's custom settings above make
    // it a PrivateChats exception.
    for scope in NotificationSettingsScope::ALL {
        let extra =
            session.request_for_scope(RequestPurpose::GetChatNotificationSettingsExceptions, scope);
        let ids = session.local_notification_exceptions(scope);
        let json = format!(
            r#"{{"@type":"chats","total_count":{n},"chat_ids":[{ids}],"@extra":"{extra}"}}"#,
            n = ids.len(),
            ids = ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(","),
            extra = extra.0,
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    /// Slice parity:chatlist-badge-settings: update one badge-counter
    /// pref in the session and persist it to the account dir (via
    /// `ConnectDriver::save_badge_prefs`).
    pub(super) fn set_badge_pref(
        &mut self,
        update: impl FnOnce(&mut BadgePrefs),
        cx: &mut Context<Self>,
    ) {
        let mut prefs = self
            .session()
            .map(|session| session.badge_prefs)
            .unwrap_or_default();
        update(&mut prefs);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.badge_prefs = prefs;
            if let Err(err) = live.driver.save_badge_prefs() {
                self.status_note = format!("couldn’t save badge settings: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.badge_prefs = prefs;
            self.status_note = "demo: badge settings are not saved".into();
        }
        cx.notify();
    }

    /// Parity slice: in-app notification sounds toggle (tdesktop "Play
    /// sounds"). Writes through to prefs (persist) and updates the
    /// Session mirror immediately.
    pub(super) fn set_inapp_sounds_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.inapp_sounds_enabled = on;
            if let Err(err) = live.driver.save_inapp_sounds_enabled() {
                self.status_note = format!("couldn’t save notification sounds: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.inapp_sounds_enabled = on;
            self.status_note = "demo: in-app sounds are not saved".into();
        }
        cx.notify();
    }

    /// kit Phase 2 (redo): notification defaults hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_notification_defaults_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::NotificationDefaults,
            |this, _, cx| {
                this.notification_defaults_open = false;
                this.defaults_sound_picker = None;
                this.defaults_exceptions_scope = None;
                cx.notify();
            },
        );
        app.update(cx, |this, cx| {
            let session = this.session();
            let saved_sounds: Vec<NotificationSound> = session
                .as_ref()
                .map(|s| s.saved_notification_sounds.clone())
                .unwrap_or_default();
            let mut body = div().flex().flex_col().gap_3();
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Used when a chat keeps the default setting. \
                         Changes apply via setScopeNotificationSettings.",
                    ),
            );
            for scope in NotificationSettingsScope::ALL {
                body = body.child(this.scope_settings_section(cx, scope, &saved_sounds));
            }
            // Parity slice: reaction + poll-vote notification settings
            // (`setReactionNotificationSettings`).
            body = body.child(this.reaction_settings_section(cx, &saved_sounds));
            // Slice parity:chatlist-badge-settings: app badge counter
            // preferences (include muted/archived, messages vs chats).
            body = body.child(this.badge_counter_section(cx));
            // Parity slice: in-app notification sounds (tdesktop "Play
            // sounds") — the client-side toggle gating
            // `Session::notification_sound_for`.
            body = body.child(this.inapp_sounds_section(cx));
            let footer = div().flex().justify_end().child(
                Button::new("close-notif-defaults")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.notification_defaults_open = false;
                        this.defaults_sound_picker = None;
                        this.defaults_exceptions_scope = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::NotificationDefaults, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title("Notification defaults")
                .content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                })
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Slice parity:chatlist-badge-settings: one labeled kit Switch row
    /// for the app badge counter section.
    pub(super) fn badge_switch_row(
        &self,
        cx: &mut Context<Self>,
        id: &str,
        label: &str,
        checked: bool,
        apply: fn(&mut BadgePrefs, bool),
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.to_owned()),
            )
            .child(
                Switch::new(format!("badge-{id}"))
                    .checked(checked)
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        this.set_badge_pref(|p| apply(p, on), cx);
                    })),
            )
            .into_any_element()
    }

    /// Slice parity:chatlist-badge-settings: app badge counter
    /// preferences (include muted / include archived / messages vs
    /// chats) as kit Switch rows in the notification defaults dialog.
    /// Toggling persists via `set_badge_pref`; the tray picks the new
    /// count up on its next 1s sync.
    pub(super) fn badge_counter_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = self.session().map(|s| s.badge_prefs).unwrap_or_default();
        div()
            .id("badge-counter-section")
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
                    .child(div().font_semibold().text_sm().child("App badge counter"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Tray icon badge"),
                    ),
            )
            .child(self.badge_switch_row(
                cx,
                "include-muted",
                "Include muted chats",
                prefs.include_muted,
                |p, on| p.include_muted = on,
            ))
            .child(self.badge_switch_row(
                cx,
                "include-archived",
                "Include archived chats",
                prefs.include_archived,
                |p, on| p.include_archived = on,
            ))
            .child(self.badge_switch_row(
                cx,
                "count-messages",
                "Count unread messages (off: count chats)",
                prefs.count_messages,
                |p, on| p.count_messages = on,
            ))
            .into_any_element()
    }

    /// Parity slice: in-app notification sounds (tdesktop "Play sounds")
    /// as a single kit Switch row in the notification defaults dialog.
    /// Toggling writes through to `prefs.json` and updates the Session
    /// mirror so the next notification's sound decision sees it.
    pub(super) fn inapp_sounds_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled = self
            .session()
            .map(|s| s.inapp_sounds_enabled)
            .unwrap_or(true);
        div()
            .id("inapp-sounds-section")
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
                    .child(div().font_semibold().text_sm().child("In-app sounds"))
                    .child(
                        Switch::new("inapp-sounds-toggle")
                            .checked(enabled)
                            .accessibility_label("Play sounds")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.set_inapp_sounds_enabled(on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Play sounds while using the app"),
            )
            .into_any_element()
    }

    pub(super) fn close_mute_menu(&mut self, cx: &mut Context<Self>) {
        self.mute_menu_open = false;
        self.notif_sound_picker_open = false;
        cx.notify();
    }

    pub(super) fn open_mute_menu(&mut self, cx: &mut Context<Self>) {
        self.mute_menu_open = true;
        self.status_note = "mute for…".into();
        cx.notify();
    }

    pub(super) fn apply_chat_mute(
        &mut self,
        chat_id: ChatId,
        mute_for: i32,
        cx: &mut Context<Self>,
    ) {
        self.mute_menu_open = false;
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_mute_for(chat_id, mute_for);
            self.status_note = match result {
                Ok(_) if mute_for == 0 => "unmuting…".into(),
                Ok(_) => "muting…".into(),
                Err(_) => "could not change mute".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    *settings = settings.clone().with_mute_for(mute_for);
                },
                cx,
            );
            self.status_note = if mute_for == 0 {
                "unmuted".into()
            } else {
                "muted".into()
            };
            cx.notify();
        }
    }

    /// Parity slice: the tdesktop "Mute" submenu is now a per-chat
    /// notification settings panel — mute presets, message-preview toggle,
    /// notification-sound picker (`getSavedNotificationSounds`), and a link
    /// to the scope defaults dialog. Current state shows in the summary line.
    ///
    /// Phase S1: secret chats additionally show the TGX
    /// `NotificationChannelSecretChat` label with the peer's name.
    pub(super) fn mute_menu_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open_chat = session.as_ref().and_then(|s| s.open_chat);
        let open_chat_summary: Option<&ChatSummary> =
            open_chat.and_then(|id| session.as_ref()?.chats.get(&id.0));
        let chat_settings: ChatNotificationSettings = open_chat_summary
            .map(|chat| chat.notification_settings.clone())
            .unwrap_or_default();
        let muted = open_chat_summary
            .and_then(|chat| session.as_ref().map(|s| s.effective_muted(chat)))
            .unwrap_or_else(|| chat_settings.is_muted());
        let preview_on = open_chat_summary
            .and_then(|chat| session.as_ref().map(|s| s.effective_preview_allowed(chat)))
            .unwrap_or(chat_settings.use_default_show_preview || chat_settings.show_preview);
        let sound_label = self.notification_sound_label(&chat_settings);
        let saved_sounds: Vec<NotificationSound> = session
            .as_ref()
            .map(|s| s.saved_notification_sounds.clone())
            .unwrap_or_default();
        let status = format!(
            "{} \u{b7} Sound: {} \u{b7} Previews: {}",
            if muted {
                if chat_settings.is_muted_forever() {
                    "Muted forever"
                } else {
                    "Muted"
                }
            } else {
                "Unmuted"
            },
            sound_label,
            if preview_on { "on" } else { "off" },
        );

        let presets = [
            ("1 hour", MUTE_FOR_1_HOUR),
            ("8 hours", MUTE_FOR_8_HOURS),
            ("2 days", MUTE_FOR_2_DAYS),
            ("Forever", MUTE_FOREVER),
        ];
        let mut preset_row = div().id("mute-presets").flex().flex_wrap().gap_1();
        for (label, seconds) in presets {
            preset_row = preset_row.child(
                Button::new(format!("mute-for-{seconds}"))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(chat_id) = open_chat {
                            this.apply_chat_mute(chat_id, seconds, cx);
                        }
                    })),
            );
        }

        let mut panel = div()
            .id("mute-menu")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("Notifications"))
                    .child(
                        Button::new("close-mute-menu")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_mute_menu(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
            // Phase S1: TGX `NotificationChannelSecretChat` — secret chats
            // get their own custom-settings label with the peer's name.
            .when_some(Self::secret_notif_label(session), |this, label| {
                this.child(div().text_xs().text_color(cx.theme().accent).child(label))
            })
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Mute for"),
            )
            .child(preset_row)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Show message preview in notifications"),
                    )
                    .child(
                        // Phase 6: kit Switch (was: On/Off ghost button).
                        Switch::new("notif-preview-toggle")
                            .checked(preview_on)
                            .accessibility_label("Show message preview in notifications")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                if let Some(chat_id) = open_chat {
                                    this.apply_chat_preview(chat_id, on, cx);
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Notification sound: {sound_label}")),
                    )
                    .child(
                        Button::new("notif-sound-picker-toggle")
                            .label(if self.notif_sound_picker_open {
                                "Hide"
                            } else {
                                "Change"
                            })
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notif_sound_picker_open = !this.notif_sound_picker_open;
                                if this.notif_sound_picker_open
                                    && let Some(live) = this.live.as_mut()
                                {
                                    let _ = live.driver.maybe_fetch_notification_sounds();
                                }
                                cx.notify();
                            })),
                    ),
            );
        if self.notif_sound_picker_open
            && let Some(chat_id) = open_chat
        {
            let current = if chat_settings.use_default_sound {
                SoundChoice::Default
            } else if chat_settings.sound_id == 0 {
                SoundChoice::Disabled
            } else {
                SoundChoice::Custom(chat_settings.sound_id)
            };
            panel = panel.child(self.notification_sound_picker(
                cx,
                SoundPickerTarget::Chat(chat_id),
                current,
                &saved_sounds,
            ));
        }
        panel.child(
            Button::new("notif-open-defaults")
                .label("Defaults for all chats\u{2026}")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notification_defaults_open = true;
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.maybe_fetch_scope_notification_settings();
                        let _ = live.driver.maybe_fetch_notification_sounds();
                        for scope in NotificationSettingsScope::ALL {
                            let _ = live.driver.maybe_fetch_notification_exceptions(scope);
                        }
                    }
                    cx.notify();
                })),
        )
    }

    /// Phase B4: self-destruct / auto-delete timer picker below the
    /// conversation header. The picker is secret-chat-only (the header
    /// button is gated), so only the secret presets are reachable today;
    /// the non-secret day-multiple branch below is defensive, kept for
    /// the planned regular-chat picker follow-up
    /// (`setChatMessageAutoDeleteTime`, schema 1.8.67 line 13454).
    pub(super) fn ttl_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open_chat = session.as_ref().and_then(|s| s.open_chat);
        let open_chat_summary: Option<&ChatSummary> =
            open_chat.and_then(|id| session.as_ref()?.chats.get(&id.0));
        let is_secret =
            open_chat_summary.is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        let current = open_chat_summary
            .and_then(|chat| chat.ttl_status_line())
            .unwrap_or_else(|| "Off".to_string());
        let title = if is_secret {
            "Self-destruct timer"
        } else {
            "Auto-delete timer"
        };
        // Schema value rule (1.8.67 line 13454): secret chats accept
        // arbitrary seconds; other chats need day multiples.
        let presets: &[(&str, i32)] = if is_secret {
            &[
                ("Off", 0),
                ("5s", 5),
                ("30s", 30),
                ("1m", 60),
                ("1h", 3600),
                ("1d", 86400),
                ("1w", 604800),
            ]
        } else {
            &[("Off", 0), ("1d", 86400), ("1w", 604800), ("30d", 2592000)]
        };
        // Phase 6: kit RadioGroup (was: buttons with a ● prefix on the
        // active preset). Controlled: the chosen index writes the value.
        let active_ix = presets.iter().position(|(_, secs)| {
            open_chat_summary.is_some_and(|chat| chat.message_auto_delete_time == *secs)
        });
        let preset_row = RadioGroup::horizontal("ttl-presets")
            .selected_index(active_ix)
            .children(
                presets
                    .iter()
                    .map(|(label, _)| Radio::new(format!("ttl-set-{label}")).label(*label)),
            )
            .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                let secs = presets[ix].1;
                if let Some(chat_id) = open_chat {
                    this.apply_chat_ttl(chat_id, secs, cx);
                }
            }));
        div()
            .id("ttl-picker")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child(title))
                    .child(
                        Button::new("close-ttl-picker")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ttl_picker_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Current: {current} — messages auto-delete after the timer; \
                         in secret chats the countdown starts once the message is viewed."
                    )),
            )
            .child(preset_row)
    }

    /// Parity slice: current sound choice for a chat, for the notifications
    /// panel summary and picker checkmarks.
    pub(super) fn notification_sound_label(&self, settings: &ChatNotificationSettings) -> String {
        if settings.use_default_sound {
            return "Default".to_string();
        }
        if settings.sound_id == 0 {
            return "None".to_string();
        }
        self.session()
            .and_then(|s| {
                s.saved_notification_sounds
                    .iter()
                    .find(|sound| sound.id == settings.sound_id)
                    .map(|sound| sound.title.clone())
            })
            .unwrap_or_else(|| "Custom".to_string())
    }

    /// Parity slice: apply a sound choice to the open chat
    /// (`setChatNotificationSettings`).
    pub(super) fn apply_chat_sound(
        &mut self,
        chat_id: ChatId,
        use_default_sound: bool,
        sound_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.notif_sound_picker_open = false;
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.set_chat_sound(
                chat_id,
                use_default_sound,
                sound_id,
            );
            self.status_note = match result {
                Ok(_) => "sound updated…".into(),
                Err(_) => "could not change sound".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    settings.use_default_sound = use_default_sound;
                    settings.sound_id = sound_id;
                },
                cx,
            );
            self.status_note = "sound updated".into();
            cx.notify();
        }
    }

    /// Parity slice: apply a message-preview exception to the open chat
    /// (`setChatNotificationSettings`).
    pub(super) fn apply_chat_preview(
        &mut self,
        chat_id: ChatId,
        show_preview: bool,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_show_preview(chat_id, show_preview);
            self.status_note = match result {
                Ok(_) => "preview setting updated…".into(),
                Err(_) => "could not change preview".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    settings.use_default_show_preview = false;
                    settings.show_preview = show_preview;
                },
                cx,
            );
            self.status_note = "preview setting updated".into();
            cx.notify();
        }
    }

    /// Parity slice: preview a saved notification sound immediately — play
    /// the MP3 when local, otherwise download it and play on completion
    /// (via `Session::pending_sound_plays`).
    pub(super) fn preview_saved_sound(&mut self, sound_id: i64) {
        if let Some(live) = self.live.as_mut() {
            match live
                .driver
                .resolve_notification_sound(NotificationSoundKind::Custom(sound_id))
            {
                SoundResolution::DefaultTone => {
                    if let Some(command) = quill::notify::default_tone_command() {
                        self.spawn_sound_command(command);
                    }
                }
                SoundResolution::FilePath(path) => {
                    if let Some(command) =
                        quill::notify::file_sound_command(&path.to_string_lossy())
                    {
                        self.spawn_sound_command(command);
                    }
                }
                SoundResolution::Pending => {}
            }
        } else if let Some(command) = quill::notify::default_tone_command() {
            // Screenshot demo: no TDLib files exist — the tone stands in.
            self.spawn_sound_command(command);
        }
    }

    /// Parity slice: the saved-sound picker shared by the per-chat panel and
    /// the scope defaults dialog. `getSavedNotificationSounds` says: "If a
    /// sound isn't in the list, then default sound needs to be used" — so
    /// Default / None are always offered first.
    pub(super) fn notification_sound_picker(
        &self,
        cx: &mut Context<Self>,
        target: SoundPickerTarget,
        current: SoundChoice,
        saved_sounds: &[NotificationSound],
    ) -> AnyElement {
        let list_id = match target {
            SoundPickerTarget::Chat(_) => "notif-sound-list".to_string(),
            SoundPickerTarget::Scope(scope) => format!("scope-sound-list-{scope:?}"),
            SoundPickerTarget::Reaction => "reaction-sound-list".to_string(),
        };
        let mut list = div().id(list_id).flex().flex_col().gap_1().py_1();
        list = list.child(self.sound_picker_row(
            cx,
            target,
            SoundChoice::Default,
            current,
            "Default",
            "Telegram default tone",
            None,
        ));
        list = list.child(self.sound_picker_row(
            cx,
            target,
            SoundChoice::Disabled,
            current,
            "None",
            "No sound",
            None,
        ));
        for sound in saved_sounds {
            let subtitle = format!("{} ({}s)", sound.title, sound.duration.max(0));
            list = list.child(self.sound_picker_row(
                cx,
                target,
                SoundChoice::Custom(sound.id),
                current,
                &sound.title,
                &subtitle,
                Some(sound.id),
            ));
        }
        list.into_any_element()
    }

    /// Parity slice: one row of the sound picker. The `▶` preview button
    /// plays the sound without selecting it.
    pub(super) fn sound_picker_row(
        &self,
        cx: &mut Context<Self>,
        target: SoundPickerTarget,
        choice: SoundChoice,
        current: SoundChoice,
        title: &str,
        subtitle: &str,
        preview_sound_id: Option<i64>,
    ) -> AnyElement {
        let selected = choice == current;
        let row_id = match (target, choice) {
            (SoundPickerTarget::Chat(_), SoundChoice::Default) => "sound-pick-chat-default",
            (SoundPickerTarget::Chat(_), SoundChoice::Disabled) => "sound-pick-chat-none",
            (SoundPickerTarget::Chat(_), SoundChoice::Custom(_)) => "sound-pick-chat-custom",
            (SoundPickerTarget::Scope(_), SoundChoice::Default) => "sound-pick-scope-default",
            (SoundPickerTarget::Scope(_), SoundChoice::Disabled) => "sound-pick-scope-none",
            (SoundPickerTarget::Scope(_), SoundChoice::Custom(_)) => "sound-pick-scope-custom",
            (SoundPickerTarget::Reaction, SoundChoice::Default) => "sound-pick-reaction-default",
            (SoundPickerTarget::Reaction, SoundChoice::Disabled) => "sound-pick-reaction-none",
            (SoundPickerTarget::Reaction, SoundChoice::Custom(_)) => "sound-pick-reaction-custom",
        };
        let mut row = div()
            .id(format!("{row_id}-row"))
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent.opacity(0.08)))
            .child(
                // Phase 6: kit Radio as the selection indicator (was: a "✓ "
                // prefix on the title). The whole row stays the click
                // target, exactly as before — the radio is display-only.
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Radio::new(format!("{row_id}-radio")).checked(selected))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().child(title.to_string()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(subtitle.to_string()),
                            ),
                    ),
            );
        if let Some(sound_id) = preview_sound_id {
            row = row.child(
                Button::new((row_id, sound_id as u64))
                    .label("▶")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, _| {
                        this.preview_saved_sound(sound_id);
                    })),
            );
        }
        row.on_click(cx.listener(move |this, _, _, cx| {
            this.apply_sound_choice(target, choice, cx);
        }))
        .into_any_element()
    }

    /// Parity slice: apply a picker choice to its target.
    pub(super) fn apply_sound_choice(
        &mut self,
        target: SoundPickerTarget,
        choice: SoundChoice,
        cx: &mut Context<Self>,
    ) {
        match target {
            SoundPickerTarget::Chat(chat_id) => {
                let (use_default_sound, sound_id) = match choice {
                    SoundChoice::Default => (true, 0),
                    SoundChoice::Disabled => (false, 0),
                    SoundChoice::Custom(id) => (false, id),
                };
                self.apply_chat_sound(chat_id, use_default_sound, sound_id, cx);
            }
            SoundPickerTarget::Scope(scope) => {
                let sound_id = match choice {
                    // Scope `-1` = app-dependent default (schema line 3368).
                    SoundChoice::Default => -1,
                    SoundChoice::Disabled => 0,
                    SoundChoice::Custom(id) => id,
                };
                self.apply_scope_sound(scope, sound_id, cx);
            }
            SoundPickerTarget::Reaction => {
                let sound_id = match choice {
                    // Reaction `-1` = app-dependent default (schema line 3394).
                    SoundChoice::Default => -1,
                    SoundChoice::Disabled => 0,
                    SoundChoice::Custom(id) => id,
                };
                self.apply_reaction_sound(sound_id, cx);
            }
        }
    }

    /// Parity slice: apply a scope's sound default
    /// (`setScopeNotificationSettings`).
    pub(super) fn apply_scope_sound(
        &mut self,
        scope: NotificationSettingsScope,
        sound_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.defaults_sound_picker = None;
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.sound_id = sound_id;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) => "default sound updated…".into(),
                Err(_) => "could not change default sound".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.sound_id = sound_id;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "default sound updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's mute default.
    pub(super) fn apply_scope_mute(
        &mut self,
        scope: NotificationSettingsScope,
        mute_for: i32,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.mute_for = mute_for;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) if mute_for == 0 => "default unmuted…".into(),
                Ok(_) => "default mute updated…".into(),
                Err(_) => "could not change default mute".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.mute_for = mute_for;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "default mute updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's preview default.
    pub(super) fn apply_scope_preview(
        &mut self,
        scope: NotificationSettingsScope,
        show_preview: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.show_preview = show_preview;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) => "default preview updated…".into(),
                Err(_) => "could not change default preview".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.show_preview = show_preview;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "default preview updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's story-mute default
    /// (`setScopeNotificationSettings`).
    pub(super) fn apply_scope_story_mute(
        &mut self,
        scope: NotificationSettingsScope,
        mute_stories: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.mute_stories = mute_stories;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) => "story notification default updated…".into(),
                Err(_) => "could not change story notification default".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.mute_stories = mute_stories;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "story notification default updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's mention-notification override
    /// (`setScopeNotificationSettings`).
    pub(super) fn apply_scope_mention_notif(
        &mut self,
        scope: NotificationSettingsScope,
        notify: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.disable_mention_notifications = !notify;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) => "default mention notifications updated…".into(),
                Err(_) => "could not change default mention notifications".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.disable_mention_notifications = !notify;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "default mention notifications updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's story-poster default
    /// (`setScopeNotificationSettings`).
    pub(super) fn apply_scope_story_poster(
        &mut self,
        scope: NotificationSettingsScope,
        show_story_poster: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.show_story_poster = show_story_poster;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) => "story poster default updated…".into(),
                Err(_) => "could not change story poster default".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.show_story_poster = show_story_poster;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "story poster default updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's pinned-message-notification override
    /// (`setScopeNotificationSettings`).
    pub(super) fn apply_scope_pinned_notif(
        &mut self,
        scope: NotificationSettingsScope,
        notify: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.disable_pinned_message_notifications = !notify;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.status_note = match result {
                Ok(_) => "default pinned-message notifications updated…".into(),
                Err(_) => "could not change default pinned-message notifications".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.disable_pinned_message_notifications = !notify;
            session.scope_notification_settings.insert(scope, settings);
            self.status_note = "default pinned-message notifications updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a reaction-notification source
    /// (`setReactionNotificationSettings`).
    pub(super) fn apply_reaction_source(
        &mut self,
        kind: ReactionSourceKind,
        source: ReactionNotificationSource,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — no
            // getter exists, so wait for the first
            // `updateReactionNotificationSettings` instead.
            let Some(mut settings) = live.driver.session.reaction_notification_settings.clone()
            else {
                self.status_note = "reaction settings still loading…".into();
                cx.notify();
                return;
            };
            match kind {
                ReactionSourceKind::Message => settings.message_reaction_source = source,
                ReactionSourceKind::Story => settings.story_reaction_source = source,
                ReactionSourceKind::PollVote => settings.poll_vote_source = source,
            }
            let result = live.driver.send_reaction_notification_settings(&settings);
            self.status_note = match result {
                Ok(_) => "reaction notification setting updated…".into(),
                Err(_) => "could not change reaction notification setting".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .reaction_notification_settings
                .clone()
                .unwrap_or_default();
            match kind {
                ReactionSourceKind::Message => settings.message_reaction_source = source,
                ReactionSourceKind::Story => settings.story_reaction_source = source,
                ReactionSourceKind::PollVote => settings.poll_vote_source = source,
            }
            session.reaction_notification_settings = Some(settings);
            self.status_note = "reaction notification setting updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply the reaction-notification sound
    /// (`setReactionNotificationSettings`).
    pub(super) fn apply_reaction_sound(&mut self, sound_id: i64, cx: &mut Context<Self>) {
        self.defaults_sound_picker = None;
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — no
            // getter exists, so wait for the first
            // `updateReactionNotificationSettings` instead.
            let Some(mut settings) = live.driver.session.reaction_notification_settings.clone()
            else {
                self.status_note = "reaction settings still loading…".into();
                cx.notify();
                return;
            };
            settings.sound_id = sound_id;
            let result = live.driver.send_reaction_notification_settings(&settings);
            self.status_note = match result {
                Ok(_) => "reaction sound updated…".into(),
                Err(_) => "could not change reaction sound".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .reaction_notification_settings
                .clone()
                .unwrap_or_default();
            settings.sound_id = sound_id;
            session.reaction_notification_settings = Some(settings);
            self.status_note = "reaction sound updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply the reaction-notification preview flag
    /// (`setReactionNotificationSettings`).
    pub(super) fn apply_reaction_preview(&mut self, show_preview: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — no
            // getter exists, so wait for the first
            // `updateReactionNotificationSettings` instead.
            let Some(mut settings) = live.driver.session.reaction_notification_settings.clone()
            else {
                self.status_note = "reaction settings still loading…".into();
                cx.notify();
                return;
            };
            settings.show_preview = show_preview;
            let result = live.driver.send_reaction_notification_settings(&settings);
            self.status_note = match result {
                Ok(_) => "reaction preview updated…".into(),
                Err(_) => "could not change reaction preview".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .reaction_notification_settings
                .clone()
                .unwrap_or_default();
            settings.show_preview = show_preview;
            session.reaction_notification_settings = Some(settings);
            self.status_note = "reaction preview updated".into();
        }
        cx.notify();
    }

    /// Parity slice: one scope's section in the defaults dialog.
    pub(super) fn scope_settings_section(
        &self,
        cx: &mut Context<Self>,
        scope: NotificationSettingsScope,
        saved_sounds: &[NotificationSound],
    ) -> AnyElement {
        let settings: ScopeNotificationSettings = self
            .session()
            .and_then(|s| s.scope_notification_settings.get(&scope).cloned())
            .unwrap_or_default();
        let loaded = self
            .session()
            .is_some_and(|s| s.scope_notification_settings.contains_key(&scope));
        let muted = settings.mute_for > 0;
        let sound_label = match settings.sound_id {
            -1 => "Default".to_string(),
            0 => "None".to_string(),
            id => saved_sounds
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.title.clone())
                .unwrap_or_else(|| "Custom".to_string()),
        };
        let current_choice = match settings.sound_id {
            -1 => SoundChoice::Default,
            0 => SoundChoice::Disabled,
            id => SoundChoice::Custom(id),
        };

        let presets = [
            ("Unmute", 0),
            ("1 hour", MUTE_FOR_1_HOUR),
            ("8 hours", MUTE_FOR_8_HOURS),
            ("2 days", MUTE_FOR_2_DAYS),
            ("Forever", MUTE_FOREVER),
        ];
        let mut preset_row = div().flex().flex_wrap().gap_1();
        for (label, seconds) in presets {
            preset_row = preset_row.child(
                Button::new(format!("scope-mute-{scope:?}-{seconds}"))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_scope_mute(scope, seconds, cx);
                    })),
            );
        }

        let mut section = div()
            .id(format!("scope-section-{:?}", scope))
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
                    .child(div().font_semibold().text_sm().child(scope.label()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loaded {
                                if muted { "Muted" } else { "Not muted" }.to_string()
                            } else {
                                "Loading…".to_string()
                            }),
                    ),
            )
            .child(preset_row)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Show message preview"),
                    )
                    .child(
                        // Phase 6: kit Switch (was: On/Off ghost button).
                        Switch::new(format!("scope-preview-{scope:?}"))
                            .checked(settings.show_preview)
                            .accessibility_label("Show message preview")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_preview(scope, on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Notify about mentions"),
                    )
                    .child(
                        // Parity slice: mention-notification override per scope
                        // (schema: disable_mention_notifications).
                        Switch::new(format!("scope-mentions-{scope:?}"))
                            .checked(!settings.disable_mention_notifications)
                            .accessibility_label("Notify about mentions")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_mention_notif(scope, on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Notify about pinned messages"),
                    )
                    .child(
                        // Parity slice: pinned-message-notification override
                        // per scope (schema: disable_pinned_message_notifications).
                        Switch::new(format!("scope-pinned-{scope:?}"))
                            .checked(!settings.disable_pinned_message_notifications)
                            .accessibility_label("Notify about pinned messages")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_pinned_notif(scope, on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Sound: {sound_label}")),
                    )
                    .child(
                        Button::new(format!("scope-sound-{:?}", scope))
                            .label(
                                if self.defaults_sound_picker
                                    == Some(SoundPickerTarget::Scope(scope))
                                {
                                    "Hide"
                                } else {
                                    "Change"
                                },
                            )
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let target = SoundPickerTarget::Scope(scope);
                                this.defaults_sound_picker =
                                    if this.defaults_sound_picker == Some(target) {
                                        None
                                    } else {
                                        Some(target)
                                    };
                                cx.notify();
                            })),
                    ),
            );
        if self.defaults_sound_picker == Some(SoundPickerTarget::Scope(scope)) {
            section = section.child(self.notification_sound_picker(
                cx,
                SoundPickerTarget::Scope(scope),
                current_choice,
                saved_sounds,
            ));
        }
        section = section
            .child(
                // Parity slice: story fields of `scopeNotificationSettings`
                // (schema line 3369-3375) — no story-sound picker (the
                // message-sound picker above already covers the per-scope
                // sound choice).
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Mute story notifications"),
                    )
                    .child(
                        Switch::new(format!("scope-story-mute-{scope:?}"))
                            .checked(settings.mute_stories)
                            .accessibility_label("Mute story notifications")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_story_mute(scope, on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Show story poster"),
                    )
                    .child(
                        Switch::new(format!("scope-story-poster-{scope:?}"))
                            .checked(settings.show_story_poster)
                            .accessibility_label("Show story poster")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_story_poster(scope, on, cx);
                            })),
                    ),
            );
        // Parity slice: per-scope notification exceptions
        // (`getChatNotificationSettingsExceptions`) — fetched on dialog
        // open; "Loading…" while the request is in flight.
        let exceptions = self
            .session()
            .and_then(|s| s.notification_exceptions.get(&scope));
        let exceptions_loading = self
            .session()
            .is_some_and(|s| s.notification_exceptions_loading.contains(&scope));
        let exceptions_label = match (exceptions, exceptions_loading) {
            (Some(ids), _) => {
                if ids.len() == 1 {
                    "1 chat".to_string()
                } else {
                    format!("{} chats", ids.len())
                }
            }
            (None, true) => "Loading…".to_string(),
            (None, false) => "—".to_string(),
        };
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Exceptions: {exceptions_label}")),
                )
                .child(
                    Button::new(format!("scope-exceptions-{scope:?}"))
                        .label(if self.defaults_exceptions_scope == Some(scope) {
                            "Hide"
                        } else {
                            "View"
                        })
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.defaults_exceptions_scope =
                                if this.defaults_exceptions_scope == Some(scope) {
                                    None
                                } else {
                                    Some(scope)
                                };
                            cx.notify();
                        })),
                ),
        );
        if self.defaults_exceptions_scope == Some(scope) {
            section = section.child(self.exceptions_list(cx, scope));
        }
        section.into_any_element()
    }

    /// Parity slice: the expanded exceptions sub-view for one scope — the
    /// exception chats (titles resolved from the session chats map) with a
    /// "Reset to default" button per chat.
    pub(super) fn exceptions_list(
        &self,
        cx: &mut Context<Self>,
        scope: NotificationSettingsScope,
    ) -> AnyElement {
        let session = self.session();
        let cached: Option<&Vec<i64>> = session
            .as_ref()
            .and_then(|s| s.notification_exceptions.get(&scope));
        let loading = session
            .as_ref()
            .is_some_and(|s| s.notification_exceptions_loading.contains(&scope));
        let mut list = div().flex().flex_col().gap_1().px_2().py_1();
        let Some(ids) = cached else {
            // Fetch failed or never fired — the next dialog open retries.
            return list
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if loading {
                            "Loading…"
                        } else {
                            "Exceptions unavailable — reopen the dialog to retry."
                        }),
                )
                .into_any_element();
        };
        if ids.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No exceptions — every chat uses the scope default."),
            );
        }
        for &id in ids {
            let title = session
                .as_ref()
                .and_then(|s| s.chats.get(&id))
                .map(|c| c.title.clone())
                .unwrap_or_else(|| format!("Chat {id}"));
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().text_sm().child(title))
                    .child(
                        Button::new(format!("exception-reset-{id}"))
                            .label("Reset to default")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.reset_notification_exception(ChatId(id), cx);
                            })),
                    ),
            );
        }
        list.into_any_element()
    }

    /// Parity slice: reset one exception chat to the scope defaults
    /// (`setChatNotificationSettings` with every `use_default_*` flag set).
    pub(super) fn reset_notification_exception(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.reset_chat_notification_settings(chat_id);
            self.status_note = match result {
                Ok(_) => "exception reset…".into(),
                Err(_) => "could not reset exception".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the list reflects it.
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.notification_settings = ChatNotificationSettings::default();
            }
            for list in session.notification_exceptions.values_mut() {
                list.retain(|id| *id != chat_id.0);
            }
            self.status_note = "exception reset".into();
        }
        cx.notify();
    }

    /// Parity slice: reaction + poll-vote notification settings section in
    /// the defaults dialog (`reactionNotificationSettings`, TDLib 1.8.67
    /// line 3396; no getter — the current values arrive as
    /// `updateReactionNotificationSettings`).
    pub(super) fn reaction_settings_section(
        &self,
        cx: &mut Context<Self>,
        saved_sounds: &[NotificationSound],
    ) -> AnyElement {
        let settings: ReactionNotificationSettings = self
            .session()
            .and_then(|s| s.reaction_notification_settings.clone())
            .unwrap_or_default();
        let loaded = self
            .session()
            .is_some_and(|s| s.reaction_notification_settings.is_some());
        let sound_label = match settings.sound_id {
            -1 => "Default".to_string(),
            0 => "None".to_string(),
            id => saved_sounds
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.title.clone())
                .unwrap_or_else(|| "Custom".to_string()),
        };
        let current_choice = match settings.sound_id {
            -1 => SoundChoice::Default,
            0 => SoundChoice::Disabled,
            id => SoundChoice::Custom(id),
        };

        let mut section = div()
            .id("reaction-settings-section")
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
                    .child(div().font_semibold().text_sm().child("Reactions"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loaded {
                                "Set".to_string()
                            } else {
                                "Loading…".to_string()
                            }),
                    ),
            );
        for (kind, label) in [
            (ReactionSourceKind::Message, "Message reactions"),
            (ReactionSourceKind::Story, "Story reactions"),
            (ReactionSourceKind::PollVote, "Poll votes"),
        ] {
            section = section.child(self.reaction_source_row(cx, kind, label));
        }
        section = section
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Show sender and emoji in notification"),
                    )
                    .child(
                        // Phase 6: kit Switch.
                        Switch::new("reaction-preview")
                            .checked(settings.show_preview)
                            .accessibility_label("Show sender and emoji in reaction notifications")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_reaction_preview(on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Sound: {sound_label}")),
                    )
                    .child(
                        Button::new("reaction-sound-toggle")
                            .label(
                                if self.defaults_sound_picker == Some(SoundPickerTarget::Reaction) {
                                    "Hide"
                                } else {
                                    "Change"
                                },
                            )
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.defaults_sound_picker = if this.defaults_sound_picker
                                    == Some(SoundPickerTarget::Reaction)
                                {
                                    None
                                } else {
                                    Some(SoundPickerTarget::Reaction)
                                };
                                cx.notify();
                            })),
                    ),
            );
        if self.defaults_sound_picker == Some(SoundPickerTarget::Reaction) {
            section = section.child(self.notification_sound_picker(
                cx,
                SoundPickerTarget::Reaction,
                current_choice,
                saved_sounds,
            ));
        }
        section.into_any_element()
    }

    /// Parity slice: one reaction source's preset row (None / Contacts /
    /// Everyone), mirroring the scope mute presets.
    pub(super) fn reaction_source_row(
        &self,
        cx: &mut Context<Self>,
        kind: ReactionSourceKind,
        label: &str,
    ) -> AnyElement {
        let mut row = div().flex().flex_wrap().gap_1().items_center();
        row = row.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{label}:")),
        );
        for (preset_label, source) in [
            ("None", ReactionNotificationSource::None),
            ("Contacts", ReactionNotificationSource::Contacts),
            ("Everyone", ReactionNotificationSource::All),
        ] {
            row = row.child(
                Button::new(format!("reaction-source-{kind:?}-{source:?}"))
                    .label(preset_label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_reaction_source(kind, source, cx);
                    })),
            );
        }
        row.into_any_element()
    }
}
