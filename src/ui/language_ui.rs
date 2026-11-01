//! Settings → Language (tdesktop `boxes/language_box.cpp`): the translate
//! options, a search field, the packs used before and the official packs.
//! Picking a pack switches the interface at once; the strings come from
//! TDLib (docs/decisions/language-packs.md).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::language_state::LanguageBox;
use super::language_state::LanguageFile;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::prefs::{self, LanguageChoice};
use quill::lang::{keys, tr};
use quill::state::LanguageReady;
use quill::telegram::envelope::LanguagePackInfoData;

/// One row of the box.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct LanguageRow {
    pub(super) info: LanguagePackInfoData,
    /// A pack from the recent list (shareable, removable).
    pub(super) recent: bool,
}

/// The rows of the box: recent packs that are not official first, then
/// the official list, both filtered by `query` (tdesktop `PrepareLists`).
pub(super) fn language_rows(
    official: &[LanguagePackInfoData],
    recent: &[LanguageChoice],
    current: &str,
    query: &str,
) -> (Vec<LanguageRow>, Vec<LanguageRow>) {
    let mut recent: Vec<LanguageRow> = recent
        .iter()
        .filter(|choice| !official.iter().any(|o| o.id == choice.pack_id))
        .map(|choice| LanguageRow {
            info: quill_info_of(choice),
            recent: true,
        })
        .collect();
    // The current pack goes first.
    recent.sort_by_key(|row| row.info.id != current);
    let official = official
        .iter()
        .filter(|info| info.is_official)
        .map(|info| LanguageRow {
            info: info.clone(),
            recent: false,
        });
    let keep = |row: &LanguageRow| {
        quill::lang::search::matches(query, &[&row.info.name, &row.info.native_name])
    };
    (
        recent.into_iter().filter(keep).collect(),
        official.filter(keep).collect(),
    )
}

fn quill_info_of(choice: &LanguageChoice) -> LanguagePackInfoData {
    LanguagePackInfoData {
        id: choice.pack_id.clone(),
        base_language_pack_id: choice.base_pack_id.clone(),
        name: choice.name.clone(),
        native_name: choice.native_name.clone(),
        plural_code: choice.plural_code.clone(),
        is_official: false,
        is_rtl: choice.is_rtl,
        is_beta: false,
        is_installed: true,
        total_string_count: 0,
        translated_string_count: 0,
    }
}

fn choice_of(info: &LanguagePackInfoData) -> LanguageChoice {
    LanguageChoice {
        pack_id: info.id.clone(),
        base_pack_id: info.base_language_pack_id.clone(),
        plural_code: info.plural_code.clone(),
        is_rtl: info.is_rtl,
        name: info.name.clone(),
        native_name: info.native_name.clone(),
    }
}

/// The id of the pack in use (`"en"` for the built-in English).
fn current_id() -> String {
    quill::lang::current_pack_id()
}

