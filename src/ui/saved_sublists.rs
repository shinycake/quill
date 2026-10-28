//! Saved Messages sublists and tags (tdesktop `Data::SavedMessages`,
//! `Info::Saved::SublistsWidget`, the message tag menu).
//!
//! With "View as Chats" on, Saved Messages lists the chats its messages were
//! saved from; a row opens that sublist's messages. A tags bar filters the
//! messages by tag (`searchSavedMessages`); a tag can be named with Premium.
#![deny(clippy::indexing_slicing)]

use super::app::{QuillApp, pane_placeholder};
use super::chat_row::{chat_avatar, saved_messages_avatar};
use super::pressable::PressableDiv;
use super::shell::{DialogKind, QuillShell};
use crate::ui::scroll_load::{load_due, loading_row, scrolled_near_end, watch_scroll};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::local_path::sandboxed_display_path;
use quill::state::{RequestPurpose, SAVED_TAG_LABEL_MAX, clean_tag_label};
use quill::telegram::envelope::{
    ReactionType, SavedMessagesTag, SavedMessagesTopic, SavedTopicKind,
};
use std::cell::RefCell;
use std::rc::Rc;

/// Which Saved Messages view the open chat shows instead of its history.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SavedMode {
    /// The list of sublists ("View as Chats").
    Sublists,
    /// The messages saved from one chat.
    Sublist,
    /// Messages carrying one tag.
    Tag,
}

/// "Add Name" / "Edit Name" for a Saved Messages tag.
pub(super) struct SavedTagDialog {
    pub(super) tag: ReactionType,
    pub(super) input: Entity<TextareaState>,
}

/// What a tag chip on a message needs for its menu.
pub(super) struct TagChipMenu {
    pub(super) owner: WeakEntity<QuillApp>,
    pub(super) tag: ReactionType,
    pub(super) choice: Option<quill::state::ReactionChoice>,
    pub(super) chat_id: ChatId,
    pub(super) message_id: quill::ids::MessageId,
    pub(super) named: bool,
    pub(super) premium: bool,
}

