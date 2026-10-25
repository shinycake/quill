//! settings sections: archive, contacts, calls, media/auto-download.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::privacy::PrivacyEditorTarget;
use super::shell::{DialogKind, QuillShell};
use crate::ui::{status_note::StatusNote, user_error::user_error_for};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::table::TableCell;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::data_settings::NetworkKind;
use quill::lang::{keys, tr, tr_with};
use quill::privacy::PrivacyKeyState;
use quill::settings::{
    AUTO_DOWNLOAD_FILE, AUTO_DOWNLOAD_GIF, AUTO_DOWNLOAD_MUSIC, AUTO_DOWNLOAD_PHOTO,
    AUTO_DOWNLOAD_VIDEO, AUTO_DOWNLOAD_VIDEO_NOTE, AUTO_DOWNLOAD_VOICE, CallPrefs,
};
use quill::telegram::requests::{ArchiveChatListSettings, PrivacyWho};
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::cell::RefCell;
use std::rc::Rc;
/// Slice CL2: the three `archiveChatListSettings` fields (schema
/// 1.8.67, line 3512) as (section title, toggle label, description),
/// in schema order. Labels mirror TGX
/// `SettingsArchiveChatListController`.
pub(super) const ARCHIVE_SETTING_ROWS: [(quill::lang::Key, quill::lang::Key, quill::lang::Key); 3] = [
    (
        keys::Q_PREFS_CHATS_FROM_UNKNOWN_USERS,
        keys::SETTINGS_AUTO_ARCHIVE,
        keys::SETTINGS_AUTO_ARCHIVE_ABOUT,
    ),
    (
        keys::SETTINGS_UNMUTED_CHATS,
        keys::SETTINGS_ALWAYS_IN_ARCHIVE,
        keys::SETTINGS_UNMUTED_CHATS_ABOUT,
    ),
    (
        keys::SETTINGS_CHATS_FROM_FOLDERS,
        keys::SETTINGS_ALWAYS_IN_ARCHIVE,
        keys::SETTINGS_CHATS_FROM_FOLDERS_ABOUT,
    ),
];

/// Slice CL2: read one `archiveChatListSettings` field by schema-order
/// index.
pub(super) fn archive_setting_get(settings: &ArchiveChatListSettings, index: usize) -> bool {
    match index {
        0 => settings.archive_and_mute_new_chats_from_unknown_users,
        1 => settings.keep_unmuted_chats_archived,
        2 => settings.keep_chats_from_folders_archived,
        _ => false,
    }
}

/// Slice CL2: write one `archiveChatListSettings` field by schema-order
/// index.
pub(super) fn archive_setting_set(
    settings: &mut ArchiveChatListSettings,
    index: usize,
    value: bool,
) {
    match index {
        0 => settings.archive_and_mute_new_chats_from_unknown_users = value,
        1 => settings.keep_unmuted_chats_archived = value,
        2 => settings.keep_chats_from_folders_archived = value,
        _ => {}
    }
}

