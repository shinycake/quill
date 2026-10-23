//! The reaction selector's rows: the emoji panel in reaction mode.

use super::*;

impl QuillApp {
    /// Telegram Desktop's full reaction selector: every reaction the
    /// message allows, then (when the chat allows custom emoji) the
    /// installed custom emoji packs. The search filters both.
    pub(super) fn build_reaction_rows(&self, query: &str) -> (Vec<PanelRow>, Vec<PanelSection>) {
        use quill::state::ReactionChoice;
        let mut rows = Vec::new();
        let mut sections = Vec::new();
        let (Some(session), Some(target)) = (self.session(), self.pickers.media_panel.reaction)
        else {
            return (rows, sections);
        };
        let Some(options) = session
            .stickers
            .message_reaction_options
            .ready_for(target.chat_id, target.message_id)
        else {
            return (rows, sections);
        };
        // Emoji matching the search (variation selectors ignored).
        let strip = |e: &str| e.replace('\u{fe0f}', "");
        let matches: Option<std::collections::HashSet<String>> = (!query.is_empty()).then(|| {
            quill::emoji_catalog::search(None, query)
                .map(|entry| strip(entry.emoji))
                .chain(
                    session
                        .stickers
                        .emoji
                        .keyword_emojis
                        .iter()
                        .map(|e| strip(e)),
                )
                .collect()
        });
        let wanted = |emoji: &str| {
            matches
                .as_ref()
                .is_none_or(|set| set.contains(&strip(emoji)))
        };
        let cells: Vec<PanelCell> = options
            .all()
            .into_iter()
            .filter_map(|choice| match choice {
                ReactionChoice::Emoji(emoji) => wanted(&emoji).then(|| {
                    PanelCell::Emoji(crate::ui::reactions::emoji_presentation(&emoji).into())
                }),
                ReactionChoice::CustomEmoji(id) => {
                    let (ix, sticker) = session
                        .stickers
                        .emoji
                        .custom_emoji_stickers
                        .iter()
                        .enumerate()
                        .find(|(_, item)| item.custom_emoji_id == Some(id))?;
                    wanted(&sticker.emoji).then_some(PanelCell::Custom {
                        source: StickerSource::CustomEmoji,
                        ix,
                    })
                }
            })
            .collect();
        if !cells.is_empty() {
            sections.push(PanelSection {
                label: tr(keys::Q_MISC_REACTIONS).into(),
                icon: SectionIcon::Glyph(gpui_kit::assets::IconName::Heart),
                first_row: rows.len(),
            });
            push_grid(&mut rows, cells, EMOJI_COLS, false);
        }
        if options.allow_custom_emoji {
            push_custom_packs(&mut rows, &mut sections, session, matches.as_ref());
        }
        (rows, sections)
    }
}
