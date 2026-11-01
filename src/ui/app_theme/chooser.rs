//! The theme chooser in Settings → Appearance (tdesktop
//! `settings_chat.cpp` `SetupDefaultThemes`, `ColorsPalette`,
//! `ChooseFromFile`): the four built-in themes as preview tiles, the
//! accent circles of the theme on screen with "System accent color",
//! custom themes with import, create, edit, export and delete, and Quill's
//! high-contrast palette.

use super::editor::EditorSource;
use super::{CustomTheme, QuillApp};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::StyledExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::color_picker::ColorSelect;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::app_theme::{BuiltinTheme, ThemeId, store};
use quill::lang::{keys, tr};
use quill::settings::ThemeChoice;
use quill::theme_colorize::Colorizer;

/// tdesktop `ColorsFromScheme`: the tile colours, recoloured like the
/// palette when the theme has an accent of its own.
fn builtin_preview(theme: BuiltinTheme, accent: Option<u32>) -> [u32; 3] {
    let colors = theme.preview();
    match accent {
        // Quill's accent only re-points the kit's primary colours.
        Some(_) if theme.is_quill() => colors,
        Some(accent) if accent != theme.accent() => {
            let colorizer = Colorizer::from_accent(theme.accent(), accent, theme.is_dark());
            colors.map(|c| colorizer.apply(c))
        }
        _ => colors,
    }
}

/// A theme tile: the chat background with an incoming and an outgoing
/// bubble (tdesktop `CloudListCheck`), and the name under it.
fn theme_tile(
    id: impl Into<ElementId>,
    name: SharedString,
    [background, outgoing, incoming]: [u32; 3],
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let bubble = |color: u32, width: f32| div().h(px(10.)).w(px(width)).rounded_md().bg(rgb(color));
    div()
        .id(id.into())
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .role(gpui_kit::Role::Button)
        .aria_label(format!(
            "{name}{}",
            if selected {
                tr(keys::Q_MISC_SELECTED)
            } else {
                ""
            }
        ))
        .tab_index(0)
        .cursor_pointer()
        .child(
            div()
                .w(px(88.))
                .h(px(60.))
                .p_2()
                .flex()
                .flex_col()
                .justify_end()
                .gap_1p5()
                .rounded_lg()
                .overflow_hidden()
                .bg(rgb(background))
                .border_2()
                .border_color(if selected {
                    theme.primary
                } else {
                    theme.border
                })
                .child(bubble(incoming, 52.))
                .child(div().flex().justify_end().child(bubble(outgoing, 44.))),
        )
        .child(
            div()
                .text_xs()
                .when(selected, |this| this.font_semibold())
                .child(name),
        )
}

impl QuillApp {
    fn applied_theme_id(&self) -> Option<&ThemeId> {
        self.settings.app_theme.applied.as_ref().map(|(id, _)| id)
    }

