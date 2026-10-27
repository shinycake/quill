//! Settings search: the field at the top of the sidebar and the ranked
//! results that take the page's place while there is a query. ↑↓ choose,
//! Enter opens, Esc clears. An empty, focused field lists recent picks
//! (tdesktop keeps them too).
#![deny(clippy::indexing_slicing)]

use super::{QuillApp, rows};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{english, keys, tr, tr_with};
use quill::settings_catalog::{self, Hit, Platforms, RecentSearches, SearchContext, Target};

#[derive(Default)]
pub(in crate::ui) struct SettingsSearch {
    /// Created with the window the settings window first renders in.
    input: Option<Entity<InputState>>,
    /// The field's text, mirrored on every edit.
    pub(super) query: String,
    /// The highlighted result (or recent pick).
    selected: usize,
    recent: Option<RecentSearches>,
    /// A result just opened: clear the field on the next render.
    pending_clear: bool,
}

/// What the results area lists.
enum Listing {
    Results(Vec<Hit>),
    Recent(Vec<Target>),
}

impl SettingsSearch {
    /// Settings closed: the next open starts with an empty field.
    pub(super) fn close(&mut self) {
        self.pending_clear = true;
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn recent_ids(&self) -> Vec<String> {
        self.recent
            .as_ref()
            .map(|r| r.as_slice().to_vec())
            .unwrap_or_default()
    }
}

impl QuillApp {
    fn search_recent(&mut self) -> &mut RecentSearches {
        self.settings.search.recent.get_or_insert_with(|| {
            quill::settings::load_settings_search_recent(&QuillApp::appearance_paths())
        })
    }

