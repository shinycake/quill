//! The neighbour posters beside the story, for a viewer opened from the
//! story tray (tdesktop `Media::Stories::Sibling`,
//! `media_stories_sibling.cpp`): the story each would open on, shrunk and
//! dimmed, with the poster's photo and name over it, placed by
//! [`quill::story_sibling_layout`]. A click opens that poster (`jumpFor`).
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::chat_row::chat_avatar;
use quill::story_sibling_layout::{SiblingLayout, sibling_layout};

/// tdesktop `kSiblingUserpicSize`: the photo's size against the sibling.
const SIBLING_PHOTO_SCALE: f32 = 0.3;
/// tdesktop `kSiblingFade` and `kSiblingFadeOver`: the dimming over the
/// picture, lighter under the pointer.
const SIBLING_FADE: f32 = 0.5;
const SIBLING_FADE_OVER: f32 = 0.4;
const SIBLING_GROUP: &str = "story-sibling";

impl QuillApp {
    /// The neighbour's picture when it still has to be downloaded.
    pub(in crate::ui) fn story_sibling_picture_to_download(&self, delta: i32) -> Option<FileId> {
        let item = self.story_sibling_item(delta)?;
        let session = self.session()?;
        let roots = self.media_display_roots();
        let local = story_viewer_display_path(&item, &session.media.files, &roots).is_some();
        let busy = file_is_downloading(
            item.download_file_id,
            &session.media.files,
            &session.media.downloads,
        );
        (!local && !busy && item.download_file_id.0 != 0).then_some(item.download_file_id)
    }

    /// Both neighbours, positioned against the story frame they are
    /// children of, for a window `window_width` wide.
    pub(in crate::ui) fn story_sibling_layer(
        &self,
        window_width: f32,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(layout) = sibling_layout(window_width) else {
            return Vec::new();
        };
        [
            (-1, layout.left_from_frame(window_width)),
            (1, layout.right_from_frame(window_width)),
        ]
        .into_iter()
        .filter_map(|(delta, left)| {
            let preview = self.story_sibling_preview(delta, &layout, cx)?;
            Some(
                div()
                    .absolute()
                    .left(px(left))
                    .top(relative(0.5))
                    .mt(px(-layout.height / 2.))
                    .child(preview)
                    .into_any_element(),
            )
        })
        .collect()
    }

    fn story_sibling_preview(
        &self,
        delta: i32,
        layout: &SiblingLayout,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let item = self.story_sibling_item(delta)?;
        let session = self.session()?;
        let roots = self.media_display_roots();
        let picture = story_viewer_display_path(&item, &session.media.files, &roots);
        let title = session
            .chats
            .get(&item.chat_id.0)
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| {
                tr_with(
                    keys::Q_MEDIA_CHAT_2,
                    &[("chat_id", &item.chat_id.0.to_string())],
                )
            });
        let photo = session
            .chat_photo_path(item.chat_id)
            .and_then(|path| sandboxed_display_path(path, &roots));
        let (width, height) = (layout.width, layout.height);
        let fill = || div().absolute().top_0().left_0().right_0().bottom_0();
        let picture: AnyElement = match picture {
            Some(path) => img(path)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => fill().bg(chrome::frame_bg()).into_any_element(),
        };
        Some(
            div()
                .id(("story-sibling", item.chat_id.0 as u64))
                .debug_selector(move || {
                    if delta < 0 {
                        "story-sibling-left"
                    } else {
                        "story-sibling-right"
                    }
                    .into()
                })
                .group(SIBLING_GROUP)
                .occlude()
                .relative()
                .flex_none()
                .w(px(width))
                .h(px(height))
                .rounded_md()
                .overflow_hidden()
                .role(gpui_kit::Role::Button)
                .aria_label(tr_with(
                    keys::Q_MEDIA_OPEN_STORIES_FOR,
                    &[("title", &title)],
                ))
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .child(picture)
                .child(
                    fill()
                        .bg(hsla(0., 0., 0., SIBLING_FADE))
                        .group_hover(SIBLING_GROUP, |style| {
                            style.bg(hsla(0., 0., 0., SIBLING_FADE_OVER))
                        }),
                )
                .child(
                    fill()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .px_2()
                        .child(chat_avatar(
                            &title,
                            photo.as_deref(),
                            width * SIBLING_PHOTO_SCALE,
                        ))
                        .child(
                            div()
                                .max_w_full()
                                .truncate()
                                .text_sm()
                                .font_semibold()
                                .text_color(chrome::fg())
                                .child(title),
                        ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.jump_story_poster(delta, cx);
                }))
                .into_any_element(),
        )
    }
}
