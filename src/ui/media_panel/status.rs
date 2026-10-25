//! The emoji status selector: the emoji panel in status mode, after
//! tdesktop's `EmojiStatusPanel` (`info/profile/info_profile_emoji_status_panel.cpp`)
//! over `EmojiListWidget` in `Mode::EmojiStatus`. It opens under the main
//! menu with a search field, a first section (remove cell, themed, recent
//! and default statuses, the current one), the Collectibles section and the
//! installed emoji packs. A click sets the status with no end date; the
//! right-click menu sets it for a while or opens the "Set Status for..."
//! box (`status_time_box.rs`).
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use quill::emoji_status_selector::{
    MENU_DURATIONS, StatusPick, StatusSlot, format_mute_for, pick, status_slots,
};
use quill::lang::tr_with;

/// Where the selector is anchored (the main menu's corner).
#[derive(Clone, Copy, PartialEq)]
pub(in crate::ui) struct StatusTarget {
    pub position: Point<Pixels>,
}

impl MediaPanel {
    /// Where the panel shows when it is a selector (reaction or status).
    pub(in crate::ui) fn selector_anchor(&self) -> Option<Point<Pixels>> {
        self.reaction
            .map(|target| target.position)
            .or(self.status.map(|target| target.position))
    }
}

impl QuillApp {
    /// Open the selector at `position`. The caller has checked Premium.
    pub(in crate::ui) fn open_status_selector(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pickers.status_search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        let panel = &mut self.pickers.media_panel;
        panel.reaction = None;
        panel.status = Some(StatusTarget { position });
        panel.key = None;
        panel.active_section = 0;
        panel.category = None;
        panel.tab = PanelTab::Emoji;
        panel.open = true;
        if let Some(live) = self.live.as_mut() {
            // The installed emoji packs, and the statuses to list.
            let _ = live.driver.open_media_panel();
            if live.driver.load_emoji_status_choices().is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MISC_COULD_NOT_LOAD_EMOJI_STATUSES,
                )));
            }
        }
        cx.notify();
    }

    /// The panel closed: leave status mode and stop resolving its choices.
    pub(super) fn close_status_selector(&mut self) {
        if self.pickers.media_panel.status.take().is_some() {
            self.pickers.media_panel.key = None;
            if let Some(live) = self.live.as_mut() {
                live.driver.close_emoji_status_choices();
            }
        }
    }

    /// The status the account has now (`0`: none).
    fn current_status_id(&self) -> i64 {
        self.session()
            .and_then(|s| s.my_user_id.and_then(|id| s.user(id)))
            .map_or(0, |user| user.emoji_status_id)
    }

    /// The cell of an emoji that has loaded, by its custom emoji id.
    fn status_cell(&self, id: i64) -> Option<PanelCell> {
        let ix = self
            .session()?
            .stickers
            .emoji
            .custom_emoji_stickers
            .iter()
            .position(|item| item.custom_emoji_id == Some(id))?;
        Some(PanelCell::Custom {
            source: StickerSource::CustomEmoji,
            ix,
        })
    }

    /// The selector's rows (`EmojiListWidget` in status mode). A search
    /// keeps the statuses and packs whose emoji match.
    pub(super) fn build_status_rows(&self, query: &str) -> (Vec<PanelRow>, Vec<PanelSection>) {
        let mut rows = Vec::new();
        let mut sections = Vec::new();
        let Some(session) = self.session() else {
            return (rows, sections);
        };
        let emoji = &session.stickers.emoji;
        if !query.is_empty() {
            let strip = |e: &str| e.replace('\u{fe0f}', "");
            let matching: std::collections::HashSet<String> =
                quill::emoji_catalog::search(None, query)
                    .map(|entry| strip(entry.emoji))
                    .chain(emoji.keyword_emojis.iter().map(|e| strip(e)))
                    .collect();
            let listed = emoji
                .custom_emoji_stickers
                .iter()
                .enumerate()
                .filter(|(_, item)| matching.contains(&strip(&item.emoji)))
                .map(|(ix, _)| PanelCell::Custom {
                    source: StickerSource::CustomEmoji,
                    ix,
                });
            let cells: Vec<PanelCell> = listed.collect();
            if !cells.is_empty() {
                rows.push(PanelRow::Header {
                    label: tr(keys::Q_MISC_SEARCH_RESULTS).into(),
                    set_id: None,
                });
                push_grid(&mut rows, cells, EMOJI_COLS, false);
            }
            push_custom_packs(&mut rows, &mut sections, session, Some(&matching));
            return (rows, sections);
        }
        let recent: Vec<i64> = emoji
            .recent_statuses
            .iter()
            .map(|status| status.custom_emoji_id)
            .collect();
        let slots = status_slots(
            &emoji.themed_status_ids,
            &recent,
            &emoji.default_status_ids,
            self.current_status_id(),
        );
        let cells: Vec<PanelCell> = slots
            .into_iter()
            .filter_map(|slot| match slot {
                StatusSlot::Remove => Some(PanelCell::StatusRemove),
                StatusSlot::Emoji(id) => self.status_cell(id),
            })
            .collect();
        sections.push(PanelSection {
            label: tr(keys::Q_MISC_RECENTLY_USED).into(),
            icon: SectionIcon::Glyph(IconName::Clock),
            first_row: rows.len(),
        });
        push_grid(&mut rows, cells, EMOJI_COLS, false);
        let collectibles: Vec<PanelCell> = emoji
            .upgraded_gift_statuses
            .iter()
            .filter_map(|status| status.gift.as_ref())
            .filter_map(|gift| self.status_cell(gift.model_custom_emoji_id))
            .collect();
        if !collectibles.is_empty() {
            let label: SharedString = tr(keys::Q_EMOJI_STATUS_COLLECTIBLES).into();
            sections.push(PanelSection {
                label: label.clone(),
                icon: SectionIcon::Glyph(IconName::Star),
                first_row: rows.len(),
            });
            rows.push(PanelRow::Header {
                label,
                set_id: None,
            });
            push_grid(&mut rows, collectibles, EMOJI_COLS, false);
        }
        push_custom_packs(&mut rows, &mut sections, session, None);
        (rows, sections)
    }

    /// A cell was chosen: set the status for `secs` (`0` until removed),
    /// remove it (`None`), or show Premium when the account has none.
    pub(super) fn choose_panel_status(
        &mut self,
        id: Option<i64>,
        secs: i32,
        cx: &mut Context<Self>,
    ) {
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        self.close_media_panel(cx);
        match pick(id, secs, premium) {
            StatusPick::PremiumPreview => {
                self.open_premium_at(quill::premium_hub::PremiumFeature::EmojiStatus, cx)
            }
            StatusPick::Remove => self.change_emoji_status_for(None, 0, cx),
            StatusPick::Set { id, secs } => self.change_emoji_status_for(Some(id), secs, cx),
        }
    }

    /// The first cell: tdesktop's star, which removes the status.
    pub(super) fn status_remove_cell(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let label: SharedString = tr(keys::EMOJI_STATUS_REMOVE).into();
        div()
            .id(("panel-status-remove", id))
            .size(px(EMOJI_CELL))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent))
            .role(gpui_kit::Role::Button)
            .aria_label(label.clone())
            .tab_index(0)
            .tooltip({
                let label = label.clone();
                move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(label.clone()).build(window, cx)
                }
            })
            .on_click(cx.listener(|this, _, _, cx| this.choose_panel_status(None, 0, cx)))
            .child(
                Icon::new(IconName::Star)
                    .size(px(24.))
                    .text_color(cx.theme().primary),
            )
            .into_any_element()
    }

    /// Right-click on a status: Set Status for 1 hour, 8 hours, 1 day,
    /// 7 days, and Custom (`fillEmojiStatusMenu`).
    pub(super) fn with_status_duration_menu(
        &self,
        cell: Stateful<Div>,
        id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        cell.context_menu(move |mut menu, _, _| {
            for secs in MENU_DURATIONS {
                let owner = owner.clone();
                let label = tr_with(
                    keys::EMOJI_STATUS_MENU_DURATION_ANY,
                    &[("duration", format_mute_for(secs).as_str())],
                );
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.choose_panel_status(Some(id), secs, cx);
                    });
                }));
            }
            let owner = owner.clone();
            menu.item(
                PopupMenuItem::new(tr(keys::MANAGE_MESSAGES_TTL_AFTER_CUSTOM)).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.close_media_panel(cx);
                            this.open_status_time_box(id, window, cx);
                        });
                    },
                ),
            )
        })
        .into_any_element()
    }
}