    pub(super) fn settings_search_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(input) = &self.settings.search.input {
            return input.clone();
        }
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_SETTINGS_SEARCH)));
        cx.subscribe_in(
            &input,
            window,
            |this, state, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.settings.search.query = state.read(cx).value().to_string();
                    this.settings.search.selected = 0;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.open_selected_search(cx),
                InputEvent::Focus | InputEvent::Blur => cx.notify(),
            },
        )
        .detach();
        self.settings.search.input = Some(input.clone());
        input
    }

    /// Whether the results area replaces the page.
    pub(super) fn settings_searching(&self, window: &Window, cx: &App) -> bool {
        !self.settings.search.query.trim().is_empty() || self.search_field_focused(window, cx)
    }

    pub(super) fn search_field_focused(&self, window: &Window, cx: &App) -> bool {
        self.settings
            .search
            .input
            .as_ref()
            .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    fn search_listing(&mut self) -> Listing {
        let query = self.settings.search.query.clone();
        let recent: Vec<String> = self.search_recent().as_slice().to_vec();
        if query.trim().is_empty() {
            return Listing::Recent(
                recent
                    .iter()
                    .filter_map(|id| settings_catalog::target(id))
                    .collect(),
            );
        }
        let ctx = SearchContext {
            text: &tr,
            english: &english,
            platform: Platforms::current(),
            recent: &recent,
        };
        Listing::Results(settings_catalog::search(&query, &ctx))
    }

    fn listing_len(&mut self) -> usize {
        match self.search_listing() {
            Listing::Results(hits) => hits.len(),
            Listing::Recent(targets) => targets.len(),
        }
    }

    /// ↑ / ↓ in the field.
    pub(super) fn move_search_selection(&mut self, down: bool, cx: &mut Context<Self>) {
        let len = self.listing_len();
        if len == 0 {
            return;
        }
        let selected = self.settings.search.selected.min(len - 1);
        self.settings.search.selected = if down {
            (selected + 1) % len
        } else {
            (selected + len - 1) % len
        };
        cx.notify();
    }

    fn open_selected_search(&mut self, cx: &mut Context<Self>) {
        let selected = self.settings.search.selected;
        let target = match self.search_listing() {
            Listing::Results(hits) => hits.get(selected).map(|hit| hit.target),
            Listing::Recent(targets) => targets.get(selected).copied(),
        };
        if let Some(target) = target {
            self.open_search_target(target, cx);
        }
    }

    /// Open what a result points at, remember it, and leave search.
    fn open_search_target(&mut self, target: Target, cx: &mut Context<Self>) {
        let recent = self.search_recent();
        recent.record(target.id());
        let recent = recent.clone();
        if let Err(err) =
            quill::settings::save_settings_search_recent(&QuillApp::appearance_paths(), &recent)
        {
            eprintln!("quill: could not save settings_search_recent.json: {err}");
        }
        self.settings.search.pending_clear = true;
        self.show_settings_page(target.page(), cx);
        if let Some(section) = super::agent_page::section_for_entry(target.id()) {
            self.agent_ui.page.scroll_to = Some(super::agent_page::Spot::Section(section));
        }
    }

    /// Esc in the field: clear it and give the page back.
    pub(super) fn clear_settings_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.settings.search.input.clone() {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.settings.search.query.clear();
        self.settings.search.selected = 0;
        window.focus(&self.settings.focus, cx);
        cx.notify();
    }

    /// ⌘F / Ctrl+F, or a letter typed anywhere in Settings.
    pub(super) fn focus_settings_search(
        &mut self,
        typed: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.settings_search_input(window, cx);
        if let Some(text) = typed {
            let value = format!("{}{text}", self.settings.search.query);
            input.update(cx, |input, cx| input.set_value(value.clone(), window, cx));
            self.settings.search.query = value;
            self.settings.search.selected = 0;
        }
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// Clears the field after a result opened (needs the window).
    pub(super) fn settle_settings_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.settings.search.pending_clear) {
            self.clear_settings_search(window, cx);
        }
    }

    /// The field, for the sidebar header (made by `settings_search_input`).
    pub(super) fn settings_search_field(&self, cx: &App) -> Option<AnyElement> {
        let input = self.settings.search.input.as_ref()?;
        Some(
            Input::new(input)
                .id("settings-search")
                .small()
                .cleanable(true)
                .aria_label(tr(keys::Q_SETTINGS_SEARCH))
                .prefix(
                    Icon::new(IconName::Search)
                        .size_4()
                        .text_color(cx.theme().muted_foreground),
                )
                .into_any_element(),
        )
    }

    /// Keys while Settings has focus: ⌘F / Ctrl+F and typing start a search;
    /// in the field ↑↓ choose and Esc clears. `true` when handled.
    pub(super) fn settings_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if window.has_active_dialog(cx)
            || self.settings.keybinding_capture.is_some()
            || self.topmost_esc_layer().is_some()
        {
            return false;
        }
        let keystroke = &event.keystroke;
        let mods = keystroke.modifiers;
        if keystroke.key == "f" && mods.secondary() && !mods.shift && !mods.alt {
            self.focus_settings_search(None, window, cx);
            return true;
        }
        if self.search_field_focused(window, cx) {
            return match keystroke.key.as_str() {
                "up" | "down" if !mods.modified() => {
                    self.move_search_selection(keystroke.key == "down", cx);
                    true
                }
                "escape" => {
                    self.clear_settings_search(window, cx);
                    true
                }
                _ => false,
            };
        }
        // Typing with nothing else focused starts a search.
        let typed = keystroke
            .key_char
            .as_deref()
            .filter(|t| t.chars().all(|c| !c.is_control()) && !t.trim().is_empty());
        if self.settings.focus.is_focused(window)
            && !mods.secondary()
            && !mods.control
            && !mods.alt
            && let Some(text) = typed
        {
            self.focus_settings_search(Some(text), window, cx);
            return true;
        }
        false
    }

    /// The results (or recent picks) in place of the page.
    pub(super) fn settings_search_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let listing = self.search_listing();
        let selected = self.settings.search.selected;
        let query = self.settings.search.query.trim().to_string();
        let (heading, rows): (Option<String>, Vec<AnyElement>) = match listing {
            Listing::Results(hits) => (
                hits.is_empty()
                    .then(|| tr_with(keys::Q_SETTINGS_SEARCH_NOTHING, &[("query", &query)])),
                hits.iter()
                    .enumerate()
                    .map(|(ix, hit)| {
                        self.search_row(ix, hit.target, &hit.highlight, ix == selected, false, cx)
                    })
                    .collect(),
            ),
            Listing::Recent(targets) => (
                Some(
                    tr(if targets.is_empty() {
                        keys::Q_SETTINGS_SEARCH_HINT
                    } else {
                        keys::Q_SETTINGS_SEARCH_RECENT
                    })
                    .to_string(),
                ),
                targets
                    .iter()
                    .enumerate()
                    .map(|(ix, target)| self.search_row(ix, *target, &[], ix == selected, true, cx))
                    .collect(),
            ),
        };
        div()
            .id("settings-search-results")
            .debug_selector(|| "settings-search-results".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .w_full()
                    .max_w(px(super::PAGE_WIDTH))
                    .mx_auto()
                    .px_6()
                    .py_6()
                    .when_some(heading, |this, heading| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(heading),
                        )
                    })
                    .when(!rows.is_empty(), |this| this.child(rows::group(None, rows))),
            )
            .into_any_element()
    }

    /// One result: the title with its matched letters bold, the path to it
    /// below; recent picks can be forgotten.
    fn search_row(
        &self,
        ix: usize,
        target: Target,
        highlight: &[usize],
        selected: bool,
        removable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = tr(target.title());
        let path = match target {
            Target::Page(_) => tr(keys::MENU_SETTINGS).to_string(),
            Target::Entry(entry) => {
                let page = tr(entry.page.title());
                match entry.section {
                    Some(section) => format!("{page} › {}", tr(section)),
                    None => page.to_string(),
                }
            }
        };
        let styled = StyledText::new(title).with_highlights(highlight_ranges(title, highlight));
        div()
            .flex()
            .items_center()
            .w_full()
            .rounded(cx.theme().radius)
            .when(selected, |this| this.bg(cx.theme().secondary))
            .child(
                Button::new(("settings-search-hit", ix))
                    .ghost()
                    .flex_1()
                    .h_auto()
                    .py_2()
                    .accessibility_label(title)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .w_full()
                            .child(div().text_sm().child(styled))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(path),
                            ),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.open_search_target(target, cx)),
                    ),
            )
            .when(removable, |this| {
                this.child(
                    Button::new(("settings-search-forget", ix))
                        .icon(IconName::X)
                        .ghost()
                        .small()
                        .accessibility_label(tr(keys::Q_SETTINGS_SEARCH_FORGET))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let recent = this.search_recent();
                            recent.remove(target.id());
                            let recent = recent.clone();
                            let _ = quill::settings::save_settings_search_recent(
                                &QuillApp::appearance_paths(),
                                &recent,
                            );
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }
}

/// Byte ranges of the highlighted chars of `text`, bold.
fn highlight_ranges(text: &str, chars: &[usize]) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let style = HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    };
    text.char_indices()
        .enumerate()
        .filter(|(ix, _)| chars.contains(ix))
        .map(|(_, (start, ch))| (start..start + ch.len_utf8(), style))
        .collect()
}
