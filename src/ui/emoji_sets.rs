//! The emoji packs box (Settings > Chat Settings > Emoji sets): the packs a
//! user has, the trending ones and a search, laid out like the stickers box
//! rows. Packs, their cover previews and the search results load on their
//! own; a row opens the pack in the sticker set box.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use super::{DialogKind, QuillShell};
use crate::ui::scroll_load::scrolled_near_end;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Input;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::emoji::EmojiSetTab;
use quill::ids::ChatId;
use quill::lang::{keys, tr, trn};
use quill::local_path::sandboxed_display_path;
use quill::state::RequestPurpose;
use quill::telegram::envelope::StickerSetInfo;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// How long typing pauses before the search goes out.
const SEARCH_DELAY: Duration = Duration::from_millis(300);
/// Distance from the end of the list at which the next page is requested.
/// Cover previews shown on a row.
const ROW_COVERS: usize = 4;
const COVER_SIZE: f32 = 28.;

const TABS: [EmojiSetTab; 2] = [EmojiSetTab::Installed, EmojiSetTab::Trending];

impl QuillApp {
    pub(super) fn open_emoji_sets(&mut self, cx: &mut Context<Self>) {
        self.pickers.emoji_set_return_tab = EmojiSetTab::Installed;
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_emoji_sets().is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_LOAD_EMOJI_PACKS,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.emoji.open = true;
        }
        cx.notify();
    }
    fn close_emoji_sets(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.stickers.emoji.open = false;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.emoji.open = false;
        }
        cx.notify();
    }
    pub(super) fn select_emoji_sets_tab(
        &mut self,
        tab: EmojiSetTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pickers.emoji_set_keystroke = self.pickers.emoji_set_keystroke.wrapping_add(1);
        self.pickers.emoji_set_return_tab = tab;
        self.pickers
            .emoji_set_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.show_emoji_sets_tab(tab, cx);
    }
    fn show_emoji_sets_tab(&mut self, tab: EmojiSetTab, cx: &mut Context<Self>) {
        self.pickers
            .emoji_set_scroll
            .set_offset(point(px(0.), px(0.)));
        if let Some(live) = self.live.as_mut() {
            if live.driver.select_emoji_set_tab(tab).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_LOAD_EMOJI_PACKS,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.emoji.tab = tab;
        }
        cx.notify();
    }
    /// The search field changed: once typing pauses, the packs matching it
    /// replace the list; clearing the field brings the last tab back.
    pub(super) fn emoji_sets_search_typed(&mut self, cx: &mut Context<Self>) {
        self.pickers.emoji_set_keystroke = self.pickers.emoji_set_keystroke.wrapping_add(1);
        let keystroke = self.pickers.emoji_set_keystroke;
        let query = self
            .pickers
            .emoji_set_search_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        if query.is_empty() {
            let tab = self.pickers.emoji_set_return_tab;
            if self
                .session()
                .is_some_and(|s| s.stickers.emoji.tab == EmojiSetTab::Search)
            {
                self.show_emoji_sets_tab(tab, cx);
            }
            return;
        }
        self.spawn_scoped(cx, async move |this, cx| {
            cx.background_executor().timer(SEARCH_DELAY).await;
            this.update(cx, |this, cx| {
                if this.pickers.emoji_set_keystroke == keystroke {
                    this.search_emoji_sets_for(&query, cx);
                }
            })
            .ok();
        })
        .detach();
    }
    fn search_emoji_sets_for(&mut self, query: &str, cx: &mut Context<Self>) {
        self.pickers
            .emoji_set_scroll
            .set_offset(point(px(0.), px(0.)));
        if let Some(live) = self.live.as_mut() {
            if live.driver.search_emoji_packs(query).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_SEARCH_EMOJI_PACKS,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let needle = query.to_lowercase();
            let emoji = &mut session.stickers.emoji;
            emoji.tab = EmojiSetTab::Search;
            emoji.search_query = query.to_string();
            emoji.found_sets = emoji
                .installed_sets
                .iter()
                .chain(&emoji.trending_sets)
                .filter(|set| set.title.to_lowercase().contains(&needle))
                .cloned()
                .collect();
        }
        cx.notify();
    }
    fn emoji_sets_load_more(&mut self, cx: &mut Context<Self>) {
        let scroll = &self.pickers.emoji_set_scroll;
        if !scrolled_near_end(scroll)
            || self
                .session()
                .is_none_or(|s| s.stickers.emoji.tab != EmojiSetTab::Trending)
        {
            return;
        }
        if let Some(live) = self.live.as_mut()
            && live.driver.more_trending_emoji_packs().is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_LOAD_MORE_EMOJI,
            )));
        }
        cx.notify();
    }
    /// Open the pack in the sticker set box, where its emoji can be looked
    /// at and the pack added or removed.
    fn preview_emoji_set(&mut self, id: i64, cx: &mut Context<Self>) {
        self.view_message_sticker_set(id, cx);
    }
    fn install_emoji_set(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.set_emoji_pack_installed(id, true) {
                    Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MEDIA_INSTALLING_EMOJI_PACK)),
                    Ok(None) => StatusNote::silent(tr(keys::Q_MEDIA_AN_EMOJI_PACK_IS_ALREADY)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_INSTALL_EMOJI_PACK)),
                });
        }
        cx.notify();
    }

    /// One cover preview: the downloaded thumbnail, or the emoji itself
    /// until it is there.
    fn emoji_cover(&self, cover: &quill::telegram::envelope::StickerItem) -> AnyElement {
        let roots = self.media_display_roots();
        let path = cover
            .display_file_id()
            .and_then(|id| {
                self.session()
                    .and_then(|s| s.media.files.get(&id.0))
                    .and_then(|f| f.usable_path())
            })
            .and_then(|p| sandboxed_display_path(p, &roots));
        let cell = div()
            .size(px(COVER_SIZE))
            .flex()
            .items_center()
            .justify_center();
        match path {
            Some(path) => cell
                .child(
                    img(path)
                        .size(px(COVER_SIZE))
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            None => cell.child(cover.emoji.clone()).into_any_element(),
        }
    }

    fn emoji_pack_row(
        &self,
        set: &StickerSetInfo,
        tab: EmojiSetTab,
        busy: Option<(i64, bool)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = set.id;
        let title = if set.title.is_empty() {
            set.name.clone()
        } else {
            set.title.clone()
        };
        let working = busy.is_some_and(|(set_id, _)| set_id == id);
        let action = if tab == EmojiSetTab::Installed {
            Button::new(format!("emoji-pack-remove-{id}"))
                .icon(gpui_kit::assets::IconName::X)
                .ghost()
                .small()
                .disabled(working)
                .tooltip(tr(keys::BOX_REMOVE))
                .accessibility_label(format!("{} {title}", tr(keys::BOX_REMOVE)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_group_confirm(
                        ChatId(0),
                        GroupConfirmAction::RemoveEmojiSet { set_id: id },
                        cx,
                    );
                }))
                .into_any_element()
        } else if set.is_installed {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_MEDIA_INSTALLED))
                .into_any_element()
        } else {
            Button::new(format!("emoji-pack-add-{id}"))
                .label(tr(keys::STICKERS_FEATURED_ADD))
                .primary()
                .small()
                .loading(working)
                .disabled(working)
                .on_click(cx.listener(move |this, _, _, cx| this.install_emoji_set(id, cx)))
                .into_any_element()
        };
        let covers = set
            .covers
            .iter()
            .take(ROW_COVERS)
            .fold(h_flex().flex_none().gap_1(), |row, cover| {
                row.child(self.emoji_cover(cover))
            });
        h_flex()
            .id(format!("emoji-pack-row-{id}"))
            .w_full()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .child(
                Button::new(format!("emoji-pack-open-{id}"))
                    .ghost()
                    .flex_1()
                    .min_w_0()
                    .h_auto()
                    .justify_start()
                    .accessibility_label(title.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.preview_emoji_set(id, cx)))
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_3()
                            .child(covers)
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .items_start()
                                    .child(div().truncate().font_medium().child(title))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(trn(keys::CUSTOM_EMOJI_COUNT, set.size)),
                                    ),
                            ),
                    ),
            )
            .child(action)
            .into_any_element()
    }

    fn emoji_sets_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let no_panel = quill::emoji::EmojiPanel::default();
        let panel = session.map_or(&no_panel, |s| &s.stickers.emoji);
        let loading = session.is_some_and(|s| {
            s.requests.has_purpose(match panel.tab {
                EmojiSetTab::Installed => RequestPurpose::GetInstalledEmojiSets,
                EmojiSetTab::Trending => RequestPurpose::GetTrendingEmojiSets,
                EmojiSetTab::Search => RequestPurpose::SearchEmojiSets,
            })
        });
        let owner = cx.entity().downgrade();
        let tabs = TabBar::new("emoji-sets-tabs")
            .underline()
            .children(TABS.iter().map(|tab| {
                Tab::new().label(match tab {
                    EmojiSetTab::Installed => tr(keys::Q_MEDIA_INSTALLED),
                    _ => tr(keys::STICKERS_FEATURED_TAB),
                })
            }))
            .selected_index(
                TABS.iter()
                    .position(|tab| *tab == self.pickers.emoji_set_return_tab)
                    .unwrap_or(0),
            )
            .on_click(move |ix, window, cx| {
                if let Some(tab) = TABS.get(*ix).copied() {
                    let _ =
                        owner.update(cx, |this, cx| this.select_emoji_sets_tab(tab, window, cx));
                }
            });
        let sets: Vec<&StickerSetInfo> = match panel.tab {
            EmojiSetTab::Installed => session.map(|s| s.ordered_emoji_packs()).unwrap_or_default(),
            EmojiSetTab::Trending => panel.trending_sets.iter().collect(),
            EmojiSetTab::Search => panel.found_sets.iter().collect(),
        };
        let rows = sets.iter().fold(v_flex().w_full(), |rows, set| {
            rows.child(self.emoji_pack_row(set, panel.tab, panel.mutating_set, cx))
        });
        let status = if loading && sets.is_empty() {
            Some(tr(keys::Q_MEDIA_LOADING_EMOJI_PACKS))
        } else if panel.failed || panel.mutation_failed {
            Some(tr(keys::Q_MEDIA_COULD_NOT_UPDATE_EMOJI_PACKS))
        } else if sets.is_empty() && !loading {
            Some(tr(keys::Q_MEDIA_NO_EMOJI_PACKS_FOUND))
        } else {
            None
        };
        v_flex()
            .id("emoji-sets-box")
            .w(px(420.))
            .max_w_full()
            .gap_2()
            .child(tabs)
            .child(
                Input::new(&self.pickers.emoji_set_search_input)
                    .cleanable(true)
                    .aria_label(tr(keys::Q_MEDIA_SEARCH_EMOJI_PACKS)),
            )
            .when_some(status, |body, status| {
                body.child(
                    div()
                        .px_3()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(status),
                )
            })
            .child(
                div()
                    .id("emoji-pack-rows")
                    .role(Role::ScrollView)
                    .aria_label(tr(keys::Q_MEDIA_EMOJI_PACKS))
                    .h(px(360.))
                    .overflow_y_scroll()
                    .track_scroll(&self.pickers.emoji_set_scroll)
                    .on_scroll_wheel(cx.listener(|this, _, _, cx| this.emoji_sets_load_more(cx)))
                    .child(rows),
            )
            .into_any_element()
    }
    pub(super) fn build_emoji_sets_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let close = QuillShell::on_close_kind(app, shell, DialogKind::EmojiSets, |this, _, cx| {
            this.close_emoji_sets(cx)
        });
        app.update(cx, |this, cx| {
            if let Some(live) = this.live.as_mut() {
                let _ = live.driver.ensure_emoji_pack_covers();
            }
            let body = Rc::new(RefCell::new(Some(this.emoji_sets_panel(cx))));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_MEDIA_EMOJI_PACKS,
                )))
                .content(move |content, _, _| {
                    content.child(
                        body.borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element()),
                    )
                })
                .on_close(close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    EmojiSets => DialogSpec::new(
        5100,
        |app| app.session().is_some_and(|s| s.stickers.emoji.open),
        QuillApp::build_emoji_sets_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "emoji_sets_tests.rs"]
mod tests;
