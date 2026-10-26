//! A person's profile cover facts (tdesktop `Info::Profile::TopBar`):
//! the story ring around the avatar, the status line with the Stars
//! rating pill and the hidden last seen button, and the Saved Messages
//! cover of the Saved Messages chat. What shows is `quill::profile_cover`.
#![deny(clippy::indexing_slicing)]

mod rating_box;
mod show_or_premium_box;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;

use super::app::QuillApp;
use super::chat_row::saved_messages_avatar;
use super::video_avatar::COVER_SIZE;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, trn};
use quill::profile_cover::{Viewer, saved_chats_status, shows_last_seen_button};
use quill::stars_rating::StarsRating;
use quill::state::PhotoOwner;
use quill::story_ring::UNREAD_WIDTH;
use quill::story_viewer::StoryOpenScope;
use quill::telegram::envelope::ParsedUser;
pub(in crate::ui) use show_or_premium_box::{ShowOrPremium, ShowOrPremiumBox};

/// The open cover boxes (a `QuillApp` field).
#[derive(Debug, Default)]
pub(super) struct ProfileCoverUi {
    /// The person whose Stars rating box is open.
    pub(super) rating_box: Option<i64>,
    /// The open "Show Your Last Seen" / "Show Your Read Date" box.
    pub(super) show_or_premium: Option<ShowOrPremiumBox>,
    /// The rating box shows your pending rating ("Future Rating").
    pub(super) rating_preview: bool,
}

/// The ring's box around the cover: the photo keeps its size inside it.
const RINGED_COVER: f32 = COVER_SIZE + 3. * UNREAD_WIDTH;

impl QuillApp {
    /// The person's cover avatar: `still` (photo or initials, with the
    /// video clip over it), ringed while they have active stories. A
    /// click opens the stories, else the photos (tdesktop
    /// `setupUserpicButton`).
    pub(in crate::ui) fn user_cover_avatar(
        &self,
        user_id: i64,
        still: AnyElement,
        has_photo: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let ring = session.and_then(|s| s.chat_story_ring(user_id));
        let story = session
            .and_then(|s| s.stories.tray.get(&user_id))
            .and_then(quill::story_tray_walk::story_to_open);
        let avatar = match ring {
            Some(ring) => super::story_ring::with_story_ring(
                move |_| still,
                ring,
                RINGED_COVER,
                cx.theme().muted_foreground,
            ),
            None => still,
        };
        let opens_story = ring.is_some() && story.is_some();
        if !opens_story && !has_photo {
            return avatar;
        }
        let label = if opens_story {
            tr(keys::VIEW_BUTTON_STORY)
        } else {
            tr(keys::Q_PEOPLE_VIEW_PROFILE_PHOTOS)
        };
        div()
            .id(("info-panel-photo-open", user_id as u64))
            .debug_selector(|| "profile-cover-avatar".to_string())
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .when(opens_story, |this| {
                this.tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
            })
            .on_click(cx.listener(move |this, _, _, cx| match story {
                Some(story) if opens_story => {
                    this.open_story_viewer(ChatId(user_id), story, StoryOpenScope::Tray, cx)
                }
                _ => this.open_peer_photos(PhotoOwner::User(user_id), cx),
            }))
            .child(avatar)
            .into_any_element()
    }

    /// The status line under a person's name: the rating pill, the status
    /// and the "when?" button (tdesktop `updateStatusPosition`).
    pub(in crate::ui) fn user_cover_status(
        &self,
        user: &ParsedUser,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let rating = session
            .and_then(|s| s.user_full_info(user.id))
            .and_then(|info| info.ratings.rating);
        let viewer = Viewer {
            is_self: session.and_then(|s| s.my_user_id) == Some(user.id),
            am_premium: session.is_some_and(|s| s.is_premium()),
            premium_possible: session.is_none_or(|s| s.premium_possible()),
        };
        let when = shows_last_seen_button(user, viewer);
        let user_id = user.id;
        div()
            .flex()
            .items_center()
            .justify_center()
            .gap_1p5()
            .max_w_full()
            .children(rating.map(|rating| rating_pill(user_id, rating, cx)))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(quill::profile_cover::status_line(user)),
            )
            .when(when, |this| {
                this.child(
                    div()
                        .debug_selector(|| "profile-cover-when".to_string())
                        .child(
                            Button::new(("profile-cover-when", user_id as u64))
                                .label(tr(keys::STATUS_LASTSEEN_WHEN))
                                .xsmall()
                                .rounded(px(9.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_show_or_premium(ShowOrPremium::LastSeen, user_id, cx);
                                })),
                        ),
                )
            })
            .into_any_element()
    }

    /// The Saved Messages chat's info cover: its bookmark avatar and how
    /// many chats it holds (tdesktop `TopBar` with `_savedMessages`).
    pub(in crate::ui) fn saved_messages_cover(&self, cx: &mut Context<Self>) -> Div {
        let saved = self.session().map(|s| &s.threads.saved);
        let status = saved_chats_status(
            saved.map_or(0, |saved| saved.topic_count),
            saved.is_some_and(|saved| saved.topics_exhausted),
        );
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .p_4()
            .debug_selector(|| "saved-messages-cover".to_string())
            .child(saved_messages_avatar(COVER_SIZE))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_0p5()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .child(tr(keys::SAVED_MESSAGES)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    ),
            )
    }
}

/// tdesktop `Ui::StarsRating`: the level on the accent, or a warning for a
/// negative rating; a click explains it.
fn rating_pill(user_id: i64, rating: StarsRating, cx: &mut Context<QuillApp>) -> AnyElement {
    let label = trn(keys::BOOST_LEVEL, rating.level);
    let button = Button::new(("profile-rating", user_id as u64))
        .xsmall()
        .rounded(px(9.))
        .accessibility_label(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.profile_cover.rating_box = Some(user_id);
            this.profile_cover.rating_preview = false;
            cx.notify();
        }));
    let button = if rating.is_negative() {
        button.danger().icon(IconName::TriangleAlert)
    } else {
        button.primary().label(rating.level.to_string())
    };
    div()
        .flex_none()
        .debug_selector(|| "profile-rating".to_string())
        .child(button)
        .into_any_element()
}

/// `text` with its `**bold**` runs in semibold (tdesktop marked text).
fn bold_marked(text: &str) -> AnyElement {
    let (plain, bold) = super::screen_chooser::split_bold(text);
    let style = HighlightStyle {
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    StyledText::new(plain)
        .with_highlights(bold.into_iter().map(|range| (range, style)))
        .into_any_element()
}

/// A name put into a marked string: its own `**` cannot open a bold run.
fn unmarked(name: &str) -> String {
    name.replace("**", "*\u{2060}*")
}

crate::ui::shell::register_dialogs! {
    StarsRating => DialogSpec::new(
        3791,
        |app| app.profile_cover.rating_box.is_some(),
        QuillApp::build_rating_box,
    )
    .closes(|app, _, cx| app.close_rating_box(cx)),
    ShowOrPremium => DialogSpec::new(
        3792,
        |app| app.profile_cover.show_or_premium.is_some(),
        QuillApp::build_show_or_premium_box,
    ),
}