impl QuillApp {
    /// Settings → Language.
    pub(super) fn open_language_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(Some(quill::settings_catalog::Page::Language));
        self.ensure_language_box(window, cx);
    }

    /// The page's search field and pack request, made once per visit.
    pub(crate) fn ensure_language_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.language.open.is_some() {
            return;
        }
        let search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::PARTICIPANT_FILTER))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let subscription = cx.subscribe_in(
            &search,
            window,
            |_this, _state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        );
        self.language.open = Some(LanguageBox {
            search,
            _search_subscription: subscription,
            removed: Vec::new(),
        });
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.request_language_packs();
        }
        cx.notify();
    }

    /// Leaving the page: drop the search field and forget the recent
    /// languages removed while it was open.
    pub(crate) fn leave_language_settings(&mut self) {
        if let Some(open) = self.language.open.take() {
            self.language
                .recent
                .retain(|choice| !open.removed.contains(&choice.pack_id));
        }
    }

    /// The official packs: TDLib's list, or the demo fixture.
    fn official_language_packs(&self) -> Vec<LanguagePackInfoData> {
        match self.live.as_ref() {
            Some(live) => live.driver.session.settings.language.packs.clone(),
            None => super::language_demo::demo_packs(),
        }
    }

    /// A row was picked: switch to its pack. Nothing happens when it is
    /// already the target: the pack in use with no switch in flight, or the
    /// pack being fetched. Picking the pack in use during a switch cancels
    /// that switch.
    pub(super) fn choose_language(&mut self, info: LanguagePackInfoData, cx: &mut Context<Self>) {
        let in_flight = self.live.as_ref().and_then(|live| {
            live.driver
                .session
                .settings
                .language
                .switching
                .as_ref()
                .map(|switch| switch.info.id.clone())
        });
        let target = in_flight.unwrap_or_else(current_id);
        if info.id == target {
            return;
        }
        match self.live.as_mut() {
            Some(live) => {
                if let Err(err) = live.driver.switch_language(info) {
                    self.connection.set_note(StatusNote::error(user_error_for(
                        keys::Q_NOTES_FAILED_SWITCH_LANGUAGE,
                        &err,
                    )));
                }
            }
            None => {
                let ready = super::language_demo::demo_ready(info);
                self.install_language(ready, cx);
            }
        }
        cx.notify();
    }

    /// Takes what the language flow produced: strings to install, changed
    /// strings, a reload request or an error.
    pub(super) fn drain_language(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(state) = self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session.settings.language)
        else {
            return false;
        };
        let ready = state.ready.take();
        let patch = std::mem::take(&mut state.patch);
        let reload = std::mem::replace(&mut state.reload, false);
        let error = state.error.take();
        let current = state.current.clone();
        let progressed = ready.is_some() || !patch.is_empty() || reload || error.is_some();
        if let Some(ready) = ready {
            self.install_language(ready, cx);
        }
        if !patch.is_empty()
            && let Some(snapshot) = quill::lang::current()
            && current
                .as_ref()
                .is_some_and(|info| info.id == snapshot.pack_id())
        {
            let next = snapshot.patched(patch.iter().map(|(k, v)| (k.as_str(), v)));
            self.save_language_cache(&next, cx);
            quill::lang::install(Some(next));
            cx.refresh_windows();
        }
        // A switch the person started wins over a reload of the old pack:
        // its strings are newer anyway.
        if reload
            && let (Some(info), Some(live)) = (current, self.live.as_mut())
            && live.driver.session.settings.language.switching.is_none()
        {
            let _ = live.driver.switch_language(info);
        }
        if let Some(error) = error {
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.restore_language_option();
            }
            self.connection.set_note(StatusNote::error(error));
        }
        progressed
    }

    /// Makes `ready` the interface's strings, remembers the choice and
    /// redraws every window.
    pub(super) fn install_language(&mut self, ready: LanguageReady, cx: &mut Context<Self>) {
        let choice = choice_of(&ready.info);
        let snapshot = (!choice.is_builtin())
            .then(|| prefs::build_snapshot(&choice, &ready.base, &ready.pack));
        if let Some(snapshot) = snapshot.as_ref() {
            self.save_language_cache(snapshot, cx);
        }
        if !ready.info.is_official && !choice.is_builtin() {
            self.language.recent = prefs::with_recent(&self.language.recent, &choice);
        }
        self.sync_language_tag(&choice.pack_id);
        quill::lang::install(snapshot);
        if let Some(root) = self.language_root() {
            let recent = self.language.recent.clone();
            let choice = if choice.is_builtin() {
                LanguageChoice::default()
            } else {
                choice
            };
            let recent_root = root.clone();
            self.language
                .writes
                .spawn(LanguageFile::Choice, cx, move || {
                    let _ = prefs::save_choice(&root, &choice);
                });
            self.language
                .writes
                .spawn(LanguageFile::Recent, cx, move || {
                    let _ = prefs::save_recent(&recent_root, &recent);
                });
        }
        cx.refresh_windows();
    }

    /// Keeps the language tag TDLib reports to Telegram
    /// (`system_language_code`, read at the next start) in step with the
    /// interface language when Quill knows that tag.
    fn sync_language_tag(&mut self, pack_id: &str) {
        let primary = pack_id.split('-').next().unwrap_or_default();
        let code = if primary.is_empty() { "en" } else { primary };
        if !quill::settings::SUPPORTED_LANGUAGES
            .iter()
            .any(|(known, _)| *known == code)
        {
            return;
        }
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .session
                .settings
                .language_prefs
                .system_language_code
                != code
        {
            live.driver
                .session
                .settings
                .language_prefs
                .system_language_code = code.to_string();
            // Best effort: the tag only matters from the next start.
            live.driver.save_language_prefs();
        }
    }

    fn save_language_cache(&mut self, snapshot: &quill::lang::Snapshot, cx: &mut Context<Self>) {
        let Some(root) = self.language_root() else {
            return;
        };
        let entries = snapshot.entries();
        let pack_id = snapshot.pack_id().to_string();
        self.language
            .writes
            .spawn(LanguageFile::Cache, cx, move || {
                let _ = prefs::save_cache_entries(&root, &pack_id, entries);
            });
    }

    /// Where the choice is saved; demos never write it.
    fn language_root(&self) -> Option<std::path::PathBuf> {
        self.live.as_ref()?;
        quill::settings::safe_app_root()
    }

    fn remove_recent_language(&mut self, id: &str, restore: bool, cx: &mut Context<Self>) {
        let Some(open) = self.language.open.as_mut() else {
            return;
        };
        if restore {
            open.removed.retain(|r| r != id);
        } else if !open.removed.iter().any(|r| r == id) {
            open.removed.push(id.to_string());
        }
        let removed = open.removed.clone();
        let kept: Vec<LanguageChoice> = self
            .language
            .recent
            .iter()
            .filter(|c| !removed.contains(&c.pack_id))
            .cloned()
            .collect();
        if let Some(root) = self.language_root() {
            self.language
                .writes
                .spawn(LanguageFile::Recent, cx, move || {
                    let _ = prefs::save_recent(&root, &kept);
                });
        }
        cx.notify();
    }

    fn language_row(&self, row: LanguageRow, current: &str, cx: &mut Context<Self>) -> AnyElement {
        let id = row.info.id.clone();
        let checked = id == current || (current == "en" && id.is_empty());
        let removed = self
            .language
            .open
            .as_ref()
            .is_some_and(|open| open.removed.contains(&id));
        let theme = cx.theme();
        let (hover, muted) = (theme.accent, theme.muted_foreground);
        let name = if row.info.name.is_empty() {
            row.info.id.clone()
        } else {
            row.info.name.clone()
        };
        let pick = row.info.clone();
        let mut line = div()
            .id(SharedString::from(format!("language-row-{id}")))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_1p5()
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(removed, |row| row.opacity(0.5))
            .on_click(cx.listener(move |this, _, _, cx| this.choose_language(pick.clone(), cx)))
            .child(
                Radio::new(SharedString::from(format!("language-radio-{id}")))
                    .checked(checked)
                    .accessibility_label(name.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child(name))
                    .when(!row.info.native_name.is_empty(), |col| {
                        col.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(row.info.native_name.clone()),
                        )
                    }),
            );
        // TDLib's local packs (ids starting with `X`) have no link.
        let local = id.starts_with('X');
        if row.recent && !(checked && local) {
            let can_share = !local;
            let can_remove = !checked;
            let owner = cx.entity().downgrade();
            line = line.child(
                Button::new(SharedString::from(format!("language-menu-{id}")))
                    .icon(gpui_kit::assets::IconName::EllipsisVertical)
                    .ghost()
                    .small()
                    .dropdown_menu(move |mut menu, _, _| {
                        if can_share {
                            let owner = owner.clone();
                            let link = format!("https://t.me/setlanguage/{id}");
                            menu = menu.item(
                                PopupMenuItem::new(tr(keys::PROXY_EDIT_SHARE))
                                    .icon(gpui_kit::assets::IconName::Share2)
                                    .on_click(move |_, _, cx| {
                                        let _ = owner.update(cx, |this, cx| {
                                            this.copy_profile_text(
                                                &link,
                                                tr(keys::USERNAME_COPIED),
                                                cx,
                                            )
                                        });
                                    }),
                            );
                        }
                        if can_remove {
                            let owner = owner.clone();
                            let id = id.clone();
                            let (label, icon) = if removed {
                                (
                                    tr(keys::PROXY_MENU_RESTORE),
                                    gpui_kit::assets::IconName::Undo2,
                                )
                            } else {
                                (
                                    tr(keys::PROXY_MENU_DELETE),
                                    gpui_kit::assets::IconName::Trash,
                                )
                            };
                            menu = menu.item(PopupMenuItem::new(label).icon(icon).on_click(
                                move |_, _, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        this.remove_recent_language(&id, removed, cx)
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            );
        }
        line.into_any_element()
    }

    pub(crate) fn language_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(search) = self.language.open.as_ref().map(|open| open.search.clone()) else {
            return div().into_any_element();
        };
        let query = search.read(cx).value().to_string();
        let current = current_id();
        let official = self.official_language_packs();
        let (recent, official) = language_rows(&official, &self.language.recent, &current, &query);
        let loading = official.is_empty()
            && self
                .live
                .as_ref()
                .is_some_and(|l| l.driver.session.settings.language.packs_loading);
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let empty = recent.is_empty() && official.is_empty() && !loading;
        let recent_rows: Vec<AnyElement> = recent
            .into_iter()
            .map(|row| self.language_row(row, &current, cx))
            .collect();
        let has_recent = !recent_rows.is_empty();
        let official_rows: Vec<AnyElement> = official
            .into_iter()
            .map(|row| self.language_row(row, &current, cx))
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.translate_settings_section(cx))
            .child(div().h(px(1.)).bg(border))
            .child(
                Textarea::new(&search)
                    .aria_label(tr(keys::PARTICIPANT_FILTER))
                    .h(px(40.)),
            )
            .child(
                div()
                    .id("language-rows")
                    .flex()
                    .flex_col()
                    .children(recent_rows)
                    .when(has_recent, |col| {
                        col.child(div().my_1().h(px(1.)).bg(border))
                    })
                    .children(official_rows)
                    .when(loading, |col| {
                        col.child(gpui_kit::component::spinner::Spinner::new().small())
                    })
                    .when(empty, |col| {
                        col.child(
                            div()
                                .px_3()
                                .py_2()
                                .text_sm()
                                .text_color(muted)
                                .child(tr(keys::LANGUAGES_NONE)),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "language_ui_tests.rs"]
mod tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "language_box_tests.rs"]
mod box_tests;
