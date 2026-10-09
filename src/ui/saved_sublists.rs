//! Saved Messages sublists and tags (tdesktop `Data::SavedMessages`,
//! `Info::Saved::SublistsWidget`, the message tag menu).
//!
//! With "View as Chats" on, Saved Messages lists the chats its messages were
//! saved from; a row opens that sublist's messages. A tags bar filters the
//! messages by tag (`searchSavedMessages`); a tag can be named with Premium.

use super::app::{QuillApp, pane_placeholder};
use super::chat_row::{chat_avatar, saved_messages_avatar};
use super::pressable::PressableDiv;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::local_path::sandboxed_display_path;
use quill::state::{HistoryMessage, SAVED_TAG_LABEL_MAX, clean_tag_label};
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
        if session.saved.tag_search.is_some() {
            Some(SavedMode::Tag)
        } else if session.saved.sublist.is_some() {
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
                let view = session.saved.sublist.as_ref()?;
                let title = session
                    .saved
                    .topics
                    .get(&view.topic_id)
                    .map_or_else(|| "Saved chat".to_owned(), |t| session.saved_topic_title(t));
                Some((title, count_line(view.history.messages.len() as i32)))
            }
            SavedMode::Tag => {
                let search = session.saved.tag_search.as_ref()?;
                let label = tag_name(
                    &session.saved_tag_choices(),
                    &session.saved.tags,
                    &search.tag,
                );
                Some((
                    "Saved Messages".to_owned(),
                    format!("Tag {label} · {}", count_line(search.history.total_count)),
                ))
            }
        }
    }

    /// The rows of a sublist or tag view.
    pub(super) fn saved_rows(&self) -> Option<Vec<HistoryMessage>> {
        let session = self.session()?;
        match self.saved_mode()? {
            SavedMode::Sublists => None,
            SavedMode::Sublist => Some(
                session
                    .saved
                    .sublist
                    .as_ref()?
                    .history
                    .ordered()
                    .into_iter()
                    .cloned()
                    .collect(),
            ),
            SavedMode::Tag => Some(
                session
                    .saved
                    .tag_search
                    .as_ref()?
                    .history
                    .ordered()
                    .into_iter()
                    .cloned()
                    .collect(),
            ),
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
            .tooltip("Back")
            .accessibility_label("Back")
            .on_click(cx.listener(|this, _, _, cx| this.saved_back(cx)))
    }

    pub(super) fn saved_back(&mut self, cx: &mut Context<Self>) {
        let tag = self.session().is_some_and(|s| s.saved.tag_search.is_some());
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
                self.status_note = "could not change the view".into();
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
                self.status_note = "could not open the saved chat".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_saved_sublist(topic_id);
        }
        cx.notify();
    }

    fn pin_saved_sublist_ui(&mut self, topic_id: i64, pinned: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .toggle_saved_topic_pinned(topic_id, pinned)
                .is_err()
            {
                self.status_note = "could not pin the saved chat".into();
            }
        } else {
            self.status_note = "pinning needs a live connection (demo)".into();
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
                    self.status_note = "could not filter by tag".into();
                }
            }
            (Some(live), None) => live.driver.clear_saved_tag_filter(),
            (None, Some(tag)) => {
                if let Some(session) = self.demo_session.as_mut() {
                    let topic = session.saved.sublist.as_ref().map_or(0, |v| v.topic_id);
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
        if !session.saved.tags_loaded
            && !session
                .requests
                .has_purpose(quill::state::RequestPurpose::GetSavedMessagesTags { topic_id: 0 })
        {
            let _ = live.driver.load_saved_tags(0);
        }
        let session = &live.driver.session;
        if session.chat_views_as_topics(chat_id)
            && !session.saved.topics_requested
            && !session.saved.topics_exhausted
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
        let topics = session.saved.ordered_topics();
        if topics.is_empty() {
            let loading = !session.saved.topics_exhausted;
            return pane_placeholder(
                if loading {
                    "Loading saved chats"
                } else {
                    "No saved chats yet"
                },
                if loading {
                    "Your saved messages are being grouped by chat."
                } else {
                    "You can save messages from other chats here."
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
            .p_2()
            .gap_0p5()
            .role(Role::List)
            .aria_label("Saved chats");
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
        let more = !session.saved.topics_exhausted
            && (session.saved.topic_count == 0
                || session.saved.topics.len() < session.saved.topic_count as usize);
        if more {
            list = list.child(
                Button::new("saved-sublists-more")
                    .label("Load more")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.load_saved_topics();
                        }
                        cx.notify();
                    })),
            );
        }
        list.into_any_element()
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
            "No messages".to_owned()
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
                                    .child(title),
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
                            .child(preview),
                    ),
            )
            .context_menu(move |menu, _, _| {
                let pin_owner = owner.clone();
                let delete_owner = owner.clone();
                menu.item(
                    PopupMenuItem::new(if pinned { "Unpin" } else { "Pin" })
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
                    PopupMenuItem::new("Delete chat")
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
        let chat_id = session.open_chat?;
        if !session.is_saved_messages(chat_id) || self.saved_mode() == Some(SavedMode::Sublists) {
            return None;
        }
        let tags = session.saved_tag_choices();
        if tags.is_empty() {
            return None;
        }
        let active = session.saved.tag_search.as_ref().map(|s| s.tag.clone());
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
                    .label("All")
                    .ghost()
                    .selected(active.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.filter_saved_tag_ui(None, cx))),
            );
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
                .aria_label("Tags")
                .child(row)
                .into_any_element(),
        )
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
            .accessibility_label(format!("Tag {name}"));
        button = match glyph {
            Some(glyph) => button.label(format!("{glyph} {name}")),
            None => button.icon(IconName::Tag).label(name),
        };
        div()
            .id(("saved-tag-chip", ix))
            .flex_none()
            .child(button.on_click(cx.listener(move |this, _, _, cx| {
                this.filter_saved_tag_ui(Some(tag.clone()), cx);
            })))
            .context_menu(move |menu, _, _| {
                let filter_owner = owner.clone();
                let edit_owner = owner.clone();
                let filter_tag = filter_tag.clone();
                let edit_tag = edit_tag.clone();
                menu.item(
                    PopupMenuItem::new("Filter by Tag")
                        .icon(IconName::Tag)
                        .on_click(move |_, _, cx| {
                            let _ = filter_owner.update(cx, |this, cx| {
                                this.filter_saved_tag_ui(Some(filter_tag.clone()), cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new(if named { "Edit Name" } else { "Add Name" })
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
                s.saved
                    .tags
                    .iter()
                    .find(|entry| entry.tag == tag)
                    .map(|entry| entry.label.clone())
            })
            .unwrap_or_default();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Name")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        input.update(cx, |input, cx| input.set_value(current, window, cx));
        self.saved_tag_dialog = Some(SavedTagDialog { tag, input });
        cx.notify();
    }

    fn close_saved_tag_dialog(&mut self, cx: &mut Context<Self>) {
        self.saved_tag_dialog = None;
        cx.notify();
    }

    fn submit_saved_tag_name(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.saved_tag_dialog.as_ref() else {
            return;
        };
        let tag = dialog.tag.clone();
        let label = clean_tag_label(dialog.input.read(cx).value().as_ref());
        match self.live.as_mut() {
            Some(live) => {
                self.status_note = match live.driver.set_saved_tag_label(&tag, &label) {
                    Ok(_) => "tag name saved".into(),
                    Err(_) => "could not rename the tag".into(),
                };
            }
            None => self.status_note = "naming tags needs a live connection (demo)".into(),
        }
        self.saved_tag_dialog = None;
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
            let Some(state) = this.saved_tag_dialog.as_ref() else {
                return dialog.on_close(on_close);
            };
            let premium = this
                .session()
                .and_then(|s| s.my_user_id.and_then(|id| s.user(id)))
                .is_some_and(|u| u.is_premium);
            let named = this.session().is_some_and(|s| {
                s.saved
                    .tags
                    .iter()
                    .any(|entry| entry.tag == state.tag && !entry.label.is_empty())
            });
            let glyph = tag_glyph(&state.tag).unwrap_or_else(|| "Tag".to_owned());
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "You can label your {glyph} tag with a text name. Up to {SAVED_TAG_LABEL_MAX} characters."
                        )),
                )
                .child(Textarea::new(&state.input).aria_label("Tag name").h(px(36.)))
                .when(!premium, |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Naming tags needs Telegram Premium."),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(Button::new("saved-tag-cancel").label("Cancel").ghost().on_click(
                            cx.listener(|this, _, window, cx| {
                                this.close_saved_tag_dialog(cx);
                                this.close_kit_dialog_if_done(DialogKind::SavedTagName, window, cx);
                            }),
                        ))
                        .child(
                            Button::new("saved-tag-save")
                                .label("Save")
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
                    "Edit Name"
                } else {
                    "Add Name"
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
    match count {
        1 => "1 message".to_owned(),
        n => format!("{n} messages"),
    }
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
        assert_eq!(tag_name(&[], &[named.clone()], &heart), "\u{2764} Love");
        assert_eq!(tag_name(&[], &[], &heart), "\u{2764}");
        assert_eq!(
            tag_name(&[], &[], &ReactionType::CustomEmoji { custom_emoji_id: 5 }),
            "tag"
        );
    }
}