impl QuillApp {
    /// Slice CL2: open the archive auto-settings dialog (TGX
    /// `SettingsArchiveChatListController` fetches
    /// `getArchiveChatListSettings` on open).
    pub(super) fn open_archive_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.chat_list.archive_settings_open = true;
            if let Err(err) = live.driver.fetch_archive_chat_list_settings() {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_ARCHIVE_SETTINGS,
                    &err,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.chat_list.archive_settings_open = true;
        }
        cx.notify();
    }

    /// Slice CL2: flip one archive auto-setting — optimistic local flip
    /// plus `setArchiveChatListSettings`; a refusal restores the old
    /// values (driver rollback).
    /// kit Phase 6: write the requested value (was: flip the bit).
    pub(super) fn set_archive_setting(&mut self, index: usize, on: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let Some(mut settings) = live.driver.session.chat_list.archive_chat_list_settings
            else {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_PREFS_ARCHIVE_SETTINGS_STILL_LOADING,
                )));
                cx.notify();
                return;
            };
            archive_setting_set(&mut settings, index, on);
            if let Err(err) = live.driver.set_archive_chat_list_settings(settings) {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_ARCHIVE_SETTINGS,
                    &err,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut()
            && let Some(mut settings) = session.chat_list.archive_chat_list_settings
        {
            archive_setting_set(&mut settings, index, on);
            session.chat_list.archive_chat_list_settings = Some(settings);
        }
        cx.notify();
    }

    /// kit Phase 2 (redo): storage usage hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    /// Phase 6: put arbitrary content in a static kit [`TableCell`]. The
    /// cell's inherent `.child()` only accepts kit table parts (it shadows
    /// [`ParentElement::child`]), so content goes through the trait method.
    pub(crate) fn table_cell(content: impl IntoElement) -> TableCell {
        ParentElement::child(TableCell::new(), content)
    }

    /// kit Phase 2 (redo): archive settings hosted in a kit `Dialog`
    /// via `window.open_dialog`. Title/footer go to the kit slots;
    /// Esc / backdrop / ✕ clear the session flag through `on_close`.
    pub(super) fn build_archive_settings_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ArchiveSettings, |this, _, _| {
                this.set_archive_settings_open(false)
            });
        app.update(cx, |this, cx| {
            let settings = this
                .session()
                .and_then(|s| s.chat_list.archive_chat_list_settings);
            let loading = this
                .session()
                .is_some_and(|s| s.chat_list.archive_settings_loading);
            let mut body = div().flex().flex_col().gap_3();
            if loading {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_PREFS_LOADING_ARCHIVE_SETTINGS)),
                );
            } else if settings.is_none() {
                body = body
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PREFS_COULD_NOT_LOAD_ARCHIVE_SETTINGS)),
                    )
                    .child(
                        Button::new("archive-settings-retry")
                            .label(tr(keys::Q_PREFS_RETRY))
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_archive_settings(cx);
                                this.close_kit_dialog_if_done(
                                    DialogKind::ArchiveSettings,
                                    window,
                                    cx,
                                );
                            })),
                    );
            } else if let Some(settings) = settings {
                let mute_offered = this.archive_mute_offered(&settings);
                for (index, (title, label, desc)) in ARCHIVE_SETTING_ROWS.iter().enumerate() {
                    if index == 0 && !mute_offered {
                        continue;
                    }
                    let enabled = archive_setting_get(&settings, index);
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().font_semibold().child(tr(*title)))
                            .child(
                                // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                                Checkbox::new(format!("archive-setting-{index}"))
                                    .checked(enabled)
                                    .label(tr(*label))
                                    .on_click(cx.listener(move |this, &on, window, cx| {
                                        this.set_archive_setting(index, on, cx);
                                        this.close_kit_dialog_if_done(
                                            DialogKind::ArchiveSettings,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(*desc)),
                            ),
                    );
                }
            }
            let footer = div().flex().justify_end().child(
                Button::new("close-archive-settings")
                    .label(tr(keys::CLOSE))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_archive_settings_open(false);
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::ArchiveSettings, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::SETTINGS_ARCHIVE_TITLE,
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
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
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Slice CL2: show/hide the archive settings dialog on the active
    /// session.
    pub(super) fn set_archive_settings_open(&mut self, open: bool) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.chat_list.archive_settings_open = open;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.chat_list.archive_settings_open = open;
        }
    }

    /// Phase C2i: call settings — confirm-before-calling (real +
    /// persisted), who-can-call-me and P2P (real TDLib privacy), and
    /// the honest "saved, not yet applied" less-data toggle.
    /// Phase 6: call prefs are kit `Switch`es, privacy who-groups are
    /// kit `RadioGroup`s (was: hand-rolled ●/○ rows). Behavior unchanged.
    pub(super) fn call_settings_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let prefs = self
            .session()
            .map(|session| session.calls.prefs.clone())
            .unwrap_or_default();
        // The same rules as Privacy and security > Who can call me, with
        // their exceptions: a choice made here keeps the exception lists.
        let rule_state = |key: PrivacySettingKey| {
            self.session()
                .and_then(|session| session.settings.privacy.get(&key).cloned())
        };
        let who_of = |key: PrivacySettingKey| match rule_state(key) {
            Some(PrivacyKeyState::Ready(detail)) => detail.who,
            _ => None,
        };
        let allow_calls = who_of(PrivacySettingKey::AllowCalls);
        let p2p = who_of(PrivacySettingKey::PeerToPeer);
        let privacy_loading = [PrivacySettingKey::AllowCalls, PrivacySettingKey::PeerToPeer]
            .into_iter()
            .any(|key| matches!(rule_state(key), None | Some(PrivacyKeyState::Loading)));
        let privacy_failed = [PrivacySettingKey::AllowCalls, PrivacySettingKey::PeerToPeer]
            .into_iter()
            .any(|key| matches!(rule_state(key), Some(PrivacyKeyState::Failed)));
        let ptt_section = self.push_to_talk_settings(&prefs.push_to_talk, cx);
        let mic_test_section = self.mic_test_section(cx);
        let device_prefs_section = self.call_device_prefs_section(cx);
        // Phase 6: one kit RadioGroup per privacy setting. Controlled:
        // the chosen index writes the value and the owner re-renders.
        let privacy_group =
            |id: &'static str, current: Option<PrivacyWho>, key: PrivacySettingKey| {
                const WHOS: [PrivacyWho; 3] = [
                    PrivacyWho::Everybody,
                    PrivacyWho::Contacts,
                    PrivacyWho::Nobody,
                ];
                const LABELS: [quill::lang::Key; 3] = [
                    keys::EDIT_PRIVACY_EVERYONE,
                    keys::Q_PREFS_MY_CONTACTS,
                    keys::Q_PREFS_NOBODY,
                ];
                let selected = current.and_then(|w| WHOS.iter().position(|x| *x == w));
                RadioGroup::vertical(id)
                    .selected_index(selected)
                    .children(
                        WHOS.iter().zip(LABELS.iter()).map(|(who, label)| {
                            Radio::new(format!("{id}-{who:?}")).label(tr(*label))
                        }),
                    )
                    .on_click(cx.listener(move |this, &ix, _, cx| {
                        if let Some(&who) = WHOS.get(ix) {
                            this.set_privacy_target_who(PrivacyEditorTarget::Rule(key), who, cx);
                        }
                    }))
            };
        // Phase 6: a kit Switch row with the title/caption beside it
        // (was: a clickable row with a ●/○ bullet).
        let pref_row = |id: &'static str,
                        on: bool,
                        title: &'static str,
                        caption: &'static str,
                        set: fn(&mut CallPrefs, bool)| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .child(
                    Switch::new(format!("{id}-switch"))
                        .checked(on)
                        .accessibility_label(title)
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            this.set_call_pref(|prefs| set(prefs, on), cx);
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().text_sm().child(title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(caption),
                        ),
                )
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .mt_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .px_1()
                    .child(tr(keys::SETTINGS_SECTION_CALL_SETTINGS)),
            )
            .child(pref_row(
                "call-pref-confirm",
                prefs.confirm_before_calling,
                tr(keys::Q_PREFS_CONFIRM_BEFORE_CALLING),
                tr(keys::Q_PREFS_ASK_BEFORE_PLACING_A_CALL),
                |prefs, on| prefs.confirm_before_calling = on,
            ))
            // Slice calls-proxy: "Use proxy for calls" (official:
            // Settings → Data & Storage → Proxy). Client-side toggle in
            // CallPrefs — the TDLib schema has no such option; when on,
            // the client routes call media through the enabled proxy
            // (SOCKS5 only, see `calls::proxy::proxy_for_calls`).
            .child(pref_row(
                "call-pref-proxy",
                prefs.use_proxy_for_calls,
                tr(keys::PROXY_USE_FOR_CALLS),
                tr(keys::Q_PREFS_ROUTE_CALLS_THROUGH_THE_ENABLED),
                |prefs, on| prefs.use_proxy_for_calls = on,
            ))
            .child({
                // Slice S4: "Use less data for calls" is a real TDLib
                // setting (`autoDownloadSettings.use_less_data_for_calls`,
                // schema 1.8.67 :9856) — one toggle driving all three
                // networks — replacing the old local-only CallPrefs flag.
                let on = self.session().is_some_and(|s| {
                    s.settings.data_storage.seeded
                        && NetworkKind::ALL.iter().all(|n| {
                            s.settings
                                .data_storage
                                .for_network(*n)
                                .use_less_data_for_calls
                        })
                });
                div()
                    .id("call-pref-less-data")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(
                        Switch::new("call-pref-less-data-switch")
                            .checked(on)
                            .accessibility_label(tr(keys::Q_PREFS_USE_LESS_DATA_FOR_CALLS))
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    if live.driver.set_less_data_for_calls(on).is_err() {
                                        this.connection.set_note(StatusNote::error(tr(
                                            keys::Q_PREFS_COULDNT_UPDATE_CALL_SETTINGS,
                                        )));
                                    }
                                } else if let Some(demo) = this.demo_session.as_mut() {
                                    // Demo: show the chosen value immediately
                                    // (no live TDLib to confirm it).
                                    for n in NetworkKind::ALL {
                                        demo.settings
                                            .data_storage
                                            .for_network_mut(n)
                                            .use_less_data_for_calls = on;
                                    }
                                    demo.settings.data_storage.seeded = true;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .child(tr(keys::Q_PREFS_USE_LESS_DATA_FOR_CALLS)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(keys::Q_PREFS_LOWER_DATA_USAGE_DURING_TELEGRAM)),
                            ),
                    )
            })
            .child(ptt_section)
            .child(mic_test_section)
            .child(device_prefs_section)
            .child(
                div()
                    .text_xs()
                    .font_medium()
                    .px_2()
                    .pt_1()
                    .child(tr(keys::EDIT_PRIVACY_CALLS_HEADER)),
            )
            .child(if privacy_failed {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_COULDN_T_LOAD_PRIVACY_SETTINGS))
                    .into_any_element()
            } else if privacy_loading && allow_calls.is_none() {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_LOADING_PRIVACY_SETTINGS))
                    .into_any_element()
            } else {
                privacy_group(
                    "call-privacy-allow",
                    allow_calls,
                    PrivacySettingKey::AllowCalls,
                )
                .into_any_element()
            })
            .child(
                div()
                    .text_xs()
                    .font_medium()
                    .px_2()
                    .pt_1()
                    .child(tr(keys::Q_PREFS_PEER_TO_PEER_CALLS)),
            )
            .child(if privacy_failed {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_COULDN_T_LOAD_PRIVACY_SETTINGS))
                    .into_any_element()
            } else if privacy_loading && p2p.is_none() {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .px_2()
                    .child(tr(keys::Q_PREFS_LOADING_PRIVACY_SETTINGS))
                    .into_any_element()
            } else {
                privacy_group("call-privacy-p2p", p2p, PrivacySettingKey::PeerToPeer)
                    .into_any_element()
            })
    }

    /// MED2: media settings section — the HQ round-video toggle (TGX
    /// "Record HQ Round Videos" / `UseHqRoundVideos`). Persisted in
    /// `MediaPrefs`; 480px captures when on, 280px otherwise.
    pub(super) fn media_settings_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hq = self
            .session()
            .is_some_and(|session| session.settings.media_prefs.hq_round_videos);
        // MED4: Instant View mode (TGX: None / Telegram-internal / All).
        let iv_mode = self
            .session()
            .map(|session| session.settings.media_prefs.instant_view_mode)
            .unwrap_or(quill::settings::InstantViewMode::Telegram);
        // Phase 6: single-choice group (was: "tap to cycle" row).
        let iv_modes = [
            (
                tr(keys::SETTINGS_TTL_AFTER_OFF),
                quill::settings::InstantViewMode::Off,
            ),
            (
                tr(keys::Q_PREFS_TELEGRAM_LINKS),
                quill::settings::InstantViewMode::Telegram,
            ),
            (
                tr(keys::Q_PREFS_ALL_LINKS),
                quill::settings::InstantViewMode::All,
            ),
        ];
        let iv_selected = iv_modes.iter().position(|(_, mode)| *mode == iv_mode);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .mt_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .px_1()
                    .child(tr(keys::Q_PREFS_MEDIA_SETTINGS)),
            )
            .child(
                // Phase 6: kit Switch (was: clickable row with a ●/○ bullet).
                div()
                    .id("media-pref-hq-round")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(
                        Switch::new("media-pref-hq-round-switch")
                            .checked(hq)
                            .accessibility_label(tr(keys::Q_PREFS_RECORD_HQ_ROUND_VIDEOS))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_hq_round_videos(on, cx);
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .child(tr(keys::Q_PREFS_RECORD_HQ_ROUND_VIDEOS)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(keys::Q_PREFS_CAPTURE_ROUND_VIDEO_NOTES_AT)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("media-pref-instant-view")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .child(tr(keys::Q_PREFS_INSTANT_VIEW)),
                    )
                    .child(
                        RadioGroup::vertical("media-pref-instant-view-group")
                            .selected_index(iv_selected)
                            .children(iv_modes.iter().map(|(label, _)| {
                                Radio::new(format!("instant-view-{label}")).label(*label)
                            }))
                            .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                                if let Some(mode) = iv_modes.get(ix).map(|entry| entry.1) {
                                    this.set_media_pref(|prefs| prefs.instant_view_mode = mode, cx);
                                }
                            })),
                    ),
            )
            .child(
                Button::new("open-emoji-sets")
                    .label(tr(keys::Q_PREFS_EMOJI_SETS))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.open_emoji_sets(cx))),
            )
            .child(self.quick_action_radios(cx))
            .child(self.quick_reaction_picker(cx))
            .child(self.corner_button_switches(cx))
            .child(self.pull_next_switch(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .child(
                        Switch::new("media-pref-autoplay-gifs")
                            .checked(
                                self.session()
                                    .is_none_or(|s| s.settings.media_prefs.autoplay_gifs),
                            )
                            .accessibility_label(tr(keys::Q_PREFS_AUTOPLAY_GIFS))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_media_pref(|prefs| prefs.autoplay_gifs = on, cx);
                                if !on {
                                    this.stop_animation_playback();
                                    this.playback.inline_videos.clear();
                                }
                                cx.notify();
                            })),
                    )
                    .child(div().text_sm().child(tr(keys::Q_PREFS_AUTOPLAY_GIFS))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .child(
                        Switch::new("media-pref-autoplay-videos")
                            .checked(
                                self.session()
                                    .is_none_or(|s| s.settings.media_prefs.autoplay_videos),
                            )
                            .accessibility_label(tr(keys::Q_PREFS_AUTOPLAY_VIDEOS))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_media_pref(|prefs| prefs.autoplay_videos = on, cx);
                                if !on {
                                    this.playback.inline_videos.clear();
                                }
                                cx.notify();
                            })),
                    )
                    .child(div().text_sm().child(tr(keys::Q_PREFS_AUTOPLAY_VIDEOS))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .child(
                        Switch::new("media-pref-loop-stickers")
                            .checked(
                                self.session()
                                    .is_none_or(|s| s.settings.media_prefs.loop_animated_stickers),
                            )
                            .accessibility_label(tr(keys::SETTINGS_LOOP_STICKERS))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.stop_sticker_playback();
                                this.set_media_pref(|prefs| prefs.loop_animated_stickers = on, cx);
                            })),
                    )
                    .child(div().text_sm().child(tr(keys::SETTINGS_LOOP_STICKERS))),
            )
            // MED3: auto-download settings below the media prefs.
            .child(self.auto_download_settings_section(cx))
    }

    /// MED3: auto-download settings — data-saver master toggle plus the
    /// per-chat-kind × media-type grid. The bit model mirrors TGX
    /// `settings_autodownload` (private/group/channel shifts); desktop has
    /// no mobile/wifi/roaming distinction, so TGX's per-connection grids
    /// collapse into this one (Telegram Desktop's own dialog is the same
    /// grid). Persisted client-side in `media_prefs.json`.
    pub(super) fn auto_download_settings_section(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let prefs = self
            .session()
            .map(|s| s.settings.media_prefs.clone())
            .unwrap_or_default();
        let section = div()
            .flex()
            .flex_col()
            .gap_1()
            .mt_2()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .px_1()
                    .child(tr(keys::Q_PREFS_AUTO_DOWNLOAD)),
            )
            .child(
                // Phase 6: kit Switch (was: clickable row with a ●/○ bullet).
                div()
                    .id("media-pref-data-saver")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(
                        Switch::new("media-pref-data-saver-switch")
                            .checked(prefs.data_saver)
                            .accessibility_label(tr(keys::Q_PREFS_DATA_SAVER))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_media_pref(|prefs| prefs.data_saver = on, cx);
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().child(tr(keys::Q_PREFS_DATA_SAVER)))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(keys::Q_PREFS_PAUSE_ALL_AUTOMATIC_MEDIA_DOWNLOADS)),
                            ),
                    ),
            );
        const ROWS: [(quill::lang::Key, u8); 7] = [
            (keys::Q_PREFS_PHOTOS, AUTO_DOWNLOAD_PHOTO),
            (keys::Q_PREFS_VOICE_MESSAGES, AUTO_DOWNLOAD_VOICE),
            (keys::Q_PREFS_VIDEO_MESSAGES, AUTO_DOWNLOAD_VIDEO_NOTE),
            (keys::Q_PREFS_VIDEOS, AUTO_DOWNLOAD_VIDEO),
            (keys::Q_PREFS_FILES, AUTO_DOWNLOAD_FILE),
            (keys::Q_PREFS_MUSIC, AUTO_DOWNLOAD_MUSIC),
            (keys::Q_PREFS_GIFS, AUTO_DOWNLOAD_GIF),
        ];
        let kinds = [
            (tr(keys::Q_PREFS_PRIVATE), prefs.auto_download_private),
            (tr(keys::MEDIA_TYPE_GROUPS), prefs.auto_download_groups),
            (tr(keys::RECENT_CHANNELS), prefs.auto_download_channels),
        ];
        let mut grid = div().flex().flex_col().gap_1().px_2().mt_1();
        let mut header = div().flex().items_center().gap_2();
        header = header.child(div().w(px(110.)).child(""));
        for (kind, _) in &kinds {
            header = header.child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(*kind),
            );
        }
        grid = grid.child(header);
        for (row_idx, (label, flag)) in ROWS.iter().enumerate() {
            let mut row = div().flex().items_center().gap_2();
            row = row.child(div().w(px(110.)).text_sm().child(tr(*label)));
            for (kind_idx, (kind, bits)) in kinds.iter().enumerate() {
                let on = bits & flag != 0;
                let flag = *flag;
                // Phase 6: kit Checkbox (was: clickable ●/○ cells).
                // Controlled: the requested value sets/clears the bit.
                row = row.child(
                    div()
                        .id(format!("auto-dl-{row_idx}-{kind_idx}"))
                        .flex_1()
                        .flex()
                        .justify_center()
                        .child(
                            Checkbox::new(format!("auto-dl-check-{row_idx}-{kind_idx}"))
                                .checked(on)
                                .accessibility_label(tr_with(
                                    keys::Q_PREFS_IN_CHATS,
                                    &[("label", tr(*label)), ("kind", kind)],
                                ))
                                .on_click(cx.listener(move |this, &on, _, cx| {
                                    this.set_media_pref(
                                        move |prefs| {
                                            let bits = match kind_idx {
                                                0 => &mut prefs.auto_download_private,
                                                1 => &mut prefs.auto_download_groups,
                                                _ => &mut prefs.auto_download_channels,
                                            };
                                            if on {
                                                *bits |= flag;
                                            } else {
                                                *bits &= !flag;
                                            }
                                        },
                                        cx,
                                    );
                                })),
                        ),
                );
            }
            grid = grid.child(row);
        }
        section.child(grid)
    }
}

