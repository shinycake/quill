//! The wallpaper and group emoji pack rows of the chat Appearance box
//! (tdesktop `edit_peer_color_box.cpp`: the "Wallpaper" button with a level
//! badge, and `CreateEmojiPackButton` for supergroups), plus the boost check
//! in front of a boosted chat's wallpaper (`BackgroundPreviewBox::
//! checkLevelForChannel`). The wallpaper itself is picked in the chat look
//! dialog (`chat_look_ui`), which serves channels and supergroups too.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_look_ui::LookChange;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::chat_appearance::{ChatAppearanceKind, FeatureLevels, WallpaperGate, WallpaperLevels};
use quill::ids::ChatId;

impl QuillApp {
    /// Whether the chat look dialog may open for a channel or supergroup the
    /// viewer can edit (a "boosted" chat).
    pub(super) fn boosted_look_allowed(&self, chat_id: i64) -> bool {
        self.session().is_some_and(|s| {
            s.chat_supergroup(ChatId(chat_id)).is_some() && s.chat_can_change_info(ChatId(chat_id))
        })
    }

    pub(super) fn is_boosted_look(&self, chat_id: i64) -> bool {
        self.session()
            .is_some_and(|s| s.chat_supergroup(ChatId(chat_id)).is_some())
    }

    fn wallpaper_levels(&self, chat_id: i64) -> WallpaperLevels {
        WallpaperLevels::from_features(
            self.session()
                .and_then(|s| s.groups.chat_boost_features.get(&chat_id)),
        )
    }

    /// The boost check before a boosted chat's wallpaper is sent. Returns
    /// `true` when Apply must stop; the reason shows in the dialog's error
    /// line. A gallery wallpaper or file is a custom background; a chat
    /// theme is a theme background; removing a wallpaper is free.
    pub(super) fn boosted_look_blocked(&mut self, chat_id: i64, changes: &[LookChange]) -> bool {
        let custom = changes.iter().any(|change| {
            matches!(
                change,
                LookChange::Wallpaper { .. } | LookChange::WallpaperFile { .. }
            )
        });
        let theme = changes
            .iter()
            .any(|change| matches!(change, LookChange::Theme(_)));
        if !custom && !theme {
            return false;
        }
        let level = self
            .session()
            .and_then(|s| s.groups.chat_boost_status.get(&chat_id))
            .map(|(level, _)| *level);
        let levels = self.wallpaper_levels(chat_id);
        let gate = match (custom, levels.gate(true, level), levels.gate(false, level)) {
            (true, gate, _) => gate,
            (false, _, gate) => gate,
        };
        let message = match gate {
            WallpaperGate::Allowed => return false,
            WallpaperGate::LevelUnknown => {
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.refresh_chat_boost_level(ChatId(chat_id));
                }
                "Checking the boost level. Press Apply again in a moment.".to_string()
            }
            WallpaperGate::NeedsBoosts { required, have } => {
                format!("Level {required} required. This chat is at level {have}.")
            }
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.session.chats_state.background_error = Some(message);
        }
        true
    }

    /// "Wallpaper" row: opens the chat look dialog for this chat.
    pub(super) fn chat_appearance_wallpaper_section(
        &self,
        chat_id: ChatId,
        kind: ChatAppearanceKind,
        level: Option<i32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let levels = self.wallpaper_levels(chat_id.0);
        let word = match kind {
            ChatAppearanceKind::Channel => "channel",
            ChatAppearanceKind::Supergroup => "group",
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().font_semibold().child(match kind {
                ChatAppearanceKind::Channel => "Channel wallpaper",
                ChatAppearanceKind::Supergroup => "Group wallpaper",
            }))
            .children(Self::chat_appearance_lock_note(
                levels.custom_background,
                level,
                muted,
            ))
            .child(
                div().flex().child(
                    Button::new("chat-appearance-wallpaper")
                        .label("Choose wallpaper…")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_chat_look_dialog(chat_id.0, cx);
                        })),
                ),
            )
            .child(div().text_xs().text_color(muted).child(format!(
                "Choose a chat theme or a wallpaper for everyone in this {word}. Chat themes need level {}.",
                levels.theme_background
            )))
            .into_any_element()
    }

    /// "Group emoji pack" (supergroups): the installed emoji packs.
    pub(super) fn chat_appearance_pack_section(
        &self,
        features: FeatureLevels,
        level: Option<i32>,
        selected: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let sets: Vec<(i64, String)> = self
            .session()
            .map(|s| {
                s.stickers
                    .emoji
                    .installed_sets
                    .iter()
                    .map(|set| (set.id, set.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let empty = sets.is_empty();
        let mut choices = div()
            .id("chat-appearance-packs")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(160.))
            .overflow_y_scroll();
        for (id, label) in std::iter::once((0, "No pack".to_string())).chain(sets) {
            choices = choices.child(
                Button::new(format!("chat-appearance-pack-{id}"))
                    .label(label)
                    .ghost()
                    .selected(id == selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_chat_appearance(cx, |draft, _| draft.emoji_pack_id = id);
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().font_semibold().child("Group emoji pack"))
            .children(Self::chat_appearance_lock_note(
                features.emoji_pack,
                level,
                muted,
            ))
            .child(choices)
            .when(empty, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Loading your emoji packs"),
                )
            })
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Members can use these emoji in the group even without Premium."),
            )
            .into_any_element()
    }
}