/// Telegram Desktop's `ShowTagMenu`: right-click a tag on a message for
/// Filter by Tag, Add or Edit Name, and Remove Tag.
pub(super) fn tag_chip_menu(chip: Stateful<Div>, menu: TagChipMenu) -> AnyElement {
    chip.context_menu(move |popup, _, _| {
        let filter_owner = menu.owner.clone();
        let name_owner = menu.owner.clone();
        let remove_owner = menu.owner.clone();
        let filter_tag = menu.tag.clone();
        let name_tag = menu.tag.clone();
        let choice = menu.choice.clone();
        let (chat_id, message_id) = (menu.chat_id, menu.message_id);
        popup
            .item(
                PopupMenuItem::new(tr(keys::Q_PEOPLE_FILTER_BY_TAG))
                    .icon(IconName::Tag)
                    .on_click(move |_, _, cx| {
                        let _ = filter_owner.update(cx, |this, cx| {
                            this.filter_saved_tag_ui(Some(filter_tag.clone()), cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(if menu.named {
                    tr(keys::Q_PEOPLE_EDIT_NAME)
                } else {
                    tr(keys::Q_PEOPLE_ADD_NAME)
                })
                .icon(IconName::Pencil)
                .disabled(!menu.premium)
                .on_click(move |_, window, cx| {
                    let _ = name_owner.update(cx, |this, cx| {
                        this.open_saved_tag_name(name_tag.clone(), window, cx);
                    });
                }),
            )
            .item(
                PopupMenuItem::new(tr(keys::Q_PEOPLE_REMOVE_TAG))
                    .icon(IconName::TagX)
                    .disabled(choice.is_none())
                    .on_click(move |_, _, cx| {
                        let Some(choice) = choice.clone() else {
                            return;
                        };
                        let _ = remove_owner.update(cx, |this, cx| {
                            this.toggle_reaction(chat_id, message_id, choice, cx);
                        });
                    }),
            )
    })
    .into_any_element()
}

/// The label a tag chip shows: the emoji, or a tag glyph for custom emoji.
fn tag_glyph(tag: &ReactionType) -> Option<String> {
    tag.emoji_text().map(str::to_owned)
}

impl QuillApp {
    /// The Saved Messages view of the open chat, if it is not the plain
    /// history.
    pub(super) fn saved_mode(&self) -> Option<SavedMode> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        if !session.is_saved_messages(chat_id) {
            return None;
        }
        if session.threads.saved.tag_search.is_some() {
            Some(SavedMode::Tag)
        } else if session.threads.saved.sublist.is_some() {
            Some(SavedMode::Sublist)
        } else if session.chat_views_as_topics(chat_id) {
            Some(SavedMode::Sublists)
        } else {
            None
        }
    }

    /// Title and subtitle the header shows for a sublist or tag filter.
    pub(super) fn saved_header(&self) -> Option<(String, String)> {
        let session = self.session()?;
        match self.saved_mode()? {
            SavedMode::Sublists => None,
            SavedMode::Sublist => {
                let view = session.threads.saved.sublist.as_ref()?;
                let title = session
                    .threads
                    .saved
                    .topics
                    .get(&view.topic_id)
                    .map_or_else(
                        || tr(keys::Q_PEOPLE_SAVED_CHAT).to_owned(),
                        |t| session.saved_topic_title(t),
                    );
                Some((title, count_line(view.history.messages.len() as i32)))
            }
            SavedMode::Tag => {
                let search = session.threads.saved.tag_search.as_ref()?;
                let label = tag_name(
                    &session.saved_tag_choices(),
                    &session.threads.saved.tags,
                    &search.tag,
                );
                Some((
                    tr(keys::SAVED_MESSAGES).to_owned(),
                    tr_with(
                        keys::Q_PEOPLE_TAG_3,
                        &[
                            ("label", &label.to_string()),
                            (
                                "total_count",
                                &(count_line(search.history.total_count)).to_string(),
                            ),
                        ],
                    ),
                ))
            }
        }
    }

    /// The history a sublist or tag view shows, with the sublist it is in
    /// (0: all of Saved Messages).
    pub(super) fn saved_history(&self) -> Option<(i64, &quill::state::TopicHistory)> {
        let saved = &self.session()?.threads.saved;
        match self.saved_mode()? {
            SavedMode::Sublists => None,
            SavedMode::Sublist => saved
                .sublist
                .as_ref()
                .map(|sublist| (sublist.topic_id, &sublist.history)),
            SavedMode::Tag => saved
                .tag_search
                .as_ref()
                .map(|search| (search.topic_id, &search.history)),
        }
    }

    /// Sublist and tag views are read-only: the composer goes away.
    pub(super) fn saved_readonly(&self) -> bool {
        self.saved_mode().is_some()
    }

    /// Back from a tag filter, then from a sublist.
    pub(super) fn saved_back_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("saved-back")
            .icon(IconName::ChevronLeft)
            .ghost()
            .tooltip(tr(keys::CREATE_GROUP_BACK))
            .accessibility_label(tr(keys::CREATE_GROUP_BACK))
            .on_click(cx.listener(|this, _, _, cx| this.saved_back(cx)))
    }

    pub(super) fn saved_back(&mut self, cx: &mut Context<Self>) {
        let tag = self
            .session()
            .is_some_and(|s| s.threads.saved.tag_search.is_some());
        if let Some(live) = self.live.as_mut() {
            if tag {
                live.driver.clear_saved_tag_filter();
            } else {
                live.driver.close_saved_sublist();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            if tag {
                session.clear_saved_tag_search();
            } else {
                session.close_saved_sublist();
            }
        }
        cx.notify();
    }

    /// "View as Chats" / "View as Messages" for the open chat.
    pub(super) fn set_chat_view_as_topics_ui(
        &mut self,
        chat_id: ChatId,
        view_as_topics: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .toggle_view_as_topics(chat_id, view_as_topics)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULD_NOT_CHANGE_THE_VIEW,
                )));
            } else if view_as_topics && live.driver.session.is_saved_messages(chat_id) {
                let _ = live.driver.load_saved_topics();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.set_chat_view_as_topics(chat_id.0, view_as_topics);
        }
        cx.notify();
    }

    pub(super) fn open_saved_sublist_ui(&mut self, topic_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.open_saved_sublist(topic_id).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULD_NOT_OPEN_THE_SAVED,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_saved_sublist(topic_id);
        }
        cx.notify();
    }

    fn pin_saved_sublist_ui(&mut self, topic_id: i64, pinned: bool, cx: &mut Context<Self>) {
        // tdesktop `PinnedLimitReached` for a sublist: the limit box, not
        // the request.
        if pinned && self.session().is_some_and(|s| s.saved_pin_limit_reached()) {
            self.show_premium_limit(quill::premium_limits::PremiumLimitKind::SavedPins, 0, cx);
            return;
        }
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .toggle_saved_topic_pinned(topic_id, pinned)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULD_NOT_PIN_THE_SAVED,
                )));
            }
        } else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_PINNING_NEEDS_A_LIVE_CONNECTION,
            )));
        }
        cx.notify();
    }

    pub(super) fn filter_saved_tag_ui(
        &mut self,
        tag: Option<ReactionType>,
        cx: &mut Context<Self>,
    ) {
        match (self.live.as_mut(), tag) {
            (Some(live), Some(tag)) => {
                if live.driver.filter_saved_by_tag(tag).is_err() {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_PEOPLE_COULD_NOT_FILTER_BY_TAG,
                    )));
                }
            }
            (Some(live), None) => live.driver.clear_saved_tag_filter(),
            (None, Some(tag)) => {
                if let Some(session) = self.demo_session.as_mut() {
                    let topic = session
                        .threads
                        .saved
                        .sublist
                        .as_ref()
                        .map_or(0, |v| v.topic_id);
                    session.begin_saved_tag_search(topic, tag);
                }
            }
            (None, None) => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.clear_saved_tag_search();
                }
            }
        }
        cx.notify();
    }

    /// Keep the sublist list and the tags loaded while Saved Messages is
    /// open: the first `loadSavedMessagesTopics` when the list shows, the
    /// global tags once.
    pub(super) fn pump_saved_messages(&mut self) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(chat_id) = live.driver.session.open_chat else {
            return;
        };
        if !live.driver.session.is_saved_messages(chat_id) {
            return;
        }
        let session = &live.driver.session;
        if !session.threads.saved.tags_loaded
            && !session
                .requests
                .has_purpose(quill::state::RequestPurpose::GetSavedMessagesTags { topic_id: 0 })
        {
            let _ = live.driver.load_saved_tags(0);
        }
        let session = &live.driver.session;
        if session.chat_views_as_topics(chat_id)
            && !session.threads.saved.topics_requested
            && !session.threads.saved.topics_exhausted
        {
            let _ = live.driver.load_saved_topics();
        }
    }

    // ---- sublist list ----------------------------------------------------

    /// The list of sublists, shown instead of the history.
    pub(super) fn saved_sublists_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let topics = session.threads.saved.ordered_topics();
        if topics.is_empty() {
            let loading = !session.threads.saved.topics_exhausted;
            return pane_placeholder(
                if loading {
                    tr(keys::Q_PEOPLE_LOADING_SAVED_CHATS)
                } else {
                    tr(keys::Q_PEOPLE_NO_SAVED_CHATS_YET)
                },
                if loading {
                    tr(keys::Q_PEOPLE_YOUR_SAVED_MESSAGES_ARE_BEING)
                } else {
                    tr(keys::NO_SAVED_SUBLISTS)
                },
                cx,
            )
            .into_any_element();
        }
        let roots = self.media_display_roots();
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        let mut list = div()
            .id("saved-sublists")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.dialogs.saved_scroll)
            .p_2()
            .gap_0p5()
            .role(Role::List)
            .aria_label(tr(keys::Q_PEOPLE_SAVED_CHATS));
        for topic in topics {
            let title = session.saved_topic_title(topic);
            let photo = match topic.kind {
                SavedTopicKind::FromChat(chat) => session
                    .chat_photo_path(ChatId(chat))
                    .and_then(|path| sandboxed_display_path(path, &roots)),
                _ => None,
            };
            list = list.child(self.saved_sublist_row(topic, title, photo, &now, cx));
        }
        if Self::saved_topics_in_flight(session) {
            list = list.child(loading_row("saved-sublists-loading", cx));
        }
        list.on_scroll(watch_scroll(cx, Self::saved_topics_more))
            .into_any_element()
    }

    fn saved_topics_in_flight(session: &quill::state::Session) -> bool {
        session
            .requests
            .has_purpose(RequestPurpose::LoadSavedMessagesTopics)
    }

    /// The next page of sublists, once the list is scrolled near its end or
    /// does not fill the pane.
    fn saved_topics_more(&mut self, cx: &mut Context<Self>, scrolled: bool) {
        // Paging needs a live connection; a demo session has nothing to ask.
        if self.live.is_none() {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        let saved = &session.threads.saved;
        let loaded = saved.topics.len();
        let has_more = !saved.topics_exhausted
            && (saved.topic_count == 0 || loaded < saved.topic_count as usize);
        if !load_due(
            scrolled_near_end(&self.dialogs.saved_scroll),
            has_more,
            Self::saved_topics_in_flight(session),
        ) || !self.dialogs.saved_asked.allow(loaded, scrolled)
        {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_saved_topics();
        }
        cx.notify();
    }

    fn saved_sublist_row(
        &self,
        topic: &SavedMessagesTopic,
        title: String,
        photo: Option<std::path::PathBuf>,
        now: &quill::local_time::CivilTime,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = topic.id;
        let pinned = topic.is_pinned;
        let avatar = match topic.kind {
            SavedTopicKind::MyNotes => saved_messages_avatar(46.),
            _ => chat_avatar(&title, photo.as_deref(), 46.).into_any_element(),
        };
        let stamp = (topic.last_message_date > 0).then(|| {
            quill::local_time::chat_list_stamp(
                &quill::local_time::civil_local(i64::from(topic.last_message_date)),
                now,
            )
        });
        let preview = if topic.last_message_preview.is_empty() {
            tr(keys::Q_PEOPLE_NO_MESSAGES).to_owned()
        } else {
            topic.last_message_preview.clone()
        };
        let muted = cx.theme().muted_foreground;
        let owner = cx.entity().downgrade();
        let row_title = title.clone();
        div()
            .id(("saved-sublist", id as u64))
            .w_full()
            .px_2()
            .h(px(64.))
            .flex()
            .items_center()
            .gap_3()
            .rounded_md()
            .bg(cx.theme().sidebar)
            .pressable(cx.theme())
            .role(Role::Button)
            .aria_label(format!("{row_title}, {preview}"))
            .tab_index(0)
            .on_click(cx.listener(move |this, _, _, cx| this.open_saved_sublist_ui(id, cx)))
            .child(avatar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_semibold()
                                    .truncate()
                                    .child(super::bidi_line::one_line_plain(title)),
                            )
                            .when(pinned, |this| {
                                this.child(Icon::new(IconName::Pin).size(px(14.)).text_color(muted))
                            })
                            .when_some(stamp, |this, stamp| {
                                this.child(div().text_xs().text_color(muted).child(stamp))
                            }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .min_w_0()
                            .truncate()
                            .child(super::bidi_line::one_line_plain(
                                super::search_ui::one_line_preview(&preview),
                            )),
                    ),
            )
            .context_menu(move |menu, _, _| {
                let pin_owner = owner.clone();
                let delete_owner = owner.clone();
                menu.item(
                    PopupMenuItem::new(if pinned {
                        tr(keys::Q_PEOPLE_UNPIN)
                    } else {
                        tr(keys::CONTEXT_PIN_MSG)
                    })
                    .icon(if pinned {
                        IconName::PinOff
                    } else {
                        IconName::Pin
                    })
                    .on_click(move |_, _, cx| {
                        let _ = pin_owner.update(cx, |this, cx| {
                            this.pin_saved_sublist_ui(id, !pinned, cx);
                        });
                    }),
                )
                .separator()
                .item(
                    PopupMenuItem::new(tr(keys::Q_PEOPLE_DELETE_CHAT))
                        .icon(IconName::Trash)
                        .on_click(move |_, _, cx| {
                            let _ = delete_owner.update(cx, |this, cx| {
                                this.confirm_delete_saved_sublist(id, cx);
                            });
                        }),
                )
            })
            .into_any_element()
    }

    fn confirm_delete_saved_sublist(&mut self, topic_id: i64, cx: &mut Context<Self>) {
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return;
        };
        self.open_group_confirm(
            chat_id,
            super::GroupConfirmAction::DeleteSavedSublist { topic_id },
            cx,
        );
    }

    // ---- tags bar --------------------------------------------------------

    /// The tags bar above the messages: "All" plus one chip per tag.
    pub(super) fn saved_tags_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.is_saved_messages(session.open_chat?) {
            return None;
        }
        self.saved_tags_bar_body(cx)
    }

    /// `saved_tags_bar` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn saved_tags_bar_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        if !session.is_saved_messages(chat_id) || self.saved_mode() == Some(SavedMode::Sublists) {
            return None;
        }
        let tags = session.saved_tag_choices();
        if tags.is_empty() {
            return None;
        }
        let active = session
            .threads
            .saved
            .tag_search
            .as_ref()
            .map(|s| s.tag.clone());
        let premium = session
            .my_user_id
            .and_then(|id| session.user(id))
            .is_some_and(|u| u.is_premium);
        let mut row = div()
            .id("saved-tags-scroll")
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_1()
            .overflow_x_scroll()
            .child(
                Button::new("saved-tag-all")
                    .label(tr(keys::Q_PEOPLE_ALL))
                    .ghost()
                    .selected(active.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.filter_saved_tag_ui(None, cx))),
            );
        // tdesktop `SearchTags`: a free account gets "Unlock" first, and
        // every tag opens the tags preview instead of filtering.
        if !premium {
            row = row.child(
                Button::new("saved-tag-unlock")
                    .icon(IconName::Lock)
                    .label(tr(keys::UNLOCK_TAGS))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.show_saved_tags_preview(cx))),
            );
        }
        for (ix, entry) in tags.into_iter().enumerate() {
            row = row.child(self.saved_tag_chip(ix, entry, active.as_ref(), premium, cx));
        }
        Some(
            div()
                .id("saved-tags-bar")
                .flex()
                .flex_none()
                .items_center()
                .w_full()
                .h(px(40.))
                .px_2()
                .bg(cx.theme().background)
                .border_b_1()
                .border_color(cx.theme().border)
                .role(Role::TabList)
                .aria_label(tr(keys::Q_PEOPLE_TAGS))
                .child(row)
                .into_any_element(),
        )
    }

    /// tdesktop `ShowPremiumPreviewBox(PremiumFeature::TagsForMessages)`.
    fn show_saved_tags_preview(&mut self, cx: &mut Context<Self>) {
        self.open_premium_at(quill::premium_hub::PremiumFeature::SavedMessagesTags, cx);
    }

    fn saved_tag_chip(
        &self,
        ix: usize,
        entry: SavedMessagesTag,
        active: Option<&ReactionType>,
        premium: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = active == Some(&entry.tag);
        let glyph = tag_glyph(&entry.tag);
        let name = if entry.label.is_empty() {
            entry.count.to_string()
        } else {
            entry.label.clone()
        };
        let named = !entry.label.is_empty();
        let tag = entry.tag.clone();
        let filter_tag = entry.tag.clone();
        let edit_tag = entry.tag.clone();
        let owner = cx.entity().downgrade();
        let mut button = Button::new(("saved-tag", ix))
            .ghost()
            .selected(selected)
            .accessibility_label(tr_with(keys::Q_PEOPLE_TAG, &[("name", &name.to_string())]));
        button = match glyph {
            Some(glyph) => button.label(format!("{glyph} {name}")),
            None => button.icon(IconName::Tag).label(name),
        };
        div()
            .id(("saved-tag-chip", ix))
            .flex_none()
            .child(button.on_click(cx.listener(move |this, _, _, cx| {
                if premium {
                    this.filter_saved_tag_ui(Some(tag.clone()), cx);
                } else {
                    this.show_saved_tags_preview(cx);
                }
            })))
            .context_menu(move |menu, _, _| {
                let filter_owner = owner.clone();
                let edit_owner = owner.clone();
                let filter_tag = filter_tag.clone();
                let edit_tag = edit_tag.clone();
                menu.item(
                    PopupMenuItem::new(tr(keys::Q_PEOPLE_FILTER_BY_TAG))
                        .icon(IconName::Tag)
                        .on_click(move |_, _, cx| {
                            let _ = filter_owner.update(cx, |this, cx| {
                                this.filter_saved_tag_ui(Some(filter_tag.clone()), cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new(if named {
                        tr(keys::Q_PEOPLE_EDIT_NAME)
                    } else {
                        tr(keys::Q_PEOPLE_ADD_NAME)
                    })
                    .icon(IconName::Pencil)
                    .disabled(!premium)
                    .on_click(move |_, window, cx| {
                        let _ = edit_owner.update(cx, |this, cx| {
                            this.open_saved_tag_name(edit_tag.clone(), window, cx);
                        });
                    }),
                )
            })
            .into_any_element()
    }

    // ---- tag name dialog -------------------------------------------------

    pub(super) fn open_saved_tag_name(
        &mut self,
        tag: ReactionType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session()
            .and_then(|s| {
                s.threads
                    .saved
                    .tags
                    .iter()
                    .find(|entry| entry.tag == tag)
                    .map(|entry| entry.label.clone())
            })
            .unwrap_or_default();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_PEOPLE_NAME))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        input.update(cx, |input, cx| input.set_value(current, window, cx));
        self.dialogs.saved_tag_dialog = Some(SavedTagDialog { tag, input });
        cx.notify();
    }

    fn close_saved_tag_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialogs.saved_tag_dialog = None;
        cx.notify();
    }

    fn submit_saved_tag_name(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.saved_tag_dialog.as_ref() else {
            return;
        };
        let tag = dialog.tag.clone();
        let label = clean_tag_label(dialog.input.read(cx).value().as_ref());
        match self.live.as_mut() {
            Some(live) => {
                self.connection
                    .set_note(match live.driver.set_saved_tag_label(&tag, &label) {
                        Ok(_) => StatusNote::silent(tr(keys::Q_PEOPLE_TAG_NAME_SAVED)),
                        Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_RENAME_THE_TAG)),
                    });
            }
            None => self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_NAMING_TAGS_NEEDS_A_LIVE,
            ))),
        }
        self.dialogs.saved_tag_dialog = None;
        cx.notify();
    }

    pub(super) fn build_saved_tag_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SavedTagName, |this, _, cx| {
                this.close_saved_tag_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.dialogs.saved_tag_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let premium = this
                .session()
                .and_then(|s| s.my_user_id.and_then(|id| s.user(id)))
                .is_some_and(|u| u.is_premium);
            let named = this.session().is_some_and(|s| {
                s.threads
                    .saved
                    .tags
                    .iter()
                    .any(|entry| entry.tag == state.tag && !entry.label.is_empty())
            });
            let glyph =
                tag_glyph(&state.tag).unwrap_or_else(|| tr(keys::Q_PEOPLE_TAG_2).to_owned());
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr_with(
                            keys::Q_LEFTOVERS_SAVED_TAG_LABEL_ABOUT,
                            &[
                                ("tag", &glyph.to_string()),
                                ("max", &SAVED_TAG_LABEL_MAX.to_string()),
                            ],
                        )),
                )
                .child(
                    Textarea::new(&state.input)
                        .aria_label(tr(keys::Q_PEOPLE_TAG_NAME))
                        .h(px(36.)),
                )
                .when(!premium, |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PEOPLE_NAMING_TAGS_NEEDS_TELEGRAM_PREMIUM)),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("saved-tag-cancel")
                                .label(tr(keys::CANCEL))
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_saved_tag_dialog(cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::SavedTagName,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("saved-tag-save")
                                .label(tr(keys::SAVED_SHORT))
                                .primary()
                                .disabled(!premium)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit_saved_tag_name(cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::SavedTagName,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(if named {
                    tr(keys::Q_PEOPLE_EDIT_NAME)
                } else {
                    tr(keys::Q_PEOPLE_ADD_NAME)
                }))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

fn count_line(count: i32) -> String {
    trn(keys::FORUM_MESSAGES, count)
}

/// The tag's name for the header: its label, else the emoji.
fn tag_name(choices: &[SavedMessagesTag], all: &[SavedMessagesTag], tag: &ReactionType) -> String {
    let label = choices
        .iter()
        .chain(all.iter())
        .find(|entry| entry.tag == *tag && !entry.label.is_empty())
        .map(|entry| entry.label.clone());
    match (tag_glyph(tag), label) {
        (Some(glyph), Some(label)) => format!("{glyph} {label}"),
        (Some(glyph), None) => glyph,
        (None, Some(label)) => label,
        (None, None) => "tag".to_owned(),
    }
}

crate::ui::shell::register_dialogs! {
    SavedTagName => DialogSpec::new(
        5300,
        |app| app.dialogs.saved_tag_dialog.is_some(),
        QuillApp::build_saved_tag_dialog,
    ),
}

#[cfg(test)]
mod tests {
    use super::{count_line, tag_name};
    use quill::telegram::envelope::{ReactionType, SavedMessagesTag};

    #[test]
    fn count_lines_pluralise() {
        assert_eq!(count_line(1), "1 message");
        assert_eq!(count_line(0), "0 messages");
        assert_eq!(count_line(12), "12 messages");
    }

    #[test]
    fn tag_names_prefer_the_label() {
        let heart = ReactionType::emoji("\u{2764}");
        let named = SavedMessagesTag {
            tag: heart.clone(),
            label: "Love".into(),
            count: 2,
        };
        assert_eq!(
            tag_name(&[], std::slice::from_ref(&named), &heart),
            "\u{2764} Love"
        );
        assert_eq!(tag_name(&[], &[], &heart), "\u{2764}");
        assert_eq!(
            tag_name(&[], &[], &ReactionType::CustomEmoji { custom_emoji_id: 5 }),
            "tag"
        );
    }
}