impl QuillApp {
    /// Whether the archive-and-mute switch is offered (tdesktop
    /// `SetupArchiveAndMute`): the server allows it, it is on, or Premium.
    fn archive_mute_offered(&self, settings: &ArchiveChatListSettings) -> bool {
        self.session().is_some_and(|s| {
            s.chat_list
                .archive_mute
                .switch_shown(settings, s.is_premium())
        })
    }

    /// Privacy and security > Archive and Mute (tdesktop
    /// `settings_privacy_security.cpp`): the three archive switches,
    /// including the one for chats from folders, inline.
    pub(crate) fn archive_privacy_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let settings = session.and_then(|s| s.chat_list.archive_chat_list_settings);
        let loading = session.is_some_and(|s| s.chat_list.archive_settings_loading);
        let mut section = div().flex().flex_col().gap_1().child(
            div()
                .text_sm()
                .font_semibold()
                .px_1()
                .child(tr(keys::SETTINGS_AUTO_ARCHIVE)),
        );
        let Some(settings) = settings else {
            let note = if loading {
                tr(keys::Q_PREFS_LOADING)
            } else {
                tr(keys::Q_PREFS_COULDNT_LOAD_THESE_SETTINGS)
            };
            return section
                .child(
                    div()
                        .id("archive-privacy-note")
                        .px_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(note),
                )
                .into_any_element();
        };
        let mute_offered = self.archive_mute_offered(&settings);
        for (index, (title, label, desc)) in ARCHIVE_SETTING_ROWS.iter().enumerate() {
            if index == 0 && !mute_offered {
                continue;
            }
            section = section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(*title)),
                    )
                    .child(
                        Switch::new(format!("archive-privacy-{index}"))
                            .checked(archive_setting_get(&settings, index))
                            .label(tr(*label))
                            .accessibility_label(format!("{}: {}", tr(*title), tr(*label)))
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.set_archive_setting(index, on, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(*desc)),
                    ),
            );
        }
        section.into_any_element()
    }
}

crate::ui::shell::register_dialogs! {
    ArchiveSettings => DialogSpec::new(
        500,
        |app| app.session().is_some_and(|s| s.chat_list.archive_settings_open),
        QuillApp::build_archive_settings_dialog,
    ),
}
