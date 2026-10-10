//! The folder editor's "Included chats" / "Excluded chats" sections and the
//! chat picker with search (tdesktop `boxes/filters/edit_filter_box.cpp`
//! and `edit_filter_chats_list.cpp`).

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::folder_limits::FolderLimitKind;
use quill::folder_picker::{PickerMode, chats_count_text, filter_chats, may_tick, picker_counter};
use quill::local_path::sandboxed_display_path;

const AVATAR: f32 = 30.;

impl QuillApp {
    /// Every chat the picker can offer, A to Z.
    fn folder_picker_chats(&self) -> Vec<(i64, String)> {
        let mut chats: Vec<(i64, String)> = self
            .session()
            .map(|s| {
                s.chats
                    .values()
                    .map(|c| (c.id.0, c.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        chats.sort_by_key(|a| a.1.to_lowercase());
        chats
    }

    fn folder_chat_limit(&self, mode: PickerMode) -> i32 {
        let kind = match mode {
            PickerMode::Include => FolderLimitKind::ChatsIncluded,
            PickerMode::Exclude => FolderLimitKind::ChatsExcluded,
        };
        self.session()
            .map_or(0, |s| s.folder_limits.current(kind, s.my_is_premium()))
    }

    fn folder_avatar(&self, chat_id: i64, title: &str) -> AnyElement {
        let roots = self.media_display_roots();
        let photo = self
            .session()
            .and_then(|s| s.chat_photo_path(quill::ids::ChatId(chat_id)))
            .and_then(|path| sandboxed_display_path(path, &roots));
        chat_avatar(title, photo.as_deref(), AVATAR).into_any_element()
    }

    /// One list of the editor: title with its "N chats" counter, what it
    /// does, the chosen chats (each can be removed) and the add button.
    pub(super) fn folder_chat_section(
        &self,
        mode: PickerMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(dialog) = self.folder_editor.as_ref() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let chosen: Vec<i64> = {
            let set = match mode {
                PickerMode::Include => &dialog.editor.included,
                PickerMode::Exclude => &dialog.editor.excluded,
            };
            let mut ids: Vec<i64> = set.iter().copied().collect();
            ids.sort_unstable();
            ids
        };
        let titles = self.folder_picker_chats();
        let title_of = |id: i64| {
            titles
                .iter()
                .find(|(chat, _)| *chat == id)
                .map_or_else(|| format!("Chat {id}"), |(_, title)| title.clone())
        };
        let key = match mode {
            PickerMode::Include => "include",
            PickerMode::Exclude => "exclude",
        };
        let mut section = div()
            .id(format!("folder-section-{key}"))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .child(mode.section_title()),
                    )
                    .child(
                        div()
                            .id(format!("folder-count-{key}"))
                            .text_xs()
                            .text_color(muted)
                            .child(chats_count_text(chosen.len())),
                    ),
            )
            .child(div().text_xs().text_color(muted).child(mode.about()));
        for chat_id in chosen {
            let title = title_of(chat_id);
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_1()
                    .py_0p5()
                    .child(self.folder_avatar(chat_id, &title))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .child(super::bidi_line::one_line_plain(title.clone())),
                    )
                    .child(
                        Button::new((
                            if mode == PickerMode::Include {
                                "folder-include-remove"
                            } else {
                                "folder-exclude-remove"
                            },
                            chat_id as u64,
                        ))
                        .icon(IconName::X)
                        .ghost()
                        .tooltip("Remove")
                        .accessibility_label(format!("Remove {title}"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(dialog) = this.folder_editor.as_mut() {
                                match mode {
                                    PickerMode::Include => dialog.editor.included.remove(&chat_id),
                                    PickerMode::Exclude => dialog.editor.excluded.remove(&chat_id),
                                };
                            }
                            cx.notify();
                        })),
                    ),
            );
        }
        section
            .child(
                Button::new(format!("folder-add-{key}"))
                    .label(mode.add_label())
                    .icon(IconName::Plus)
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(dialog) = this.folder_editor.as_mut() {
                            dialog.picker = Some(mode);
                            dialog.picker_search.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });
                        }
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// The picker: title with "N / limit", a search field and the chats
    /// with checkboxes. Ticking past the limit opens the limit box.
    pub(super) fn folder_chat_picker(
        &self,
        mode: PickerMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(dialog) = self.folder_editor.as_ref() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let limit = self.folder_chat_limit(mode);
        let selected_count = match mode {
            PickerMode::Include => dialog.editor.included.len() + dialog.editor.pinned.len(),
            PickerMode::Exclude => dialog.editor.excluded.len(),
        };
        let query = dialog.picker_search.read(cx).value().to_string();
        let rows = filter_chats(&self.folder_picker_chats(), &query);
        let key = match mode {
            PickerMode::Include => "include",
            PickerMode::Exclude => "exclude",
        };
        let mut list = div()
            .id(format!("folder-picker-list-{key}"))
            .flex()
            .flex_col()
            .gap_0p5()
            .overflow_y_scroll()
            .max_h(px(300.));
        if rows.is_empty() {
            list = list.child(
                div()
                    .id("folder-picker-empty")
                    .py_3()
                    .text_sm()
                    .text_color(muted)
                    .child("No chats found."),
            );
        }
        for (chat_id, title) in rows {
            let ticked = match mode {
                PickerMode::Include => dialog.editor.included.contains(&chat_id),
                PickerMode::Exclude => dialog.editor.excluded.contains(&chat_id),
            };
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_1()
                    .py_0p5()
                    .child(self.folder_avatar(chat_id, &title))
                    .child(
                        Checkbox::new((
                            if mode == PickerMode::Include {
                                "folder-pick-include"
                            } else {
                                "folder-pick-exclude"
                            },
                            chat_id as u64,
                        ))
                        .checked(ticked)
                        .label(title)
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if on == ticked {
                                return;
                            }
                            if on && !may_tick(selected_count, limit, ticked) {
                                let kind = match mode {
                                    PickerMode::Include => FolderLimitKind::ChatsIncluded,
                                    PickerMode::Exclude => FolderLimitKind::ChatsExcluded,
                                };
                                this.show_folder_limit(kind, cx);
                                return;
                            }
                            if let Some(dialog) = this.folder_editor.as_mut() {
                                match mode {
                                    PickerMode::Include => dialog.editor.toggle_included(chat_id),
                                    PickerMode::Exclude => dialog.editor.toggle_excluded(chat_id),
                                }
                            }
                            cx.notify();
                        })),
                    ),
            );
        }
        div()
            .id("folder-picker")
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .justify_between()
                    .child(div().text_sm().font_semibold().child(mode.picker_title()))
                    .child(
                        div()
                            .id("folder-picker-counter")
                            .text_xs()
                            .text_color(muted)
                            .child(picker_counter(selected_count, limit)),
                    ),
            )
            .child(Textarea::new(&dialog.picker_search).aria_label("Search chats"))
            .child(list)
            .child(
                div().flex().justify_end().child(
                    Button::new("folder-picker-done")
                        .label("Done")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(dialog) = this.folder_editor.as_mut() {
                                dialog.picker = None;
                            }
                            cx.notify();
                        })),
                ),
            )
            .into_any_element()
    }
}
