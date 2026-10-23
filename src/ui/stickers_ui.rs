//! Sticker collection picker and set management.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use super::pressable::PressableDiv;
use super::*;
use super::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId};
use quill::lang::{keys, tr, tr_with, trn};
use quill::local_path::sandboxed_display_path;
use quill::state::StickersPurpose;
use quill::state::{RequestPurpose, StickerTab};
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    fn batch_install_sticker_sets(&mut self, ids: &[i64], cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.manage_sticker_sets(ids, true) {
                    Ok(sent) => StatusNote::silent(tr_with(
                        keys::Q_MEDIA_INSTALLING_STICKER_SETS,
                        &[("sent", &sent.to_string())],
                    )),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_START_STICKER_BATCH)),
                });
        }
        cx.notify();
    }

    pub(super) fn sync_sticker_suggestions(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.update_sticker_suggestions(text).is_err()
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MEDIA_COULD_NOT_LOAD_STICKER_SUGGESTIONS,
            )));
            cx.notify();
        }
        // The suggestions row is part of the composer.
        self.notify_composer(cx);
    }

    /// A keystroke's sticker and animated-emoji suggestions: the driver is
    /// asked only when the text ends in an emoji or something is still held
    /// to clear, and the composer is notified once.
    pub(super) fn sync_composer_suggestions(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && !live.driver.composer_suggestions_idle(text)
        {
            if live.driver.update_sticker_suggestions(text).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_LOAD_STICKER_SUGGESTIONS,
                )));
                cx.notify();
            }
            if let Some(live) = self.live.as_mut()
                && live.driver.update_animated_emoji_suggestion(text).is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULD_NOT_LOAD_ANIMATED_EMOJI,
                )));
                cx.notify();
            }
        }
        self.notify_composer(cx);
    }

    pub(super) fn animated_emoji_suggestion(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let sticker = session.stickers.emoji.animated_emoji.as_ref()?;
        let row = div()
            .id("animated-emoji-suggestion")
            .flex()
            .gap_2()
            .child(self.sticker_cell(sticker, "animated-emoji", cx));
        Some(row.into_any_element())
    }

    fn set_sticker_suggest_mode(
        &mut self,
        mode: quill::sticker_suggest::StickerSuggestMode,
        cx: &mut Context<Self>,
    ) {
        self.set_media_pref(|prefs| prefs.sticker_suggest_mode = mode, cx);
        if let Some(live) = self.live.as_mut() {
            drop(
                live.driver
                    .session
                    .requests
                    .take_purpose(RequestPurpose::SuggestStickers),
            );
            live.driver.session.clear_sticker_suggestions();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.clear_sticker_suggestions();
        }
        let text = self.composer.read(cx).value().to_string();
        self.sync_sticker_suggestions(&text, cx);
    }

    pub(super) fn sticker_suggestions_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if session.stickers.stickers.suggestions.is_empty() {
            return None;
        }
        let row = div()
            .id("sticker-suggestions")
            .flex()
            .gap_2()
            .overflow_x_scroll()
            .children(
                session
                    .stickers
                    .stickers
                    .suggestions
                    .iter()
                    .map(|sticker| self.sticker_cell(sticker, "suggest", cx)),
            );
        Some(row.into_any_element())
    }

    pub(super) fn sticker_cell(
        &self,
        sticker: &quill::telegram::envelope::StickerItem,
        prefix: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let roots = self.media_display_roots();
        let file_id = sticker.file_id;
        let emoji = sticker.emoji.clone();
        let width = sticker.width;
        let height = sticker.height;
        let thumb = sticker
            .thumb_file_id
            .filter(|id| id.0 != 0)
            .map(|id| (id, sticker.thumb_width, sticker.thumb_height));
        let display_id = sticker.display_file_id();
        let path = [display_id, sticker.thumb_file_id]
            .into_iter()
            .flatten()
            .filter_map(|id| self.session()?.media.files.get(&id.0)?.usable_path())
            .find_map(|path| sandboxed_display_path(path, &roots));
        let label = if emoji.is_empty() {
            tr(keys::Q_MEDIA_STICKER).to_string()
        } else {
            emoji.clone()
        };
        let cell_id = format!("sticker-{prefix}-{}-{}", sticker.set_id, sticker.id);
        let animated = self.sticker_image(file_id, sticker.format, cx);
        let source = animated
            .map(ImageSource::Render)
            .or_else(|| path.map(ImageSource::from));
        let cell = if let Some(source) = source {
            img(source)
                .id(SharedString::from(cell_id.clone()))
                .w(px(72.))
                .h(px(72.))
                .aspect_ratio(px(72.) / px(72.))
                .rounded_md()
                .object_fit(ObjectFit::Contain)
                .with_fallback({
                    let label = label.clone();
                    move || {
                        div()
                            .w(px(72.))
                            .h(px(72.))
                            .rounded_md()
                            .bg(fill_muted())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(label.clone())
                            .into_any_element()
                    }
                })
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(cell_id))
                .w(px(72.))
                .h(px(72.))
                .rounded_md()
                .bg(bg_subtle())
                .border_1()
                .border_color(text_muted())
                .flex()
                .items_center()
                .justify_center()
                .child(label)
                .into_any_element()
        };
        let premium_mark = self
            .session()
            .filter(|s| sticker.requires_premium && s.premium_badges_shown())
            .map(|s| crate::ui::premium_mark::premium_mark(s.my_is_premium()));
        let cell = div()
            .id(SharedString::from(format!(
                "sticker-button-{prefix}-{}-{}",
                sticker.set_id, sticker.id
            )))
            .relative()
            .w(px(72.))
            .h(px(72.))
            .rounded_md()
            .pressable(cx.theme())
            .role(gpui_kit::Role::Button)
            .aria_label(tr_with(
                keys::Q_MEDIA_SEND_STICKER,
                &[("emoji", &emoji.to_string())],
            ))
            .tab_index(0)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.send_sticker_pick(file_id, emoji.clone(), width, height, thumb, cx);
            }))
            .child(cell)
            .children(premium_mark);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(cell)
            .into_any_element()
    }

    /// Telegram Desktop's start page of an empty private chat: "No
    /// messages here yet..." with a random hello sticker
    /// (`getGreetingStickers`) that sends itself when clicked
    /// (`history_view_about_view.cpp`, `GenerateChatIntro`). `None` for
    /// groups, bots, Saved Messages and while no greeting is known.
    pub(super) fn greeting_intro(
        &mut self,
        chat: Option<&quill::state::ChatSummary>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let chat = chat?;
        let quill::telegram::envelope::ChatKind::Private { user_id } = chat.kind else {
            return None;
        };
        let session = self.session()?;
        if session.my_user_id == Some(user_id.0) || session.is_bot_user(user_id.0) {
            return None;
        }
        if let Some(intro) = self.business_start_page_intro(user_id.0, cx) {
            return Some(intro);
        }
        let session = self.session()?;
        if !session.stickers.stickers.greeting_loaded {
            let weak = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, cx| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.fetch_greeting_stickers();
                        cx.notify();
                    }
                });
            });
        }
        let sticker = greeting_pick(&session.stickers.stickers.greeting, chat.id.0)?.clone();
        if let Some(file) = sticker.display_file_id()
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.ensure_media_files(&[file]);
        }
        let cell = self.sticker_cell(&sticker, "greeting", cx);
        Some(
            div()
                .id("greeting-intro")
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .py_3()
                        .rounded_lg()
                        .bg(bg_subtle())
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .child(tr(keys::CHAT_INTRO_DEFAULT_TITLE)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(text_muted())
                                .child(tr(keys::CHAT_INTRO_DEFAULT_MESSAGE)),
                        )
                        .child(cell),
                )
                .into_any_element(),
        )
    }

    pub(super) fn open_archived_stickers(&mut self, cx: &mut Context<Self>) {
        self.settings.sticker_settings_open = true;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.stickers.stickers.open = true;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.stickers.open = true;
        }
        self.select_sticker_tab(StickerTab::Archived, cx);
    }

    pub(super) fn build_archived_stickers_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ArchivedStickers, |this, _, cx| {
                this.close_sticker_panel(cx)
            });
        app.update(cx, |this, cx| {
            let body = Rc::new(RefCell::new(Some(
                this.sticker_picker_panel(cx).into_any_element(),
            )));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_MEDIA_ARCHIVED_STICKERS,
                )))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        content.child(
                            body.borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element()),
                        )
                    },
                ))
                .on_close(on_close)
        })
    }

    fn more_archived_stickers(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.fetch_archived_stickers(true) {
                    Ok(_) => StatusNote::info(tr(keys::Q_MEDIA_LOADING_ARCHIVED_STICKERS)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_LOAD_ARCHIVED_STICKERS)),
                });
        }
        cx.notify();
    }

    fn archive_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.manage_sticker_set(set_id, false, true) {
                    Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MEDIA_ARCHIVING_STICKER_SET)),
                    Ok(None) => {
                        StatusNote::silent(tr(keys::Q_MEDIA_STICKER_SET_UPDATE_ALREADY_PENDING))
                    }
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_ARCHIVE_STICKER_SET)),
                });
        }
        cx.notify();
    }

    fn select_sticker_tab(&mut self, tab: StickerTab, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.select_sticker_tab(tab) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_LEFTOVERS_STICKERS)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_LOAD_STICKER_TAB)),
                });
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.stickers.tab = tab;
        }
        cx.notify();
    }

    fn more_trending_stickers(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.fetch_trending_stickers(true) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MEDIA_LOADING_MORE_TRENDING_STICKERS)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_LOAD_MORE_TRENDING)),
                });
        }
        cx.notify();
    }

    fn favorite_sticker(&mut self, id: FileId, favorite: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.set_favorite_sticker(id, favorite) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MEDIA_UPDATING_FAVORITE_STICKERS)),
                    Err(_) => {
                        StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_UPDATE_FAVORITE_STICKER))
                    }
                });
        }
        cx.notify();
    }

    fn clear_recent_stickers(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.clear_recent_stickers() {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MEDIA_CLEARING_RECENT_STICKERS)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_CLEAR_RECENT_STICKERS)),
                });
        }
        cx.notify();
    }

    fn search_sticker_picker(&mut self, cx: &mut Context<Self>) {
        let query = self
            .pickers
            .sticker_search_input
            .read(cx)
            .value()
            .to_string();
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.search_sticker_picker(&query) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MEDIA_SEARCHING_STICKERS)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_SEARCH_STICKERS)),
                });
        }
        cx.notify();
    }

    fn more_sticker_search_results(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.more_sticker_search_results() {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MEDIA_LOADING_MORE_STICKERS)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_LOAD_MORE_STICKERS)),
                });
        }
        cx.notify();
    }

    fn install_sticker_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.manage_sticker_set(set_id, true, false) {
                    Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MEDIA_INSTALLING_STICKER_SET)),
                    Ok(None) => StatusNote::silent(tr(keys::Q_MEDIA_STICKER_SET_UPDATE_ALREADY_IN)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_INSTALL_STICKER_SET)),
                });
        }
        cx.notify();
    }

    fn reorder_sticker_set(&mut self, source: i64, target: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.reorder_sticker_set(source, target) {
                    Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MEDIA_SAVING_STICKER_SET_ORDER)),
                    Ok(None) => {
                        StatusNote::silent(tr(keys::Q_MEDIA_STICKER_ORDER_UNCHANGED_OR_UPDATE))
                    }
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_REORDER_STICKER_SETS)),
                });
        }
        cx.notify();
    }

    fn sticker_set_action(
        &self,
        set: &quill::telegram::envelope::StickerSetInfo,
        prefix: &str,
        cx: &mut Context<Self>,
    ) -> Button {
        let set_id = set.id;
        let pending = self.session().is_some_and(|session| {
            [(true, false), (false, true), (false, false)]
                .into_iter()
                .any(|(installed, archived)| {
                    session.requests.has_purpose(RequestPurpose::Stickers(
                        StickersPurpose::ManageStickerSet {
                            set_id,
                            installed,
                            archived,
                        },
                    ))
                })
        });
        let button = Button::new(format!("sticker-{prefix}-action-{set_id}"))
            .disabled(pending)
            .ghost();
        if set.is_installed {
            button
                .label(if pending {
                    tr(keys::Q_MEDIA_UPDATING)
                } else {
                    tr(keys::STICKERS_REMOVE_PACK_CONFIRM)
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_group_confirm(
                        ChatId(0),
                        GroupConfirmAction::RemoveStickerSet { set_id },
                        cx,
                    )
                }))
        } else {
            button
                .label(if pending {
                    tr(keys::Q_MEDIA_INSTALLING)
                } else if self.session().is_some_and(|s| {
                    s.stickers
                        .stickers
                        .archived
                        .iter()
                        .any(|set| set.id == set_id)
                }) {
                    tr(keys::EMOJI_PREMIUM_RESTORE)
                } else {
                    tr(keys::Q_MEDIA_INSTALL)
                })
                .on_click(cx.listener(move |this, _, _, cx| this.install_sticker_set(set_id, cx)))
        }
    }

    pub(super) fn sticker_picker_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self
            .session()
            .map(|session| session.stickers.stickers.clone())
            .unwrap_or_default();
        let mut sets = div()
            .id("sticker-set-row")
            .flex()
            .flex_wrap()
            .gap_1()
            .max_h(px(96.))
            .overflow_y_scroll();
        let mut tabs = div().flex().flex_wrap().items_center().gap_1();
        for (tab, label) in [
            (StickerTab::Installed, tr(keys::Q_MEDIA_INSTALLED)),
            (StickerTab::Recent, tr(keys::Q_MEDIA_RECENT)),
            (StickerTab::Favorites, tr(keys::Q_MEDIA_FAVORITES)),
            (StickerTab::Trending, tr(keys::STICKERS_FEATURED_TAB)),
            (StickerTab::Search, tr(keys::Q_MEDIA_SEARCH)),
            (StickerTab::Archived, tr(keys::STICKERS_ARCHIVED_TAB)),
        ] {
            tabs = tabs.child(
                Button::new(format!("sticker-tab-{tab:?}"))
                    .label(label)
                    .ghost()
                    .small()
                    .selected(panel.tab == tab)
                    .on_click(cx.listener(move |this, _, _, cx| this.select_sticker_tab(tab, cx))),
            );
        }
        let mode = self
            .session()
            .map(|s| s.settings.media_prefs.sticker_suggest_mode)
            .unwrap_or_default();
        let query = panel.search_query.to_lowercase();
        let mut seen = std::collections::HashSet::new();
        let visible_sets: Vec<_> = match panel.tab {
            StickerTab::Installed => panel.sets.iter().collect(),
            StickerTab::Trending => panel.trending.iter().collect(),
            StickerTab::Archived => panel.archived.iter().collect(),
            StickerTab::Search => panel
                .found_sets
                .iter()
                .chain(panel.sets.iter().filter(|set| {
                    !query.is_empty()
                        && (set.title.to_lowercase().contains(&query)
                            || set.name.to_lowercase().contains(&query))
                }))
                .filter(|set| seen.insert(set.id))
                .collect(),
            _ => vec![],
        };
        let batch_ids: Vec<_> = visible_sets
            .iter()
            .filter(|set| !set.is_installed)
            .map(|set| set.id)
            .collect();
        let batch_idle = panel.batch_pending.is_empty();
        let can_remove_all = panel.tab == StickerTab::Installed && !panel.sets.is_empty();
        let on_recent = panel.tab == StickerTab::Recent && !panel.recent.is_empty();
        // Panel management lives in an overflow menu so the picker stays a
        // picker: emoji suggestions, clear recent, batch install/remove.
        let owner = cx.entity().downgrade();
        let more_menu = Button::new("sticker-panel-more")
            .icon(IconName::Ellipsis)
            .ghost()
            .small()
            .tooltip(tr(keys::Q_MEDIA_STICKER_OPTIONS))
            .accessibility_label(tr(keys::Q_MEDIA_STICKER_OPTIONS))
            .dropdown_menu(move |mut menu, _, _| {
                use quill::sticker_suggest::StickerSuggestMode;
                menu = menu.label(tr(keys::Q_MEDIA_SUGGEST_STICKERS_BY_EMOJI));
                for (value, label) in [
                    (
                        StickerSuggestMode::InstalledAndRecommended,
                        tr(keys::Q_MEDIA_INSTALLED_AND_RECOMMENDED),
                    ),
                    (
                        StickerSuggestMode::InstalledOnly,
                        tr(keys::Q_MEDIA_INSTALLED_ONLY),
                    ),
                    (StickerSuggestMode::None, tr(keys::SETTINGS_COLOR_EMOJI_OFF)),
                ] {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).checked(mode == value).on_click(
                        move |_, _, cx| {
                            let _ = owner
                                .update(cx, |this, cx| this.set_sticker_suggest_mode(value, cx));
                        },
                    ));
                }
                menu = menu.separator();
                if on_recent {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(tr(keys::Q_MEDIA_CLEAR_RECENT_STICKERS)).on_click(
                            move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| this.clear_recent_stickers(cx));
                            },
                        ),
                    );
                }
                if !batch_ids.is_empty() {
                    let owner = owner.clone();
                    let ids = batch_ids.clone();
                    menu = menu.item(
                        PopupMenuItem::new(trn(keys::Q_MEDIA_ADD_SETS_SHOWN, ids.len()))
                            .disabled(!batch_idle)
                            .on_click(move |_, _, cx| {
                                let ids = ids.clone();
                                let _ = owner.update(cx, |this, cx| {
                                    this.batch_install_sticker_sets(&ids, cx)
                                });
                            }),
                    );
                }
                if can_remove_all {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(tr(keys::Q_MEDIA_REMOVE_ALL_INSTALLED_SETS))
                            .disabled(!batch_idle)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.open_group_confirm(
                                        ChatId(0),
                                        GroupConfirmAction::RemoveInstalledStickerSets,
                                        cx,
                                    );
                                });
                            }),
                    );
                }
                menu
            });
        for set in &visible_sets {
            let set_id = set.id;
            let selected = panel.selected_set_id == Some(set_id);
            let title = if set.title.is_empty() {
                set.name.clone()
            } else {
                set.title.clone()
            };
            let row = div()
                .id(format!("sticker-set-drag-{set_id}"))
                .child(
                    Button::new(format!("sticker-set-{set_id}"))
                        .label(title.clone())
                        .ghost()
                        .small()
                        .selected(selected)
                        .tooltip(tr_with(
                            keys::Q_LEFTOVERS_STICKER_SET_SIZE,
                            &[("count", &set.size.to_string())],
                        ))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.select_sticker_set(set_id, cx)),
                        ),
                )
                .when(panel.tab == StickerTab::Installed, |row| {
                    row.cursor_move()
                        .on_drag(
                            StickerSetDrag {
                                id: set_id,
                                title: title.clone(),
                            },
                            |drag: &StickerSetDrag, _, _, cx| cx.new(|_| drag.clone()),
                        )
                        .on_drop(cx.listener(move |this, drag: &StickerSetDrag, _, cx| {
                            this.reorder_sticker_set(drag.id, set_id, cx);
                        }))
                });
            sets = sets.child(row);
        }
        if panel.tab == StickerTab::Trending && panel.trending_next_offset < panel.trending_total {
            sets = sets.child(
                Button::new("sticker-trending-more")
                    .label(tr(keys::Q_MEDIA_MORE_TRENDING_SETS))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_trending_stickers(cx))),
            );
        }
        if panel.tab == StickerTab::Archived && panel.archived_has_more {
            sets = sets.child(
                Button::new("more-archived-stickers")
                    .label(tr(keys::Q_MEDIA_MORE_ARCHIVED_SETS))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_archived_stickers(cx))),
            );
        }
        let mut grid = div().id("sticker-grid").flex().flex_wrap().gap_2();
        for sticker in panel.visible_stickers() {
            let file_id = sticker.file_id;
            let cell = self.sticker_cell(sticker, "pick", cx);
            let favorite = panel.favorites.iter().any(|item| item.file_id == file_id);
            let in_recent = panel.tab == StickerTab::Recent
                && panel.recent.iter().any(|item| item.file_id == file_id);
            let owner = cx.entity().downgrade();
            grid = grid.child(
                div()
                    .id(format!("sticker-cell-{}-{}", sticker.set_id, sticker.id))
                    .child(cell)
                    .context_menu(move |menu, _, _| {
                        let favorite_owner = owner.clone();
                        let recent_owner = owner.clone();
                        let menu = menu.item(
                            PopupMenuItem::new(if favorite {
                                tr(keys::FAVED_STICKERS_REMOVE)
                            } else {
                                tr(keys::FAVED_STICKERS_ADD)
                            })
                            .on_click(move |_, _, cx| {
                                let _ = favorite_owner.update(cx, |this, cx| {
                                    this.favorite_sticker(file_id, !favorite, cx)
                                });
                            }),
                        );
                        if in_recent {
                            menu.item(PopupMenuItem::new(tr(keys::EMOJI_REMOVE_RECENT)).on_click(
                                move |_, _, cx| {
                                    let _ = recent_owner.update(cx, |this, cx| {
                                        if let Some(live) = this.live.as_mut() {
                                            let _ = live.driver.remove_recent_sticker(file_id);
                                        }
                                        cx.notify();
                                    });
                                },
                            ))
                        } else {
                            menu
                        }
                    }),
            );
        }
        if panel.tab == StickerTab::Search
            && panel.search_has_more
            && panel.selected_set_id.is_none()
        {
            grid = grid.child(
                Button::new("sticker-search-more")
                    .label(tr(keys::Q_MEDIA_MORE_STICKERS))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_sticker_search_results(cx))),
            );
        }
        let mut preview = div().flex().items_center().justify_between().gap_2();
        if let Some(set) = visible_sets
            .iter()
            .find(|set| Some(set.id) == panel.selected_set_id)
        {
            preview = preview
                .child(div().font_semibold().child(tr_with(
                    keys::Q_MEDIA_STICKERS,
                    &[
                        ("title", &set.title.to_string()),
                        ("size", &set.size.to_string()),
                    ],
                )))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(self.sticker_set_action(set, "preview", cx))
                        .when(set.is_installed, |row| {
                            let set_id = set.id;
                            row.child(
                                Button::new(format!("archive-set-{set_id}"))
                                    .label(tr(keys::Q_MEDIA_ARCHIVE))
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.archive_sticker_set(set_id, cx)
                                    })),
                            )
                        }),
                );
            if panel.tab == StickerTab::Search {
                preview = preview.child(
                    Button::new("sticker-search-back")
                        .label(tr(keys::Q_MEDIA_BACK_TO_RESULTS))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.select_sticker_tab(StickerTab::Search, cx)
                        })),
                );
            }
        }
        let loading_tab = self.session().is_some_and(|session| {
            let purpose = match panel.tab {
                StickerTab::Installed => RequestPurpose::GetInstalledStickerSets,
                StickerTab::Recent => RequestPurpose::GetRecentStickers,
                StickerTab::Favorites => RequestPurpose::GetFavoriteStickers,
                StickerTab::Trending => RequestPurpose::GetTrendingStickerSets,
                StickerTab::Archived => RequestPurpose::GetArchivedStickerSets,
                StickerTab::Search => RequestPurpose::SearchStickerSets,
            };
            session.requests.has_purpose(purpose)
                || panel.tab == StickerTab::Search
                    && session.requests.has_purpose(RequestPurpose::SearchStickers)
        });
        // Only states worth reading: loading, failure, and empty views.
        let status: Option<String> = if loading_tab || panel.loading_set {
            Some(tr(keys::Q_MEDIA_LOADING_STICKERS).into())
        } else if panel.failed {
            Some(tr(keys::Q_MEDIA_COULDNT_UPDATE_STICKERS_TRY_AGAIN).into())
        } else {
            match panel.tab {
                StickerTab::Installed if panel.sets.is_empty() => {
                    Some(tr(keys::Q_MEDIA_NO_STICKER_SETS_YET).into())
                }
                StickerTab::Archived if panel.archived.is_empty() => {
                    Some(tr(keys::Q_MEDIA_NO_ARCHIVED_STICKER_SETS).into())
                }
                StickerTab::Recent if panel.recent.is_empty() => {
                    Some(tr(keys::Q_MEDIA_STICKERS_YOU_SEND_APPEAR_HERE).into())
                }
                StickerTab::Favorites if panel.favorites.is_empty() => {
                    Some(tr(keys::Q_MEDIA_RIGHT_CLICK_A_STICKER_TO).into())
                }
                StickerTab::Trending if panel.trending.is_empty() => {
                    Some(tr(keys::Q_MEDIA_NO_TRENDING_STICKER_SETS).into())
                }
                StickerTab::Search if panel.search_query.is_empty() => {
                    Some(tr(keys::Q_MEDIA_SEARCH_BY_EMOJI_TITLE_OR).into())
                }
                StickerTab::Search
                    if panel.found_stickers.is_empty() && visible_sets.is_empty() =>
                {
                    Some(tr(keys::Q_MEDIA_NO_STICKERS_FOUND).into())
                }
                _ => None,
            }
        };
        div()
            .id("sticker-picker")
            .max_h(px(420.))
            .overflow_y_scroll()
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
                    .gap_2()
                    .child(tabs)
                    .child(
                        div().flex().items_center().gap_1().child(more_menu).child(
                            Button::new("close-sticker-picker")
                                .icon(gpui_kit::assets::IconName::X)
                                .ghost()
                                .small()
                                .tooltip(tr(keys::Q_MEDIA_CLOSE_STICKERS))
                                .accessibility_label(tr(keys::Q_MEDIA_CLOSE_STICKERS))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_sticker_panel(cx);
                                })),
                        ),
                    ),
            )
            .when_some(status, |body, status| {
                body.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(status),
                )
            })
            .when(panel.batch_total > 0, |body| {
                body.child(div().text_sm().child(tr_with(
                    keys::Q_MEDIA_BATCH_OF_UPDATED_PENDING_FAILED,
                    &[
                        ("batch_completed", &panel.batch_completed.to_string()),
                        ("batch_total", &panel.batch_total.to_string()),
                        ("len", &(panel.batch_pending.len()).to_string()),
                        ("batch_failed", &panel.batch_failed.to_string()),
                    ],
                )))
            })
            .when(panel.tab == StickerTab::Search, |body| {
                body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().flex_1().min_w_0().child(
                                Textarea::new(&self.pickers.sticker_search_input)
                                    .aria_label(tr(keys::Q_MEDIA_SEARCH_STICKERS))
                                    .h(px(36.)),
                            ),
                        )
                        .child(
                            Button::new("sticker-search-submit")
                                .label(tr(keys::Q_MEDIA_SEARCH))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.search_sticker_picker(cx)),
                                ),
                        ),
                )
            })
            .child(sets)
            .child(preview)
            .child(
                div()
                    .id("sticker-scroll")
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .child(grid),
            )
    }
}

#[derive(Clone)]
struct StickerSetDrag {
    id: i64,
    title: String,
}

impl Render for StickerSetDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(cx.theme().accent.opacity(0.15))
            .text_sm()
            .child(self.title.clone())
    }
}

/// The greeting a chat shows: stable per chat (tdesktop picks at random
/// once per intro; keeping it by chat id avoids flicker on re-render).
fn greeting_pick(
    list: &[quill::telegram::envelope::StickerItem],
    chat_id: i64,
) -> Option<&quill::telegram::envelope::StickerItem> {
    if list.is_empty() {
        return None;
    }
    list.get(chat_id.unsigned_abs() as usize % list.len())
}

crate::ui::shell::register_dialogs! {
    ArchivedStickers => DialogSpec::new(
        5000,
        |app| app.settings.sticker_settings_open,
        QuillApp::build_archived_stickers_dialog,
    ),
}

#[cfg(test)]
mod greeting_tests {
    use super::greeting_pick;

    #[test]
    fn no_greeting_without_stickers() {
        assert!(greeting_pick(&[], 5).is_none());
    }
}
