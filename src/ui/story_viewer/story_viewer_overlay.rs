//! Methods moved out of `story_viewer.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::chat_row::chat_avatar;
use quill::lang::{keys, tr, tr_with, trn};

impl QuillApp {
    /// Phase 9.1: fullscreen story overlay, modeled on
    /// `media_viewer_overlay`: the poster header (photo, name, position,
    /// age, privacy badge), the
    /// photo (video shows its thumbnail; live/unsupported show a
    /// placeholder), the caption, and Prev / Next / Close controls. The
    /// backdrop click and Escape (see `cancel_search`) close it.
    pub(in crate::ui) fn story_viewer_overlay(
        &mut self,
        window_width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // The shared emoji / sticker / GIF panel while the reply row has it.
        let reply_panel = self.story_reply_panel_layer(cx);
        let item = self
            .stories
            .viewer
            .current()
            .cloned()
            .unwrap_or_else(|| StoryViewerItem {
                chat_id: ChatId(0),
                story_id: 0,
                kind: StoryViewerKind::Unsupported,
                display_file_ids: Vec::new(),
                download_file_id: FileId(0),
                caption: String::new(),
                caption_entities: Vec::new(),
                duration_label: None,
                duration_secs: None,
                video_file_id: None,
                is_live: false,
                live_call: None,
                areas: Vec::new(),
            });
        let (position, total) = self.stories.viewer.position().unwrap_or((0, 0));
        let now = Instant::now();
        let poster = self
            .session()
            .and_then(|s| s.chats.get(&item.chat_id.0))
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| {
                tr_with(keys::Q_MEDIA_CHAT_3, &[("0", &item.chat_id.0.to_string())])
            });
        let no_files = quill::state::FileMap::default();
        let no_downloads = quill::state::DownloadIntents::default();
        let (files, downloading): (&quill::state::FileMap, &quill::state::DownloadIntents) =
            self.session().map_or((&no_files, &no_downloads), |s| {
                (&s.media.files, &s.media.downloads)
            });
        let roots = self.media_display_roots();
        let path = story_viewer_display_path(&item, files, &roots);
        let downloading_now = item
            .display_file_ids
            .iter()
            .chain(std::iter::once(&item.download_file_id))
            .any(|id| file_is_downloading(*id, files, downloading));
        let kind_label = item.kind.label();
        // `path` is the poster: a video story's `display_file_ids` hold
        // only its thumbnail (`quill::story_viewer`), never the clip.
        let video_frame = self.story_video_element(path.clone());
        // A joined live story: the stream and its comments.
        let live_media = self.live_story_media(&item, cx);
        let live_watch = live_media.is_some();
        let visual: AnyElement = if let Some(media) = live_media {
            media
        } else if let Some(frame) = video_frame {
            frame
        } else if let Some(path) = path {
            img(path)
                .id(("story-viewer-img", item.story_id as u64))
                .w(px(360.))
                .h(px(640.))
                .aspect_ratio(px(360.) / px(640.))
                .rounded_md()
                .object_fit(ObjectFit::Contain)
                .bg(chrome::frame_bg())
                .with_fallback(move || {
                    div()
                        .w(px(360.))
                        .h(px(640.))
                        .rounded_md()
                        .bg(chrome::frame_bg())
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(chrome::fg())
                        .child(tr_with(
                            keys::Q_MEDIA_COULD_NOT_RENDER_2,
                            &[("kind_label", kind_label)],
                        ))
                        .into_any_element()
                })
                .into_any_element()
        } else {
            let status = if downloading_now {
                tr_with(keys::Q_MEDIA_DOWNLOADING_2, &[("kind_label", kind_label)])
            } else {
                tr_with(
                    keys::Q_MEDIA_NOT_DOWNLOADED_2,
                    &[("kind_label", kind_label)],
                )
            };
            let status = match (&item.duration_label, downloading_now) {
                (Some(duration), _) => tr_with(
                    keys::Q_MEDIA_VIDEO_3,
                    &[
                        ("duration", &duration.to_string()),
                        ("status", &status.to_string()),
                    ],
                ),
                _ => status,
            };
            let joinable = matches!(item.kind, StoryViewerKind::Live) && item.live_call.is_some();
            let body: AnyElement = if joinable {
                let chat_id = item.chat_id;
                let story_id = item.story_id;
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .text_color(chrome::fg())
                            .child(tr(keys::Q_MEDIA_LIVE_STORY)),
                    )
                    .child(
                        Button::new("story-join-live")
                            .label(tr(keys::Q_MEDIA_JOIN_LIVE))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.join_live_story_from_viewer(chat_id, story_id, cx);
                            })),
                    )
                    .into_any_element()
            } else {
                let status = if matches!(item.kind, StoryViewerKind::Unsupported) {
                    tr_with(
                        keys::Q_MEDIA_NOT_SUPPORTED_IN_THIS_SLICE,
                        &[("kind_label", kind_label)],
                    )
                } else {
                    status
                };
                div()
                    .text_sm()
                    .text_color(chrome::fg())
                    .child(status)
                    .into_any_element()
            };
            div()
                .id(("story-viewer-loading", item.story_id as u64))
                .w(px(360.))
                .h(px(640.))
                .rounded_md()
                .bg(chrome::frame_bg())
                .flex()
                .items_center()
                .justify_center()
                .child(body)
                .into_any_element()
        };
        // Phase 9.8: clickable story areas — chips over the 360x640 media
        // box, centered on the `areaPosition` x/y fractions
        // (`schema/td_api.tl:1184`; x/y are the rectangle's CENTER). The
        // media box is the positioning context (areas are media fractions)
        // and clips overflowing chips; `rotation_angle` is not rendered in
        // this slice.
        let area_chips: Vec<AnyElement> = item
            .areas
            .iter()
            .enumerate()
            .map(|(index, area)| {
                let kind = area.kind.clone();
                let label = Self::story_area_label(&kind);
                // `areaPosition` x/y are the rectangle's CENTER
                // (`schema/td_api.tl:1184`): the chip's top-left is the
                // center minus half the chip size.
                let chip_w = (area.width * 360.0).max(48.0) as f32;
                let chip_h = (area.height * 640.0).max(24.0) as f32;
                div()
                    .id((
                        "story-area",
                        (item.story_id as u64).wrapping_mul(1000) + index as u64,
                    ))
                    .absolute()
                    .left(px(area.x as f32 * 360.0 - chip_w / 2.0))
                    .top(px(area.y as f32 * 640.0 - chip_h / 2.0))
                    .w(px(chip_w))
                    .h(px(chip_h))
                    .flex()
                    .items_center()
                    .justify_center()
                    .role(gpui_kit::Role::Button)
                    .aria_label(label.clone())
                    .tab_index(0)
                    .cursor_pointer()
                    .rounded_md()
                    .bg(rgba(0x00000099))
                    .border_1()
                    .border_color(rgba(0xffffff66))
                    .text_xs()
                    .text_color(rgb(0xffffff))
                    .px_2()
                    .child(label)
                    // tdesktop only pauses on a press that is not on a
                    // clickable area (`ClickHandler::getPressed()`); keep
                    // the press from reaching the media's hold-to-pause.
                    .swallow_press()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.story_area_click(&kind, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        // B14: press-and-hold on the media pauses until release.
        let paused_chip = self.stories.pause.is_paused().then(|| {
            div()
                .absolute()
                .left(px(8.))
                .bottom(px(8.))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgba(0x00000099))
                .text_xs()
                .text_color(rgb(0xffffff))
                .child(tr(keys::Q_MEDIA_PAUSED))
        });
        let visual: AnyElement = div()
            .id(("story-viewer-media", item.story_id as u64))
            .relative()
            .w(px(360.))
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.story_hold(true, cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.story_hold(false, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.story_hold(false, cx)),
            )
            .child(visual)
            .children(area_chips)
            .children(paused_chip)
            .into_any_element();
        // The neighbour posters hang off the frame, outside its clip, so
        // they centre on it whatever the panel holds above and below. The
        // frame still gives way, clipped, when the window is too short.
        let visual: AnyElement = div()
            .debug_selector(|| "story-viewer-frame".into())
            .relative()
            .flex()
            .flex_col()
            .min_h_0()
            .child(visual)
            .children(self.story_sibling_layer(window_width, cx))
            .into_any_element();
        // A live stream has no caption, progress or reactions row
        // (tdesktop hides them for a video stream).
        let caption: Option<AnyElement> = (!item.caption.is_empty() && !live_watch).then(|| {
            rich_text_line(
                &item.caption,
                &item.caption_entities,
                (item.chat_id.0, item.story_id as u64),
                true,
                &self.message_ui.spoiler_revealed,
                // Settings → Appearance: captions follow the message font size.
                self.msg_font(),
                // Captions don't resolve custom emoji in this slice (text fallback).
                &HashMap::new(),
                cx,
            )
        });
        let live_volume = self.live_story_volume(cx);
        let live_header = self.live_story_header(live_volume, cx);
        let live_composer = self.live_story_composer(cx);
        div()
            .id("story-viewer-overlay")
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
                    .id("story-viewer-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(scrim())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_story_viewer(cx);
                    })),
            )
            .child(
                div()
                    .id("story-viewer-panel")
                    .debug_selector(|| "story-viewer-panel".into())
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .p_4()
                    .flex_none()
                    .w(px(quill::story_sibling_layout::STORY_COLUMN_WIDTH))
                    .max_h_full()
                    .when(!live_watch, |this| {
                        this.child(self.story_progress_bar(position, total, now))
                    })
                    .child(
                        div()
                            .flex()
                            .w(px(360.))
                            .items_center()
                            .gap_2()
                            .child(self.story_header_identity(&item, &poster, position, total, cx))
                            .children(
                                live_header.map(|controls| div().flex_shrink_0().child(controls)),
                            )
                            .child(self.story_header_buttons(live_watch, cx)),
                    )
                    .child(visual)
                    .when_some(caption, |this, caption| {
                        this.child(div().text_color(chrome::fg()).child(caption))
                    })
                    // Phase 9.5: "Reposted from ...", a link to the origin
                    // ("edited" is in the header).
                    .children(self.story_meta_row(cx))
                    .when(!live_watch, |this| {
                        this.child(self.story_bottom_bar(cx))
                            .child(self.story_more_row(cx))
                            .child(self.story_action_row(cx))
                    })
                    .children(live_composer)
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("story-viewer-prev")
                                    .label(tr(keys::Q_MEDIA_PREV))
                                    .custom(chrome::ghost(cx))
                                    .disabled(!self.story_step_available(-1))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_story_viewer(-1, cx);
                                    })),
                            )
                            .child(
                                Button::new("story-viewer-next")
                                    .label(tr(keys::Q_MEDIA_NEXT))
                                    .custom(chrome::ghost(cx))
                                    .disabled(!self.story_step_available(1))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_story_viewer(1, cx);
                                    })),
                            ),
                    ),
            )
            .children(reply_panel)
    }

    /// tdesktop's story header (`media_stories_header.cpp`): the poster's
    /// photo with a privacy badge, the name, the story's position after it
    /// (`ComposeCounter`, its own label so the name truncates first,
    /// `:492-509`), and how long ago it was posted. A watched stream shows
    /// the LIVE badge and its viewers there instead, with no age and no
    /// "edited" (`setVideoStreamViewers`, `updateDateText`, `:544-572`,
    /// `:964-967`). Pressing it opens the poster's profile (`:343-345`)
    /// when there is one; hovering the badge says who can see the story
    /// (`:738-765`). The age comes from the clock on every draw, and the
    /// viewer's 100 ms tick redraws it.
    fn story_header_identity(
        &self,
        item: &StoryViewerItem,
        poster: &str,
        position: usize,
        total: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let story = self.current_story();
        let me = self.session().and_then(|s| s.my_user_id);
        let is_self = self.session().is_some_and(|s| {
            me.is_some()
                && s.info_panel_target_for_chat(item.chat_id)
                    == me.map(quill::state::InfoPanelTarget::User)
        });
        let name = if is_self {
            tr(keys::STORIES_MY_NAME).to_string()
        } else {
            poster.to_owned()
        };
        let counter = (total > 1).then(|| format!("{STORY_BULLET} {position}/{total}"));
        let photo = self
            .session()
            .and_then(|s| s.chat_photo_path(item.chat_id))
            .and_then(|path| sandboxed_display_path(path, &self.media_display_roots()));
        let badge = story
            .as_ref()
            .and_then(|story| quill::story_header::privacy_badge(story.privacy_settings.as_ref()));
        let about = badge.map(|badge| story_privacy_about(badge, is_self, poster));
        let second_line = match self.live_story_viewers() {
            Some(viewers) => Some(
                crate::ui::video_player::controls::live_badge(viewers)
                    .text_xs()
                    .text_color(chrome::fg())
                    .into_any_element(),
            ),
            None => story.as_ref().map(|story| {
                let mut detail = story_age_label(story.date);
                if story.is_edited {
                    detail = format!("{detail} {STORY_BULLET} {}", tr(keys::EDITED));
                }
                div()
                    .debug_selector(|| "story-header-age".into())
                    .text_xs()
                    .text_color(chrome::secondary())
                    .truncate()
                    .child(detail)
                    .into_any_element()
            }),
        };
        let sender = MessageSender::Chat {
            chat_id: item.chat_id.0,
        };
        let has_profile = self
            .session()
            .and_then(|s| s.avatar_profile_target(sender))
            .is_some();
        let mut avatar = div()
            .relative()
            .flex_shrink_0()
            .size(px(36.))
            .child(chat_avatar(poster, photo.as_deref(), 36.));
        if let (Some(badge), Some(about)) = (badge, about) {
            avatar = avatar.child(story_privacy_badge(badge, about));
        }
        let name_row = div()
            .flex()
            .items_baseline()
            .gap_1()
            .min_w_0()
            .child(
                div()
                    .debug_selector(|| "story-header-name".into())
                    .font_semibold()
                    .text_color(chrome::fg())
                    .min_w_0()
                    .truncate()
                    .child(super::bidi_line::one_line_plain(name.clone())),
            )
            .children(counter.map(|counter| {
                div()
                    .debug_selector(|| "story-header-counter".into())
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(chrome::secondary())
                    .child(counter)
            }));
        div()
            .id("story-viewer-identity")
            .debug_selector(|| "story-header-identity".into())
            .flex()
            .flex_1()
            .items_center()
            .gap_2()
            .min_w_0()
            .when(has_profile, |identity| {
                identity
                    .role(gpui_kit::Role::Button)
                    .aria_label(name)
                    .tab_index(0)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_story_poster_profile(sender, window, cx);
                    }))
            })
            .child(avatar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(name_row)
                    .children(second_line),
            )
            .into_any_element()
    }

    /// Pressing the header: the poster's profile, after the viewer closes
    /// (the same hand-off as the "Stories" button).
    pub(super) fn open_story_poster_profile(
        &mut self,
        sender: MessageSender,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_story_viewer(cx);
        self.open_avatar_profile(sender, window, cx);
    }

    /// The line under the caption: the repost label, a button that opens
    /// its origin.
    fn story_meta_row(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let label = self.story_repost_line()?;
        Some(
            div()
                .id("story-viewer-repost")
                .debug_selector(|| "story-viewer-repost".into())
                .role(gpui_kit::Role::Button)
                .aria_label(label.clone())
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .rounded_sm()
                .px_1()
                .text_xs()
                .text_color(chrome::fg())
                .child(label)
                .on_click(cx.listener(|this, _, window, cx| {
                    // The backdrop under the panel closes the viewer.
                    cx.stop_propagation();
                    this.press_story_repost(window, cx);
                })),
        )
    }

    /// Phase 9.6: segmented progress bar — one segment per story in the
    /// viewer sequence; viewed segments full, the current one fills with
    /// playback progress, upcoming ones dim. Matches Telegram Android's
    /// `StoryLinesDrawable` (segment `a < index` full, `a == index` partial,
    /// the rest a dim track, `StoryLinesDrawable.java:111-140`) and
    /// Unigram's `StoryProgress` (viewed opacity 1, upcoming 0.3,
    /// `StoryContent.xaml.cs:2117`).
    pub(in crate::ui) fn story_progress_bar(
        &self,
        position: usize,
        total: usize,
        now: Instant,
    ) -> impl IntoElement {
        let current_progress = self.story_segment_progress(now);
        div()
            .id("story-viewer-progress")
            .flex()
            .w(px(360.))
            .gap_1()
            .children((0..total).map(|i| {
                let fill = if i + 1 < position {
                    1.0
                } else if i + 1 == position {
                    current_progress
                } else {
                    0.0
                };
                div()
                    .flex_1()
                    .h(px(3.))
                    .rounded_full()
                    .bg(rgba(0xffffff4d))
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .w(relative(fill)),
                    )
            }))
    }
}

