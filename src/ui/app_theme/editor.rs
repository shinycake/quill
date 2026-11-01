//! The palette editor (tdesktop `window_theme_editor.cpp`, "Edit color
//! palette"): every key Quill paints with, searchable, each with a colour
//! picker. Changes show live across the app. "Save theme" stores it as a
//! custom theme and applies it; "Export" writes a `.tdesktop-theme` file
//! anywhere (tdesktop's editor menu "Export theme"). tdesktop's "Save
//! theme" uploads a cloud theme, which TDLib cannot do.

use super::super::{DialogKind, QuillShell};
use super::{QuillApp, image_color};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::StyledExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::app_theme::{
    KeyGroup, Palette, PaletteKey, ThemeFile, ThemeId, custom_palette, hex_string, store,
};
use quill::lang::{keys, tr, tr_with};
use std::cell::RefCell;
use std::rc::Rc;

/// A row of the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorRow {
    Key(PaletteKey),
    /// The theme's chat background (tdesktop's "Background Image" row; here
    /// one colour).
    Background,
}

pub(crate) struct ThemeEditor {
    name: Entity<InputState>,
    search: Entity<InputState>,
    picker: Entity<ColorPickerState>,
    /// The theme as it will be saved, with keys Quill does not use kept.
    file: ThemeFile,
    /// The colour standing for the theme's background, worked out once
    /// (decoding its picture) so a colour change does not decode it again.
    image: Option<u32>,
    pub(super) palette: Palette,
    selected: EditorRow,
    /// The custom theme being edited; saving under the same name replaces
    /// it.
    editing: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl ThemeEditor {
    /// What the chooser shows as applied while the editor previews.
    pub(super) fn preview_id(&self) -> ThemeId {
        ThemeId::Custom(self.editing.clone().unwrap_or_default())
    }

    fn color(&self, row: EditorRow) -> u32 {
        match row {
            EditorRow::Key(key) => self.palette.rgba(key),
            EditorRow::Background => {
                (self.palette.background.as_ref().map_or(0, |b| b.average()) << 8) | 0xff
            }
        }
    }
}

/// The rows matching `query` (label or key name, ignoring case), by
/// group; the background row leads the window group.
pub(crate) fn rows_matching(query: &str) -> Vec<(KeyGroup, Vec<EditorRow>)> {
    let query = query.trim().to_lowercase();
    let matches = |text: &str| query.is_empty() || text.to_lowercase().contains(&query);
    KeyGroup::ALL
        .iter()
        .filter_map(|&group| {
            let mut rows = Vec::new();
            if group == KeyGroup::Window
                && (matches(tr(keys::Q_MISC_CHAT_BACKGROUND)) || matches("background"))
            {
                rows.push(EditorRow::Background);
            }
            rows.extend(
                PaletteKey::ALL
                    .iter()
                    .filter(|k| k.group() == group && (matches(k.label()) || matches(k.name())))
                    .map(|&k| EditorRow::Key(k)),
            );
            (!rows.is_empty()).then_some((group, rows))
        })
        .collect()
}

fn hsla_of(rgba: u32) -> Hsla {
    super::kit_colors::hsla(rgba)
}

fn rgba_of(color: Hsla) -> u32 {
    let c = Rgba::from(color);
    let channel = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    (channel(c.r) << 24) | (channel(c.g) << 16) | (channel(c.b) << 8) | channel(c.a)
}

/// Where the editor starts from.
pub(crate) enum EditorSource {
    /// A new theme from the colours on screen (tdesktop "Create new
    /// theme": "based on your currently selected colors and wallpaper").
    Current,
    /// An existing custom theme.
    Custom(String),
}

impl QuillApp {
    /// Open the palette editor.
    pub(crate) fn open_theme_editor(
        &mut self,
        source: EditorSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match source {
            EditorSource::Current => {
                let palette = self
                    .settings
                    .app_theme
                    .applied
                    .as_ref()
                    .map(|(_, p)| p.clone())
                    .unwrap_or_else(|| quill::app_theme::BuiltinTheme::Classic.palette());
                let file = ThemeFile::from_palette(&palette);
                // A solid background: no picture to decode.
                let image = image_color(&file);
                self.show_theme_editor(file, None, image, window, cx);
            }
            // The theme's picture is decoded off the UI thread.
            EditorSource::Custom(name) => {
                let dir = Self::themes_dir();
                let read = cx.background_spawn(async move {
                    store::load(&dir, &name).map(|file| {
                        let image = image_color(&file);
                        (file, name, image)
                    })
                });
                cx.spawn_in(window, async move |this, cx| {
                    let read = read.await;
                    let _ = this.update_in(cx, |this, window, cx| match read {
                        Ok((file, name, image)) => {
                            this.show_theme_editor(file, Some(name), image, window, cx);
                        }
                        Err(err) => {
                            this.connection.set_note(StatusNote::error(err.note()));
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }
    }

    fn show_theme_editor(
        &mut self,
        file: ThemeFile,
        editing: Option<String>,
        image: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let palette = custom_palette(&file, image);
        let taken = self.custom_theme_names();
        let initial_name = editing
            .clone()
            .unwrap_or_else(|| store::unique_name(tr(keys::Q_MISC_MY_THEME), &taken));
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::THEME_EDITOR_NAME))
                .default_value(initial_name)
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_MISC_SEARCH)));
        let selected = EditorRow::Key(PaletteKey::WindowBgActive);
        let picker = cx.new(|cx| {
            ColorPickerState::new(window, cx)
                .default_value(hsla_of(palette.rgba(PaletteKey::WindowBgActive)))
        });
        let subscriptions = vec![
            cx.subscribe_in(&search, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(
                &picker,
                window,
                |this, _, event: &ColorPickerEvent, _, cx| {
                    if let ColorPickerEvent::Change(Some(color)) = event {
                        this.editor_color_changed(*color, cx);
                    }
                },
            ),
        ];
        self.settings.app_theme.editor = Some(ThemeEditor {
            name,
            search,
            picker,
            file,
            image,
            palette,
            selected,
            editing,
            _subscriptions: subscriptions,
        });
        self.apply_appearance(cx);
        cx.notify();
    }

    /// Close the editor; what it changed goes away unless it was saved.
    pub(crate) fn close_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.settings.app_theme.editor = None;
        self.apply_appearance(cx);
        cx.notify();
    }

    pub(super) fn select_editor_row(
        &mut self,
        row: EditorRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.settings.app_theme.editor.as_mut() else {
            return;
        };
        editor.selected = row;
        let color = hsla_of(editor.color(row));
        editor
            .picker
            .update(cx, |picker, cx| picker.set_value(color, window, cx));
        cx.notify();
    }

    /// The picker committed `color` for the selected row: show it live.
    pub(super) fn editor_color_changed(&mut self, color: Hsla, cx: &mut Context<Self>) {
        let Some(editor) = self.settings.app_theme.editor.as_mut() else {
            return;
        };
        let rgba = rgba_of(color);
        match editor.selected {
            EditorRow::Key(key) => editor.file.set_color(key, rgba),
            EditorRow::Background => {
                editor.file.set_background(rgba >> 8);
                editor.image = image_color(&editor.file);
            }
        }
        editor.palette = custom_palette(&editor.file, editor.image);
        self.apply_appearance(cx);
        cx.notify();
    }

    /// Store the edited theme under its name and apply it.
    fn save_theme_editor(&mut self, cx: &mut Context<Self>) -> bool {
        let names = self.custom_theme_names();
        let Some(editor) = self.settings.app_theme.editor.as_ref() else {
            return false;
        };
        let typed = store::clean_name(editor.name.read(cx).value().as_ref());
        // Saving over another theme would lose it: a taken name gets a
        // number, unless it is the theme being edited.
        let taken: Vec<String> = names
            .into_iter()
            .filter(|n| Some(n) != editor.editing.as_ref())
            .collect();
        let name = store::unique_name(&typed, &taken);
        let file = editor.file.clone();
        let night = editor.palette.is_dark();
        let replaced = editor.editing.clone().filter(|old| *old != name);
        let dir = Self::themes_dir();
        match store::save(&dir, &name, &file) {
            Ok(_) => {
                // Renamed while editing: the old file goes.
                if let Some(old) = replaced {
                    let _ = store::delete(&dir, &old);
                }
                self.settings.app_theme.editor = None;
                self.refresh_custom_themes();
                self.choose_app_theme(ThemeId::Custom(name), night, cx);
                true
            }
            Err(err) => {
                self.connection.set_note(StatusNote::error(err.note()));
                cx.notify();
                false
            }
        }
    }

    /// tdesktop "Export theme": write `file` as a `.tdesktop-theme` where
    /// the user picks.
    pub(crate) fn export_theme_file(
        &mut self,
        file: ThemeFile,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        let dir =
            quill::media_viewer::downloads_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let suggested = format!("{}.{}", store::clean_name(name), store::EXTENSION);
        let picker = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dest))) = picker.await else {
                return;
            };
            let written = cx
                .background_executor()
                .spawn(async move { std::fs::write(&dest, file.to_zip()) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.connection.set_note(match written {
                    Ok(()) => StatusNote::info(tr(keys::Q_MISC_THEME_EXPORTED)),
                    Err(err) => StatusNote::error(tr_with(
                        keys::Q_MISC_COULDNT_EXPORT_THE_THEME,
                        &[("err", &err.to_string())],
                    )),
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn theme_editor_row(
        &self,
        editor: &ThemeEditor,
        row: EditorRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let selected = editor.selected == row;
        let rgba = editor.color(row);
        let (label, key) = match row {
            EditorRow::Key(key) => (key.label(), key.name()),
            EditorRow::Background => (tr(keys::Q_MISC_CHAT_BACKGROUND_2), "background"),
        };
        let id = match row {
            EditorRow::Key(key) => SharedString::from(format!("theme-key-{}", key.name())),
            EditorRow::Background => SharedString::from("theme-key-background"),
        };
        let swatch = if selected {
            ColorPicker::new(&editor.picker)
                .accessibility_label(format!("{label} color"))
                .into_any_element()
        } else {
            div()
                .size(px(20.))
                .rounded_sm()
                .border_1()
                .border_color(theme.border)
                .bg(hsla_of(rgba))
                .into_any_element()
        };
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .py_1()
            .rounded_md()
            .role(gpui_kit::Role::Button)
            .aria_label(format!("{label}, {}", hex_string(rgba)))
            .tab_index(0)
            .cursor_pointer()
            .when(selected, |this| this.bg(theme.list_active))
            .when(!selected, |this| this.hover(|s| s.bg(theme.list_hover)))
            .on_click(
                cx.listener(move |this, _, window, cx| this.select_editor_row(row, window, cx)),
            )
            .child(div().w(px(28.)).flex().justify_center().child(swatch))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(div().text_sm().child(label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(key),
                    ),
            )
            .child(
                div()
                    .ml_auto()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(hex_string(rgba)),
            )
            .into_any_element()
    }

    fn theme_editor_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.settings.app_theme.editor.as_ref() else {
            return div().into_any_element();
        };
        let query = editor.search.read(cx).value().to_string();
        let groups = rows_matching(&query);
        let muted = cx.theme().muted_foreground;
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(keys::Q_MISC_PICK_A_ROW_THEN_ITS)),
            )
            .child(Input::new(&editor.name).aria_label(tr(keys::THEME_EDITOR_NAME)))
            .child(Input::new(&editor.search).aria_label(tr(keys::Q_MISC_SEARCH_COLORS)));
        if groups.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_MISC_NO_COLORS_MATCH)),
            );
        }
        for (group, rows) in groups {
            let mut list = div().flex().flex_col().gap_0p5();
            for row in rows {
                list = list.child(self.theme_editor_row(editor, row, cx));
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().font_semibold().child(group.title()))
                    .child(list),
            );
        }
        body.into_any_element()
    }

    pub(crate) fn build_theme_editor_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ThemeEditor, |this, _, cx| {
                this.close_theme_editor(cx)
            });
        app.update(cx, |this, cx| {
            let body = this.theme_editor_body(cx);
            let footer = div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("theme-editor-export")
                        .label(tr(keys::Q_MISC_EXPORT))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let Some(editor) = this.settings.app_theme.editor.as_ref() else {
                                return;
                            };
                            let file = editor.file.clone();
                            let name = editor.name.read(cx).value().to_string();
                            this.export_theme_file(file, &name, cx);
                        })),
                )
                .child(div().flex_1())
                .child(
                    Button::new("theme-editor-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_theme_editor(cx);
                            this.close_kit_dialog_if_done(DialogKind::ThemeEditor, window, cx);
                        })),
                )
                .child(
                    Button::new("theme-editor-save")
                        .label(tr(keys::THEME_EDITOR_SAVE_TITLE))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.save_theme_editor(cx) {
                                this.close_kit_dialog_if_done(DialogKind::ThemeEditor, window, cx);
                            }
                        })),
                );
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                // No dim: the app behind shows the changes live.
                .overlay(false)
                .width(px(460.))
                .title(crate::ui::shell::dialog_title(tr(keys::THEME_EDITOR_TITLE)))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    ThemeEditor => DialogSpec::new(
        6590,
        |app| app.settings.app_theme.editor.is_some(),
        QuillApp::build_theme_editor_dialog,
    ),
}

#[cfg(test)]
mod tests {
    use super::{EditorRow, rgba_of, rows_matching};
    use quill::app_theme::{KeyGroup, PaletteKey};

    #[test]
    fn search_filters_rows_by_label_or_key() {
        let all: usize = rows_matching("").iter().map(|(_, r)| r.len()).sum();
        assert_eq!(all, PaletteKey::COUNT + 1, "every key plus the background");
        let bubble = rows_matching("bubble");
        assert_eq!(bubble.len(), 1);
        assert_eq!(bubble[0].0, KeyGroup::Messages);
        assert!(bubble[0].1.contains(&EditorRow::Key(PaletteKey::MsgOutBg)));
        let by_key = rows_matching("DIALOGSBG");
        assert_eq!(by_key[0].1, vec![EditorRow::Key(PaletteKey::DialogsBg)]);
        assert_eq!(rows_matching("background")[0].1[0], EditorRow::Background);
        assert!(rows_matching("zzz").is_empty());
    }

    #[test]
    fn picker_colours_keep_their_alpha() {
        let c = super::hsla_of(0x11223344);
        assert_eq!(rgba_of(c), 0x11223344);
    }
}
