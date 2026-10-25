//! The story viewer's controls after tdesktop's `media/stories`
//! (`media_stories_header.cpp`, `media_stories_controller.cpp`,
//! `media_stories_recent_views.cpp`): icon buttons in the header (sound,
//! pause, the ⋯ menu, close), the reply field with the heart and share
//! icons under someone else's story, and the recent-views row under your
//! own. Everything else the old row of text buttons held is a ⋯ menu entry
//! with the same gates.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::scroll_load::scrolled_near_end;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonCustomVariant;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use quill::lang::{keys, tr};
use quill::story_extras::{can_save_story, can_share_story, hide_label, profile_label};

/// Icon size inside the header and bottom buttons.
const ICON: f32 = 18.;
/// Square size of a chrome icon button.
const BUTTON: f32 = 32.;

/// What a ⋯ menu entry does. Mirrors the old text buttons one to one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuAct {
    Stories,
    React,
    Save,
    CopyLink,
    Edit,
    Cover,
    Privacy,
    Repost,
    CloseFriends,
    Profile,
    Hide,
    Stealth,
    SearchHere,
    Statistics,
    Report,
    Delete,
}

pub(super) struct MenuEntry {
    pub(super) act: MenuAct,
    label: String,
    icon: IconName,
    disabled: bool,
}

fn icon_button(
    id: &'static str,
    icon: IconName,
    label: &str,
    variant: ButtonCustomVariant,
) -> Button {
    let label = SharedString::from(label.to_string());
    Button::new(id)
        .custom(variant)
        .icon(Icon::new(icon).with_size(px(ICON)))
        .size(px(BUTTON))
        .rounded(px(BUTTON / 2.))
        .tooltip(label.clone())
        .accessibility_label(label)
}