/// "just now", "5 minutes ago", "3 hours ago", "today at 14:05" ...
/// (tdesktop `ComposeTimestamp`), from the clock at this draw.
fn story_age_label(date: i32) -> String {
    use quill::local_time::{hhmm, now_unix, utc_offset_at};
    use quill::story_header::{Age, age};
    let (when, now) = (i64::from(date), now_unix());
    let time = |t: &quill::local_time::CivilTime| hhmm(t);
    match age(when, now, utc_offset_at(when), utc_offset_at(now)) {
        Age::JustNow => tr(keys::MEDIAVIEW_JUST_NOW).to_string(),
        Age::MinutesAgo(n) => trn(keys::MEDIAVIEW_MINUTES_AGO, n),
        Age::HoursAgo(n) => trn(keys::MEDIAVIEW_HOURS_AGO, n),
        Age::Today(t) => tr_with(keys::MEDIAVIEW_TODAY, &[("time", &time(&t))]),
        Age::Yesterday(t) => tr_with(keys::MEDIAVIEW_YESTERDAY, &[("time", &time(&t))]),
        Age::Date(t) => tr_with(
            keys::MEDIAVIEW_DATE_TIME,
            &[
                (
                    "date",
                    &format!("{:02}.{:02}.{:02}", t.day, t.month, t.year.rem_euclid(100)),
                ),
                ("time", &time(&t)),
            ],
        ),
    }
}

