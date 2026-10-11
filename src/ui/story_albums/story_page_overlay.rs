//! Methods moved out of `story_albums.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// Phase 9.7: the chat story page overlay — story albums (list +
    /// opened album), chat-page stories with pin/unpin, and the paginated
    /// archive list. The status line renders the honest
    /// `Session::story_page_op` state (Sending / Succeeded / Failed).
    pub(in crate::ui) fn story_page_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(page) = self.stories.page.as_ref() else {
            return div().into_any_element();
        };
        let chat_id = page.chat_id;
        let open_album = page.open_album;
        let delete_confirm = page.delete_confirm;
        let new_album_name = page.new_album_name.clone();
        let new_album_story_ids = page.new_album_story_ids.clone();
        let rename_input = page.rename_input.clone();
        let add_story_ids = page.add_story_ids.clone();
        let title = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|c| c.title.clone())
            .unwrap_or_else(|| format!("Chat {}", chat_id.0));
        // Honest operation state: Checking shows as `Sending` (the load
        // already fired), mutations label their action, failures carry
        // the TDLib reason.
        let op_status = self
            .session()
            .and_then(|s| s.stories.page_op.clone())
            .map(|op| match op.state {
                StoryPageOpState::Checking => format!("{}…", op.label),
                StoryPageOpState::Sending => format!("{}…", op.label),
                StoryPageOpState::Succeeded => format!("{} — done", op.label),
                StoryPageOpState::Failed(reason) => format!("{} — failed: {reason}", op.label),
            });
        let label_for = |story_id: i32| {
            let caption = self
                .session()
                .and_then(|s| s.stories.stories.get(&(chat_id.0, story_id)))
                .map(|story| story.caption.clone())
                .unwrap_or_default();
            let snippet: String = caption.chars().take(40).collect();
            if snippet.is_empty() {
                format!("Story {story_id}")
            } else {
                format!("Story {story_id} — {snippet}")
            }
        };

        let mut body = div().flex().flex_col().gap_3();
        if let Some(album_id) = open_album {
            // ---- Opened album: rename, delete, stories, add. ----
            let name = self
                .session()
                .and_then(|s| s.stories.albums.get(&chat_id.0))
                .and_then(|albums| albums.iter().find(|a| a.id == album_id))
                .map(|a| a.name.clone())
                .unwrap_or_else(|| format!("Album {album_id}"));
            let story_ids: Vec<i32> = self
                .session()
                .and_then(|s| s.stories.album_stories.get(&(chat_id.0, album_id)))
                .cloned()
                .unwrap_or_default();
            let mut detail = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("story-page-back")
                                .label("← Albums")
                                .ghost()
                                .text_color(cx.theme().foreground)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.back_to_story_albums(cx);
                                })),
                        )
                        .child(
                            div()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(name.clone()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Textarea::new(&rename_input)
                                .aria_label("Story album name")
                                .h(px(36.))
                                .flex_1(),
                        )
                        .child(Button::new("story-page-rename").label("Rename").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.rename_story_album(cx);
                            }),
                        )),
                )
                .child(
                    Button::new("story-page-delete")
                        .label(if delete_confirm == Some(album_id) {
                            "Confirm delete"
                        } else {
                            "Delete album"
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.delete_story_album(album_id, cx);
                        })),
                );
            for story_id in story_ids {
                let label = label_for(story_id);
                detail = detail.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(cx.theme().secondary)
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(label),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new(("story-page-album-top", story_id as u64))
                                        .label("↑ Top")
                                        .ghost()
                                        .text_color(cx.theme().foreground)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.move_story_to_album_top(album_id, story_id, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("story-page-album-remove", story_id as u64))
                                        .label("Remove")
                                        .ghost()
                                        .text_color(cx.theme().foreground)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.remove_story_from_album(album_id, story_id, cx);
                                        })),
                                ),
                        ),
                );
            }
            detail = detail.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Textarea::new(&add_story_ids)
                            .aria_label("Story identifiers to add")
                            .h(px(36.))
                            .flex_1(),
                    )
                    .child(Button::new("story-page-add-stories").label("Add").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.add_stories_to_album(cx);
                        }),
                    )),
            );
            body = body.child(detail);
        } else {
            // ---- Album list + create form. ----
            let albums: Vec<(i32, String)> = self
                .session()
                .and_then(|s| s.stories.albums.get(&chat_id.0))
                .map(|albums| albums.iter().map(|a| (a.id, a.name.clone())).collect())
                .unwrap_or_default();
            let mut list = div().flex().flex_col().gap_2().child(
                div()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child("Albums"),
            );
            for (index, (album_id, name)) in albums.iter().enumerate() {
                let album_id = *album_id;
                let at_top = index == 0;
                let at_bottom = index + 1 == albums.len();
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(cx.theme().secondary)
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(name.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new(("story-page-open", album_id as u64))
                                        .label("Open")
                                        .ghost()
                                        .text_color(cx.theme().foreground)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_story_album(album_id, window, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("story-page-up", album_id as u64))
                                        .label("↑")
                                        .ghost()
                                        .text_color(cx.theme().foreground)
                                        .disabled(at_top)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.move_story_album(album_id, true, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("story-page-down", album_id as u64))
                                        .label("↓")
                                        .ghost()
                                        .text_color(cx.theme().foreground)
                                        .disabled(at_bottom)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.move_story_album(album_id, false, cx);
                                        })),
                                ),
                        ),
                );
            }
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("New album"),
                    )
                    .child(
                        Textarea::new(&new_album_name)
                            .aria_label("New story album name")
                            .h(px(36.)),
                    )
                    .child(
                        Textarea::new(&new_album_story_ids)
                            .aria_label("Story identifiers for new album")
                            .h(px(36.)),
                    )
                    .child(
                        Button::new("story-page-create")
                            .label("Create album")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.create_story_album(cx);
                            })),
                    ),
            );
            // ---- Chat-page stories with pin/unpin. ----
            let (pinned, chat_page_ids): (Vec<i32>, Vec<i32>) = self
                .session()
                .and_then(|s| s.stories.chat_page_stories.get(&chat_id.0))
                .map(|state| (state.pinned_story_ids.clone(), state.story_ids.clone()))
                .unwrap_or_default();
            let mut chat_page = div().flex().flex_col().gap_2().child(
                div()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child("Chat page stories"),
            );
            for story_id in &chat_page_ids {
                let story_id = *story_id;
                let label = label_for(story_id);
                let is_pinned = pinned.contains(&story_id);
                chat_page = chat_page.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(cx.theme().secondary)
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(label),
                        )
                        .child(
                            Button::new(("story-page-pin", story_id as u64))
                                .label(if is_pinned { "Unpin" } else { "Pin" })
                                .ghost()
                                .text_color(cx.theme().foreground)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_story_pin(story_id, cx);
                                })),
                        ),
                );
            }
            chat_page = chat_page.child(
                Button::new("story-page-load-chat-page")
                    .label("Load more")
                    .ghost()
                    .text_color(cx.theme().foreground)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.load_more_chat_page_stories(cx);
                    })),
            );
            // ---- Archive. ----
            let archived_ids: Vec<i32> = self
                .session()
                .and_then(|s| s.stories.archived_stories.get(&chat_id.0))
                .map(|state| state.story_ids.clone())
                .unwrap_or_default();
            let mut archive = div().flex().flex_col().gap_2().child(
                div()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child("Archive"),
            );
            for story_id in &archived_ids {
                let label = label_for(*story_id);
                archive = archive.child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(cx.theme().secondary)
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(label),
                        ),
                );
            }
            archive = archive.child(
                Button::new("story-page-load-archive")
                    .label("Load more")
                    .ghost()
                    .text_color(cx.theme().foreground)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.load_more_archived_stories(cx);
                    })),
            );
            body = body.child(list).child(chat_page).child(archive);
        }

        div()
            .id("story-page-overlay")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("story-page-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(rgba(0x000000e6))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_story_page(cx);
                    })),
            )
            .child(
                div()
                    .id("story-page-panel")
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .max_w(px(560.))
                    .max_h_full()
                    .overflow_y_scroll()
                    .rounded_lg()
                    .bg(cx.theme().popover)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(format!("{title} — Stories")),
                            )
                            .child(
                                Button::new("story-page-close")
                                    .icon(gpui_kit::assets::IconName::X)
                                    .tooltip("Close")
                                    .accessibility_label("Close")
                                    .ghost()
                                    .text_color(cx.theme().foreground)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_story_page(cx);
                                    })),
                            ),
                    )
                    .when_some(op_status, |this, status| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(status),
                        )
                    })
                    .child(body),
            )
            .into_any_element()
    }
}