    /// Themes: the built-in tiles, the accent of the one on screen, custom
    /// themes and high contrast.
    pub(in crate::ui) fn app_themes_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let applied = self.applied_theme_id().cloned();
        let mut tiles = div().flex().flex_wrap().gap_3();
        // On Omarchy its current theme comes first.
        if let Some(palette) = self.omarchy_palette() {
            let name = self
                .omarchy_theme()
                .map(|state| state.theme.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| tr(keys::Q_SETTINGS_OMARCHY).to_string());
            tiles = tiles.child(
                theme_tile(
                    "app-theme-omarchy",
                    name.into(),
                    super::preview_colors(&palette),
                    applied == Some(ThemeId::Omarchy),
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| this.follow_omarchy(cx))),
            );
        }
        for theme in BuiltinTheme::ALL {
            let selected = applied == Some(ThemeId::Builtin(theme));
            let accent = self.theme_accent(theme);
            tiles = tiles.child(
                theme_tile(
                    format!("app-theme-{}", theme.id()),
                    theme.name().into(),
                    builtin_preview(theme, accent),
                    selected,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.choose_app_theme(ThemeId::Builtin(theme), theme.is_dark(), cx)
                })),
            );
        }
        let hint = if self.appearance.theme == ThemeChoice::HighContrast {
            tr(keys::Q_MISC_HIGH_CONTRAST_IS_ON_IT)
        } else if applied == Some(ThemeId::Omarchy) {
            tr(keys::Q_SETTINGS_OMARCHY_FOLLOWING)
        } else {
            tr(keys::Q_MISC_LIGHT_THEMES_SHOW_WHILE_NIGHT)
        };
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.appearance_section(
                cx,
                tr(keys::Q_MISC_THEMES),
                hint,
                tiles.into_any_element(),
            ));
        if self.applied_builtin_theme().is_some() {
            body = body.child(self.appearance_accent_section(cx));
        }
        body = body.child(self.custom_themes_section(cx));
        body = body.child(
            self.appearance_section(
                cx,
                tr(keys::Q_MISC_HIGH_CONTRAST),
                tr(keys::Q_MISC_BLACK_SURFACES_AND_WHITE_TEXT),
                Switch::new("appearance-high-contrast")
                    .checked(self.appearance.theme == ThemeChoice::HighContrast)
                    .accessibility_label(tr(keys::Q_MISC_HIGH_CONTRAST))
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_appearance(cx, |a| {
                            a.theme = if on {
                                ThemeChoice::HighContrast
                            } else {
                                ThemeChoice::Light
                            }
                        })
                    }))
                    .into_any_element(),
            ),
        );
        body.into_any_element()
    }

    /// tdesktop `ColorsPalette`: the theme's own accent, its eight others
    /// and a custom one; "System accent color" where the OS has one.
    pub(crate) fn appearance_accent_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(theme) = self.applied_builtin_theme() else {
            return div().into_any_element();
        };
        let current = self.theme_accent(theme).unwrap_or(theme.accent());
        let system = self
            .settings
            .system_accent
            .filter(|_| self.appearance.system_accent);
        let mut choices = vec![theme.accent()];
        choices.extend_from_slice(theme.accent_choices());
        let ring = cx.theme().foreground;
        let mut row = div().flex().flex_wrap().gap_2().items_center();
        for color in choices {
            let selected = system.is_none() && current == color;
            row = row.child(
                div()
                    .id(SharedString::from(format!("app-accent-{color:06x}")))
                    .size(px(28.))
                    .p(px(3.))
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        ring
                    } else {
                        Hsla::from(rgba(0x00000000))
                    })
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!(
                        "Accent #{color:06x}{}",
                        if selected {
                            tr(keys::Q_MISC_SELECTED)
                        } else {
                            ""
                        }
                    ))
                    .tab_index(0)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.set_theme_accent(theme, color, cx)),
                    )
                    .child(div().size_full().rounded_full().bg(rgb(color))),
            );
        }
        row = row.child(
            ColorSelect::new(&self.settings.accent_picker)
                .featured_colors(
                    theme
                        .accent_choices()
                        .iter()
                        .map(|&c| Hsla::from(rgb(c)))
                        .collect(),
                )
                .placeholder(tr(keys::Q_MISC_CUSTOM))
                .accessibility_label(tr(keys::Q_MISC_CUSTOM_ACCENT_COLOR))
                .w(px(150.)),
        );
        let mut body = div().flex().flex_col().gap_2().child(row);
        if self.settings.system_accent.is_some() {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new("app-accent-system")
                            .checked(system.is_some())
                            .accessibility_label(tr(keys::SETTINGS_THEME_SYSTEM_ACCENT_COLOR))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_appearance(cx, |a| a.system_accent = on)
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .child(tr(keys::SETTINGS_THEME_SYSTEM_ACCENT_COLOR)),
                    ),
            );
        }
        let hint = if theme.is_quill() {
            tr(keys::Q_MISC_HIGHLIGHTS_SELECTIONS_AND_LINKS_ACROSS)
        } else {
            tr(keys::Q_MISC_RECOLORS_THE_THEME_AROUND_THE)
        };
        self.appearance_section(
            cx,
            tr(keys::Q_MISC_ACCENT_COLOR),
            hint,
            body.into_any_element(),
        )
    }

    /// Custom themes: tiles to apply one, and import / create / edit /
    /// export / delete. tdesktop's "Choose from file" and "Create new
    /// theme"; its cloud themes need an API TDLib does not have.
    fn custom_themes_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let themes: Vec<CustomTheme> = self.custom_themes(cx).to_vec();
        let applied = self.applied_theme_id().cloned();
        let mut tiles = div().flex().flex_wrap().gap_3();
        let mut selected_name = None;
        for theme in &themes {
            let id = ThemeId::Custom(theme.name.clone());
            let selected = applied.as_ref() == Some(&id);
            if selected {
                selected_name = Some(theme.name.clone());
            }
            let dark = theme.dark;
            tiles = tiles.child(
                theme_tile(
                    SharedString::from(format!("app-theme-custom-{}", theme.name)),
                    theme.name.clone().into(),
                    theme.preview,
                    selected,
                    cx,
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.choose_app_theme(id.clone(), dark, cx)),
                ),
            );
        }
        let mut actions = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                Button::new("app-theme-import")
                    .label(tr(keys::Q_MISC_IMPORT_THEME_FILE))
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| this.import_theme_file(cx))),
            )
            .child(
                Button::new("app-theme-create")
                    .label(tr(keys::SETTINGS_BG_THEME_CREATE))
                    .outline()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_theme_editor(EditorSource::Current, window, cx)
                    })),
            );
        if let Some(name) = selected_name {
            let (edit, export, delete) = (name.clone(), name.clone(), name);
            actions = actions
                .child(
                    Button::new("app-theme-edit")
                        .label(tr(keys::THEME_EDIT))
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_theme_editor(EditorSource::Custom(edit.clone()), window, cx)
                        })),
                )
                .child(
                    Button::new("app-theme-export")
                        .label(tr(keys::Q_MISC_EXPORT))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            match store::load(&Self::themes_dir(), &export) {
                                Ok(file) => this.export_theme_file(file, &export, cx),
                                Err(err) => {
                                    this.connection.set_note(StatusNote::error(err.note()));
                                    cx.notify();
                                }
                            }
                        })),
                )
                .child(
                    Button::new("app-theme-delete")
                        .label(tr(keys::THEME_DELETE))
                        .ghost()
                        .danger()
                        .on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.delete_custom_theme(&delete, cx)
                            }),
                        ),
                );
        }
        let control = div()
            .flex()
            .flex_col()
            .gap_3()
            .when(!themes.is_empty(), |this| this.child(tiles))
            .child(actions);
        self.appearance_section(
            cx,
            tr(keys::Q_MISC_CUSTOM_THEMES),
            tr(keys::Q_MISC_THEME_FILES_IN_THE_TDESKTOP),
            control.into_any_element(),
        )
    }

    /// tdesktop "Choose from file" for themes: the picked file is copied
    /// into the themes folder and applied.
    fn import_theme_file(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::THEME_EDITOR_MENU_IMPORT).into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            // The copy and the decode of the theme's picture run off the
            // UI thread.
            let imported = cx
                .background_spawn(async move {
                    let dir = Self::themes_dir();
                    let taken = store::list(&dir);
                    store::import(&dir, &path, &taken)
                        .map(|(name, file)| (name, super::file_palette(&file)))
                })
                .await;
            let _ = this.update(cx, |this, cx| match imported {
                Ok((name, palette)) => {
                    this.refresh_custom_themes();
                    let night = palette.is_dark();
                    this.settings.app_theme.custom_cache = Some((name.clone(), Some(palette)));
                    this.choose_app_theme(ThemeId::Custom(name), night, cx);
                }
                Err(err) => {
                    this.connection.set_note(StatusNote::error(err.note()));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Delete a custom theme; a day or night slot holding it goes back to
    /// the built-in default.
    fn delete_custom_theme(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Err(err) = store::delete(&Self::themes_dir(), name) {
            self.connection.set_note(StatusNote::error(err.note()));
        }
        self.refresh_custom_themes();
        let gone = ThemeId::Custom(name.to_string());
        self.set_appearance(cx, |a| {
            if a.day_theme == gone {
                a.day_theme = ThemeId::default_day();
            }
            if a.night_theme == gone {
                a.night_theme = ThemeId::default_night();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::builtin_preview;
    use quill::app_theme::BuiltinTheme;

    #[test]
    fn tiles_follow_the_accent() {
        let theme = BuiltinTheme::Day;
        assert_eq!(builtin_preview(theme, None), theme.preview());
        assert_eq!(
            builtin_preview(theme, Some(theme.accent())),
            theme.preview()
        );
        let pink = builtin_preview(theme, Some(0xd46c99));
        assert_ne!(
            pink[0],
            theme.preview()[0],
            "the blue background turns pink"
        );
        assert_eq!(pink[2], 0xffffff, "white stays");
    }
}