/// The tooltip text of the privacy badge (`lng_stories_about_*`), without
/// the markup tdesktop bolds.
fn story_privacy_about(
    badge: quill::story_header::PrivacyBadge,
    is_self: bool,
    poster: &str,
) -> String {
    use quill::story_header::PrivacyBadge;
    let key = match (badge, is_self) {
        (PrivacyBadge::CloseFriends, true) => keys::STORIES_ABOUT_CLOSE_FRIENDS_MY,
        (PrivacyBadge::CloseFriends, false) => keys::STORIES_ABOUT_CLOSE_FRIENDS,
        (PrivacyBadge::Contacts, true) => keys::STORIES_ABOUT_CONTACTS_MY,
        (PrivacyBadge::Contacts, false) => keys::STORIES_ABOUT_CONTACTS,
        (PrivacyBadge::SelectedContacts, true) => keys::STORIES_ABOUT_SELECTED_CONTACTS_MY,
        (PrivacyBadge::SelectedContacts, false) => keys::STORIES_ABOUT_SELECTED_CONTACTS,
    };
    tr_with(key, &[("user", poster)]).replace("**", "")
}

/// tdesktop's `kQBullet`, between the name and the position and between
/// the age and "edited".
const STORY_BULLET: char = '\u{2022}';

/// The badge on the photo's corner: a disc with the audience's icon and
/// tdesktop's `UserpicBadge` top-to-bottom gradient (`LookupPrivacyBadge`):
/// green `historyPeer2UserpicBg` for close friends, purple
/// `historyPeer5UserpicBg` for contacts, orange `historyPeer8UserpicBg` for
/// selected contacts. The story chrome is dark in both themes, so the
/// palette's fixed userpic colours stand as they are.
fn story_privacy_badge(badge: quill::story_header::PrivacyBadge, about: String) -> AnyElement {
    use gpui_kit::assets::IconName;
    use gpui_kit::component::tooltip::Tooltip;
    use quill::story_header::PrivacyBadge;
    let (icon, top, bottom) = match badge {
        PrivacyBadge::CloseFriends => (IconName::Star, 0x9ad164, 0x46ba43),
        PrivacyBadge::Contacts => (IconName::User, 0xb694f9, 0x6c61df),
        PrivacyBadge::SelectedContacts => (IconName::Users, 0xfebb5b, 0xf68136),
    };
    div()
        .id("story-viewer-privacy-badge")
        .absolute()
        .right(px(-3.))
        .bottom(px(-3.))
        .size(px(16.))
        .rounded_full()
        .border_2()
        .border_color(chrome::frame_bg())
        .bg(linear_gradient(
            180.,
            linear_color_stop(rgb(top), 0.),
            linear_color_stop(rgb(bottom), 1.),
        ))
        .flex()
        .items_center()
        .justify_center()
        .text_color(chrome::fg())
        .tooltip(move |window, cx| Tooltip::new(about.clone()).build(window, cx))
        .child(gpui_kit::component::Icon::new(icon).size(px(10.)))
        .into_any_element()
}