impl QuillApp {
    /// The icons at the header's right edge. A watched live stream keeps
    /// its own controls and gets only the close icon.
    pub(super) fn story_header_buttons(
        &self,
        live_watch: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div().flex().flex_shrink_0().items_center().gap_1();
        if !live_watch {
            let is_video = self
                .stories
                .viewer
                .current()
                .is_some_and(|item| matches!(item.kind, StoryViewerKind::Video));
            if is_video {
                let (icon, label) = if self.stories.muted {
                    (IconName::VolumeX, tr(keys::Q_MEDIA_SOUND_OFF))
                } else {
                    (IconName::Volume2, tr(keys::Q_MEDIA_SOUND_ON))
                };
                row = row.child(
                    icon_button("story-mute", icon, label, chrome::ghost(cx))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_story_mute(cx))),
                );
            }
            let (icon, label) = if self.stories.pause.is_paused() {
                (IconName::Play, tr(keys::Q_MEDIA_RESUME))
            } else {
                (IconName::Pause, tr(keys::Q_MEDIA_PAUSE))
            };
            row = row.child(
                icon_button("story-pause", icon, label, chrome::ghost(cx))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_story_pause(cx))),
            );
            let entries = self.story_viewer_menu_entries();
            if !entries.is_empty() {
                let owner = cx.entity().downgrade();
                row = row.child(
                    icon_button(
                        "story-more",
                        IconName::Ellipsis,
                        tr(keys::PROFILE_ACTION_SHORT_MORE),
                        chrome::ghost(cx),
                    )
                    .dropdown_menu(move |mut menu, _, _| {
                        for entry in &entries {
                            let (owner, act) = (owner.clone(), entry.act);
                            if matches!(act, MenuAct::Report | MenuAct::Delete) {
                                menu = menu.separator();
                            }
                            menu = menu.item(
                                PopupMenuItem::new(entry.label.clone())
                                    .icon(entry.icon)
                                    .disabled(entry.disabled)
                                    .on_click(move |_, window, cx| {
                                        let _ = owner.update(cx, |this, cx| {
                                            this.run_story_menu(act, window, cx)
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
                );
            }
        }
        row.child(
            icon_button(
                "story-viewer-close",
                IconName::Close,
                tr(keys::Q_MEDIA_CLOSE_STORY_VIEWER),
                chrome::ghost(cx),
            )
            .on_click(cx.listener(|this, _, _, cx| this.close_story_viewer(cx))),
        )
        .into_any_element()
    }

    /// The ⋯ menu's entries for the current story, gated as the text
    /// buttons were: TDLib's `can_*` flags for what the story allows.
    pub(super) fn story_viewer_menu_entries(&self) -> Vec<MenuEntry> {
        let Some(item) = self.stories.viewer.current() else {
            return Vec::new();
        };
        let Some(story) = self.current_story() else {
            return Vec::new();
        };
        let own = story.can_be_deleted;
        let busy = self.session().is_some_and(|s| s.stories.manage.pending);
        let is_video = matches!(item.kind, StoryViewerKind::Video);
        let hidden = self.story_chat_hidden(item.chat_id.0);
        let mut out = Vec::new();
        let mut add = |act, label: &str, icon, disabled| {
            out.push(MenuEntry {
                act,
                label: label.to_string(),
                icon,
                disabled,
            })
        };
        add(
            MenuAct::Stories,
            tr(keys::Q_MEDIA_STORIES),
            IconName::Images,
            false,
        );
        if can_save_story(&story) {
            add(
                MenuAct::Save,
                tr(keys::SETTINGS_SAVE),
                IconName::Download,
                false,
            );
        }
        add(
            MenuAct::CopyLink,
            tr(keys::CONTEXT_COPY_LINK),
            IconName::Copy,
            false,
        );
        add(
            MenuAct::React,
            tr(keys::Q_MEDIA_REACT),
            IconName::FaceSlightlySmilingPlus,
            false,
        );
        if story.can_be_edited {
            add(
                MenuAct::Edit,
                tr(keys::Q_MEDIA_EDIT),
                IconName::Settings2,
                busy,
            );
            if is_video {
                add(
                    MenuAct::Cover,
                    tr(keys::Q_MEDIA_COVER),
                    IconName::Crop,
                    busy,
                );
            }
        }
        if story.can_set_privacy_settings {
            add(
                MenuAct::Privacy,
                tr(keys::Q_MEDIA_PRIVACY),
                IconName::Lock,
                busy,
            );
        }
        if story.can_be_forwarded {
            add(
                MenuAct::Repost,
                tr(keys::Q_MEDIA_REPOST),
                IconName::Redo2,
                busy,
            );
        }
        if own {
            add(
                MenuAct::CloseFriends,
                tr(keys::Q_MEDIA_CLOSE_FRIENDS),
                IconName::Star,
                false,
            );
        }
        if story.can_toggle_is_posted_to_chat_page {
            add(
                MenuAct::Profile,
                profile_label(story.is_posted_to_chat_page),
                IconName::User,
                false,
            );
        }
        if !own {
            add(MenuAct::Hide, hide_label(hidden), IconName::EyeOff, false);
        }
        if let Some(entry) = self.story_stealth_entry(item) {
            add(
                MenuAct::Stealth,
                tr(keys::STEALTH_MODE_MENU_ITEM),
                if entry.locked {
                    IconName::Lock
                } else {
                    IconName::HatGlasses
                },
                false,
            );
        }
        if self.viewer_story_search_query().is_some() {
            add(
                MenuAct::SearchHere,
                tr(keys::Q_MEDIA_STORIES_HERE),
                IconName::Search,
                false,
            );
        }
        if story.can_get_statistics {
            add(
                MenuAct::Statistics,
                tr(keys::Q_MEDIA_STATISTICS),
                IconName::ChartColumn,
                false,
            );
        }
        if !own {
            add(
                MenuAct::Report,
                tr(keys::Q_MEDIA_REPORT),
                IconName::Flag,
                false,
            );
        }
        if own {
            add(
                MenuAct::Delete,
                tr(keys::BOX_DELETE),
                IconName::Delete,
                false,
            );
        }
        out
    }

    fn run_story_menu(&mut self, act: MenuAct, window: &mut Window, cx: &mut Context<Self>) {
        let item = self.stories.viewer.current().cloned();
        match act {
            MenuAct::Stories => {
                if let Some(item) = item {
                    self.close_story_viewer(cx);
                    self.open_story_page(item.chat_id, window, cx);
                }
            }
            MenuAct::React => self.toggle_story_reaction_picker(cx),
            MenuAct::Save => self.save_story_media(cx),
            MenuAct::CopyLink => self.copy_story_link(cx),
            MenuAct::Edit => {
                if let Some(item) = item {
                    self.open_story_edit(item.chat_id.0, item.story_id, window, cx);
                }
            }
            MenuAct::Cover => {
                if let Some(item) = item {
                    self.toggle_story_cover_edit(item.chat_id.0, item.story_id, cx);
                }
            }
            MenuAct::Privacy => self.toggle_story_privacy_edit(window, cx),
            MenuAct::Repost => {
                if let Some(item) = item {
                    self.open_story_repost(item.chat_id.0, item.story_id, window, cx);
                }
            }
            MenuAct::CloseFriends => self.open_close_friends_editor(window, cx),
            MenuAct::Profile => self.toggle_story_profile(cx),
            MenuAct::Hide => self.toggle_hide_story_peer(cx),
            MenuAct::Stealth => self.setup_story_stealth(None, cx),
            MenuAct::SearchHere => {
                if let Some(query) = self.viewer_story_search_query() {
                    self.search_stories_from_viewer(query, window, cx);
                }
            }
            MenuAct::Statistics => self.toggle_story_stats(cx),
            MenuAct::Report => self.toggle_story_report(cx),
            MenuAct::Delete => self.delete_story_viewer(cx),
        }
    }

    /// tdesktop's reply placeholder: "Comment story..." in a group,
    /// "Reply privately..." everywhere else (`media_stories_reply.cpp`).
    fn story_reply_placeholder(&self) -> &'static str {
        let group = self
            .stories
            .viewer
            .current()
            .and_then(|item| self.session()?.chats.get(&item.chat_id.0))
            .is_some_and(|chat| {
                matches!(
                    chat.kind,
                    quill::telegram::envelope::ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
                )
            });
        if group {
            tr(keys::STORY_COMMENT_PH)
        } else {
            tr(keys::STORY_REPLY_PH)
        }
    }

    /// The chevron at the story's left or right edge that steps to the
    /// previous or next story; absent when there is nowhere to go.
    pub(super) fn story_step_button(
        &self,
        delta: i32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.story_step_available(delta) {
            return None;
        }
        let (id, icon, label) = if delta < 0 {
            (
                "story-viewer-prev",
                IconName::ChevronLeft,
                tr(keys::Q_MEDIA_PREV),
            )
        } else {
            (
                "story-viewer-next",
                IconName::ChevronRight,
                tr(keys::Q_MEDIA_NEXT),
            )
        };
        let edge = div().absolute().top(relative(0.5)).mt(px(-BUTTON / 2.));
        let edge = if delta < 0 {
            edge.left(px(6.))
        } else {
            edge.right(px(6.))
        };
        Some(
            edge.child(
                icon_button(id, icon, label, chrome::over_media(cx))
                    .on_click(cx.listener(move |this, _, _, cx| this.step_story_viewer(delta, cx))),
            )
            .into_any_element(),
        )
    }

    /// The row under the story: the reply field with the heart and share
    /// icons for someone else's story, the recent views (eye and count,
    /// reactions, forwards) with share for your own.
    pub(super) fn story_bottom_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let story = self.current_story();
        let can_reply = story.as_ref().is_some_and(|s| s.can_be_replied);
        let can_share = story.as_ref().is_some_and(can_share_story);
        let interactions = story.as_ref().is_some_and(|s| s.can_get_interactions);
        let chosen = story.as_ref().and_then(|s| s.chosen_reaction_emoji.clone());
        let mut row = div().flex().w(px(360.)).items_center().gap_1();
        let placeholder = self.story_reply_placeholder();
        if interactions {
            let info = story.as_ref().and_then(|s| s.interaction_info);
            let views = info.map_or(0, |i| i.view_count);
            let label = if views > 0 {
                views.to_string()
            } else {
                tr(keys::Q_MEDIA_VIEWERS).to_string()
            };
            row = row.child(
                Button::new("story-viewers")
                    .custom(chrome::ghost(cx))
                    .icon(Icon::new(IconName::Eye).with_size(px(ICON)))
                    .label(label)
                    .selected(self.stories.viewers_open)
                    .tooltip(tr(keys::Q_MEDIA_VIEWERS))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_story_viewers(cx))),
            );
            let stat = |id: &'static str, icon, count: i32| {
                (count > 0).then(|| {
                    h_flex()
                        .id(id)
                        .gap_1()
                        .px_2()
                        .items_center()
                        .text_sm()
                        .text_color(chrome::secondary())
                        .child(Icon::new(icon).with_size(px(ICON - 2.)))
                        .child(count.to_string())
                })
            };
            row = row
                .children(stat(
                    "story-reaction-count",
                    IconName::Heart,
                    info.map_or(0, |i| i.reaction_count),
                ))
                .children(stat(
                    "story-forward-count",
                    IconName::Forward,
                    info.map_or(0, |i| i.forward_count),
                ))
                .child(div().flex_1());
        } else {
            if can_reply && !self.stories.reply_open {
                row = row.child(
                    div()
                        .id("story-reply")
                        .flex_1()
                        .min_w_0()
                        .h(px(36.))
                        .px_4()
                        .flex()
                        .items_center()
                        .rounded(px(18.))
                        .bg(chrome::compose_bg())
                        .text_color(chrome::gray_text())
                        .cursor_text()
                        .role(gpui_kit::Role::Button)
                        .aria_label(tr(keys::Q_MEDIA_REPLY_TO_STORY))
                        .tab_index(0)
                        .hover(|style| style.bg(chrome::compose_bg_over()))
                        .truncate()
                        .child(placeholder)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.stories.reply_input.update(cx, |input, cx| {
                                input.set_placeholder(placeholder, window, cx)
                            });
                            this.toggle_story_reply(cx);
                        })),
                );
            } else {
                row = row.child(div().flex_1());
            }
            let liked = chosen.as_deref() == Some("❤");
            let heart = if liked {
                chrome::ghost(cx).foreground(chrome::heart())
            } else {
                chrome::ghost(cx)
            };
            row = row.child(
                div()
                    .id("story-quick-react-wrap")
                    // Right-click opens the picker, where tdesktop shows its
                    // reaction strip on hover.
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| this.toggle_story_reaction_picker(cx)),
                    )
                    .child(
                        icon_button(
                            "story-quick-react",
                            IconName::Heart,
                            tr(keys::Q_MEDIA_REACT),
                            heart,
                        )
                        .selected(liked)
                        .on_click(cx.listener(|this, _, _, cx| this.quick_react_story(cx))),
                    ),
            );
            if let Some(extra) = story.as_ref().and_then(|s| s.chosen_reaction_extra) {
                row = row.child(self.story_chosen_extra_badge(extra));
            }
        }
        if can_share {
            row = row.child(
                icon_button(
                    "story-share",
                    IconName::Share2,
                    tr(keys::Q_MEDIA_SHARE),
                    chrome::ghost(cx),
                )
                .on_click(cx.listener(|this, _, window, cx| this.toggle_story_share(window, cx))),
            );
        }
        let mut column = div().flex().flex_col().gap_2().items_center();
        if self.stories.reply_open && !interactions {
            column = column.child(self.story_reply_row(cx));
        }
        column.child(row).into_any_element()
    }

    /// The viewer's chosen custom-emoji or paid reaction, beside the heart.
    fn story_chosen_extra_badge(&self, extra: StoryChosenExtraReaction) -> AnyElement {
        match extra {
            StoryChosenExtraReaction::CustomEmoji(id) => match self.story_custom_emoji_path(id) {
                Some(path) => img(path)
                    .id(("story-chosen-custom-emoji", id as u64))
                    .w(px(28.))
                    .h(px(28.))
                    .aspect_ratio(px(28.) / px(28.))
                    .object_fit(ObjectFit::Contain)
                    .with_fallback(|| div().text_xl().child("✨").into_any_element())
                    .into_any_element(),
                None => div().text_xl().child("✨").into_any_element(),
            },
            StoryChosenExtraReaction::Paid => div().text_xl().child("⭐").into_any_element(),
        }
    }

    /// Load the next viewers page when the list is scrolled near its end
    /// (live only; the demo has no further pages to fetch).
    pub(in crate::ui) fn story_viewers_scrolled(&mut self, cx: &mut Context<Self>) {
        if !self.stories.viewers_open || self.live.is_none() {
            return;
        }
        let more = self
            .session()
            .and_then(|s| s.stories.viewers.as_ref())
            .is_some_and(|v| !v.loading && !v.next_offset.is_empty());
        if more && scrolled_near_end(&self.stories.viewers_scroll) {
            self.load_more_story_viewers(cx);
        }
    }
}
