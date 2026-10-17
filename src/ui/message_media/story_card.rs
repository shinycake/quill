//! `messageStory`: a forwarded story, and a story mention.
//!
//! tdesktop `MediaStory::createView` (`data/data_media_types.cpp`) shows a
//! forwarded story as the story's own photo or video with its caption, an
//! expired one as "This story has expired.", and a mention as a service
//! box (`HistoryView::StoryMention`): the story as a round picture over a
//! "View Story" button. A click opens the story viewer. The story comes
//! from `getStory`, fetched with the open chat's replies.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};
use quill::service_text::{Seg, ServiceText};
use quill::state::{StoryCardState, StoryPreview};

/// Side of the round picture of a story mention (`st::storyMentionSize`).
const MENTION_SIDE: f32 = 72.0;

/// First downloaded picture of a story preview inside the media roots.
fn picture_path(
    preview: &StoryPreview,
    files: &HashMap<i32, ParsedFile>,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    preview.picture_ids.iter().find_map(|id| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, roots))
    })
}

/// Card frame of a forwarded story: the box a photo or video of the
/// story's size gets in a bubble.
pub(in crate::ui) fn story_frame(preview: Option<&StoryPreview>) -> (Pixels, Pixels) {
    let (kind, w, h) = match preview {
        Some(p) if p.duration_secs.is_some() => (MediaFrameKind::Video, p.width, p.height),
        Some(p) => (MediaFrameKind::Photo, p.width, p.height),
        None => (MediaFrameKind::Photo, 720, 1280),
    };
    media_frame_for(kind, w, h, 0)
}

/// The "no longer available" wording of an expired mention
/// (`lng_action_story_mention_unavailable` and `_me_unavailable`): the
/// mention text's name is kept as the link.
pub(in crate::ui) fn expired_mention_text(mention: &ServiceText, outgoing: bool) -> ServiceText {
    let name = mention
        .segs
        .iter()
        .find(|seg| seg.link.is_some())
        .cloned()
        .unwrap_or(Seg {
            text: tr(keys::Q_MEDIA_SOMEONE).into(),
            link: None,
        });
    let plain = |text: &str| Seg {
        text: text.into(),
        link: None,
    };
    let segs = if outgoing {
        vec![
            plain(tr(keys::Q_MEDIA_THE_STORY_WHERE_YOU_MENTIONED)),
            name,
            plain(tr(keys::Q_MEDIA_IS_NO_LONGER_AVAILABLE)),
        ]
    } else {
        vec![
            plain(tr(keys::Q_MEDIA_THE_STORY_WHERE)),
            name,
            plain(tr(keys::Q_MEDIA_MENTIONED_YOU_IS_NO_LONGER)),
        ]
    };
    ServiceText { segs }
}

impl QuillApp {
    /// A press on a story message: the viewer opens on that story. An
    /// expired one only says so.
    pub(in crate::ui) fn open_message_story(
        &mut self,
        poster: ChatId,
        story_id: i32,
        cx: &mut Context<Self>,
    ) {
        let state = self
            .session()
            .map(|s| s.story_card_state(poster.0, story_id));
        if state == Some(StoryCardState::Expired) {
            self.set_status_note(StatusNote::silent(tr(keys::FORWARDED_STORY_EXPIRED)), cx);
            return;
        }
        self.open_story_viewer(poster, story_id, cx);
    }
}

/// A forwarded story in a bubble.
pub(in crate::ui) fn story_card(
    row_id: u64,
    poster: ChatId,
    story_id: i32,
    session: Option<&Session>,
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let state = session.map_or(StoryCardState::Loading, |s| {
        s.story_card_state(poster.0, story_id)
    });
    if state == StoryCardState::Expired {
        return div()
            .id(("story-expired", row_id))
            .role(gpui_kit::Role::Label)
            .aria_label(tr(keys::FORWARDED_STORY_EXPIRED))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(fill_muted())
            .text_sm()
            .text_color(text_muted())
            .child(tr(keys::FORWARDED_STORY_EXPIRED))
            .into_any_element();
    }
    let preview = session.and_then(|s| s.story_preview(poster.0, story_id));
    let (width, height) = story_frame(preview.as_ref());
    let mut tile = div()
        .id(("story-card", row_id))
        .relative()
        .w(width)
        .h(height)
        .overflow_hidden()
        .rounded_md()
        .bg(fill_muted())
        .role(gpui_kit::Role::Button)
        .aria_label(tr(keys::Q_MEDIA_OPEN_STORY))
        .tab_index(0)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_message_story(poster, story_id, cx);
        }));
    if let Some(path) = preview
        .as_ref()
        .and_then(|p| picture_path(p, files, media_roots))
    {
        tile = tile.child(
            img(crate::ui::image_budget::sized_media(
                &path,
                (width, height),
                preview.as_ref().map(|p| (p.width, p.height)),
                crate::ui::image_budget::Fit::Cover,
            ))
            .size_full()
            .object_fit(ObjectFit::Cover),
        );
    }
    // Over the picture the colors are fixed (they sit on media, not on
    // the theme), like the video badges.
    let badge = |label: String| {
        div()
            .px_2()
            .py_0p5()
            .rounded_full()
            .bg(gpui_kit::black().opacity(0.55))
            .text_color(gpui_kit::white())
            .text_xs()
            .child(label)
    };
    tile = tile.child(
        div()
            .absolute()
            .top_2()
            .left_2()
            .child(badge(tr(keys::IN_DLG_STORY).into())),
    );
    if let Some(duration) = preview.as_ref().and_then(|p| p.duration_secs) {
        tile = tile
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(media_disc(("story-play", row_id), MediaDisc::Play)),
            )
            .child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .child(badge(format_voice_duration(duration))),
            );
    }
    let caption = preview
        .map(|p| p.caption)
        .filter(|caption| !caption.is_empty());
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(tile)
        .when_some(caption, |this, caption| {
            this.child(div().text_sm().child(caption))
        })
        .into_any_element()
}

/// The round picture and "View Story" button under a story-mention row.
/// `None` once the story is gone (the pill then says so).
pub(in crate::ui) fn story_mention_extras(
    row_id: u64,
    poster: ChatId,
    story_id: i32,
    session: Option<&Session>,
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let state = session?.story_card_state(poster.0, story_id);
    if state == StoryCardState::Expired {
        return None;
    }
    let preview = session.and_then(|s| s.story_preview(poster.0, story_id));
    let path = preview
        .as_ref()
        .and_then(|p| picture_path(p, files, media_roots));
    let dims = preview.as_ref().map(|p| (p.width, p.height));
    // tdesktop draws the ring in both states: the unread gradient, or the
    // thinner muted outline once read (`StoryMention::draw`).
    let ring = quill::story_ring::StoryRing {
        count: 1,
        unread: usize::from(session.is_some_and(|s| s.story_is_unread(poster.0, story_id))),
    };
    let circle = div()
        .id(("story-mention", row_id))
        .flex_none()
        .role(gpui_kit::Role::Button)
        .aria_label(tr(keys::VIEW_BUTTON_STORY))
        .tab_index(0)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_message_story(poster, story_id, cx);
        }))
        .child(crate::ui::story_ring::with_story_ring(
            move |avatar| {
                let side = px(avatar);
                let base = div()
                    .size(side)
                    .rounded_full()
                    .overflow_hidden()
                    .bg(fill_muted());
                match path.clone() {
                    Some(path) => base
                        .child(
                            img(crate::ui::image_budget::sized_media(
                                &path,
                                (side, side),
                                dims,
                                crate::ui::image_budget::Fit::Cover,
                            ))
                            .size_full()
                            .rounded_full()
                            .object_fit(ObjectFit::Cover),
                        )
                        .into_any_element(),
                    None => base.into_any_element(),
                }
            },
            ring,
            MENTION_SIDE,
            text_muted().into(),
        ));
    Some(
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(circle)
            .child(
                Button::new(format!("story-mention-view-{row_id}"))
                    .label(tr(keys::VIEW_BUTTON_STORY))
                    .small()
                    .primary()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_message_story(poster, story_id, cx);
                    })),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::expired_mention_text;
    use quill::service_text::{Seg, ServiceLink, ServiceText};
    use quill::telegram::envelope::MessageSender;

    fn mention() -> ServiceText {
        ServiceText {
            segs: vec![
                Seg {
                    text: "Dana".into(),
                    link: Some(ServiceLink::Sender(MessageSender::Chat { chat_id: 5 })),
                },
                Seg {
                    text: " mentioned you in a story".into(),
                    link: None,
                },
            ],
        }
    }

    #[test]
    fn an_expired_mention_says_the_story_is_gone() {
        assert_eq!(
            expired_mention_text(&mention(), false).plain(),
            "The story where Dana mentioned you is no longer available."
        );
        assert_eq!(
            expired_mention_text(&mention(), true).plain(),
            "The story where you mentioned Dana is no longer available."
        );
    }

    #[test]
    fn the_name_stays_a_link() {
        let text = expired_mention_text(&mention(), false);
        assert_eq!(text.spans().1.len(), 1);
    }
}
