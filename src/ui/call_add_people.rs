//! "Add People" over an established 1:1 call (tdesktop
//! `Group::PrepareInviteBox`, a `ConfInviteController` in a `PeerListBox`):
//! a search field, "Create Call Link", every contact by last seen with the
//! other side checked and disabled, and, while you type, the contacts
//! whose names match followed by the people the server found. What the
//! list shows is `Session::add_people_view`; the picks survive the
//! filtering. Invite, or "Create Call Link" with nobody picked, takes the
//! call into a new conference call (`ui::call_conference_ui`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_base::{Disableable, StyledExt};
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Icon, Sizable, h_flex};
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::state::{AddPeopleEmpty, AddPeopleRow, AddPeopleView};
use quill::telegram::envelope::CallState;

/// A row's userpic (tdesktop `groupCallInviteMembersList` 42 px items
/// carry a 36 px photo).
const USERPIC: f32 = 36.;

/// The open box.
pub(super) struct AddPeopleBox {
    /// The 1:1 call the box is over: it closes when that call ends.
    call_id: i32,
    /// Everyone checked, in the order they were picked.
    pub(super) picked: Vec<i64>,
    /// People checked from a search result, latest first: they stay at
    /// the top of the list (tdesktop `prependRowFromSearchResult`).
    pub(super) promoted: Vec<i64>,
    /// The search field, made in the call window when the box first
    /// shows there.
    search: Option<Entity<InputState>>,
    /// The text the field starts with.
    initial_query: String,
}

impl AddPeopleBox {
    /// A fresh box over the call `call_id`: nothing picked, no search.
    pub(super) fn new(call_id: i32) -> Self {
        Self {
            call_id,
            picked: Vec::new(),
            promoted: Vec::new(),
            search: None,
            initial_query: String::new(),
        }
    }

    /// A box whose search field starts with `query` (screenshot demos).
    #[cfg(feature = "demo-capture")]
    pub(super) fn with_query(call_id: i32, query: String) -> Self {
        Self {
            initial_query: query,
            ..Self::new(call_id)
        }
    }
}

impl QuillApp {
    /// "Add People" (tdesktop `Panel::addPeople`): only once connected.
    pub(super) fn open_call_add_people(&mut self, cx: &mut Context<Self>) {
        let established = self
            .session()
            .and_then(|session| session.calls.active_call.as_ref())
            .filter(|call| matches!(call.state, CallState::Ready))
            .map(|call| call.id);
        let Some(call_id) = established else {
            self.connection
                .set_note(StatusNote::info(tr(keys::CALL_ERROR_ADD_NOT_STARTED)));
            cx.notify();
            return;
        };
        self.calls.add_people = Some(AddPeopleBox::new(call_id));
        if let Some(live) = self.live.as_mut()
            && live.driver.session.users_state.contacts.is_none()
        {
            let _ = live.driver.fetch_contacts();
        }
        cx.notify();
    }

    /// The box's call ended (or was replaced): the box goes with it, its
    /// field and picks never reused (called as the call window follows
    /// the call).
    pub(super) fn close_stale_call_add_people(&mut self) {
        let current = self
            .session()
            .and_then(|session| session.calls.active_call.as_ref())
            .map(|call| call.id);
        let stale = self
            .calls
            .add_people
            .as_ref()
            .is_some_and(|open| Some(open.call_id) != current);
        if stale {
            self.calls.add_people = None;
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.search_add_people("");
            }
        }
    }

    fn close_call_add_people(&mut self, cx: &mut Context<Self>) {
        self.calls.add_people = None;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.search_add_people("");
        }
        cx.notify();
    }

    /// The search field's text, empty before it exists.
    fn call_add_people_query(&self, cx: &App) -> String {
        self.calls
            .add_people
            .as_ref()
            .and_then(|open| open.search.as_ref())
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// What the box lists for the typed query.
    fn call_add_people_view(&self, cx: &App) -> Option<AddPeopleView> {
        let open = self.calls.add_people.as_ref()?;
        let session = self.session()?;
        let peer = session
            .calls
            .active_call
            .as_ref()
            .filter(|call| call.id == open.call_id)?
            .user_id;
        Some(session.add_people_view(
            peer,
            &open.picked,
            &open.promoted,
            &self.call_add_people_query(cx),
        ))
    }

    /// Check or uncheck someone; a person checked from the server's
    /// results joins the top of the list.
    fn toggle_call_add_person(&mut self, user_id: i64, from_search: bool, cx: &mut Context<Self>) {
        if let Some(open) = self.calls.add_people.as_mut() {
            if let Some(at) = open.picked.iter().position(|id| *id == user_id) {
                open.picked.remove(at);
            } else {
                open.picked.push(user_id);
                if from_search && !open.promoted.contains(&user_id) {
                    open.promoted.insert(0, user_id);
                }
            }
        }
        cx.notify();
    }

    /// Enter in the search field toggles the first person you can pick.
    fn toggle_first_call_add_person(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.call_add_people_view(cx) else {
            return;
        };
        if let Some(user_id) = view.first_pickable() {
            let from_search = view.global.iter().any(|row| row.user_id == user_id);
            self.toggle_call_add_person(user_id, from_search, cx);
        }
    }

    /// The box's Invite, or "Create Call Link" with nobody picked: the
    /// call moves into a new conference call.
    fn finish_call_add_people(&mut self, share_link: bool, cx: &mut Context<Self>) {
        let Some(open) = self.calls.add_people.take() else {
            return;
        };
        let invite = if share_link { Vec::new() } else { open.picked };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.search_add_people("");
            if live
                .driver
                .upgrade_call_to_conference(invite, share_link)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_AV_COULDNT_JOIN_THE_VOICE_CHAT,
                )));
            }
        } else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_AV_VOICE_CHATS_NEED_A_LIVE)));
        }
        cx.notify();
    }

    /// The box's search field, made (and focused) in the call window the
    /// first time the box shows there (tdesktop focuses the box's
    /// multi-select).
    fn call_add_people_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<InputState>> {
        let open = self.calls.add_people.as_mut()?;
        if let Some(input) = open.search.clone() {
            return Some(input);
        }
        let initial = std::mem::take(&mut open.initial_query);
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(keys::PARTICIPANT_FILTER))
                .default_value(initial.clone())
        });
        cx.subscribe_in(
            &input,
            window,
            |this, state, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    let query = state.read(cx).value().to_string();
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.search_add_people(&query);
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.toggle_first_call_add_person(cx),
                _ => {}
            },
        )
        .detach();
        input.update(cx, |input, cx| input.focus(window, cx));
        if !initial.trim().is_empty()
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.search_add_people(&initial);
        }
        if let Some(open) = self.calls.add_people.as_mut() {
            open.search = Some(input.clone());
        }
        Some(input)
    }

    /// One person: userpic, name and last seen, checked when picked.
    fn call_add_people_row(
        &self,
        row: &AddPeopleRow,
        from_search: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let user_id = row.user_id;
        let photo = self
            .session()
            .and_then(|session| session.user_photo_path(user_id))
            .and_then(|path| {
                quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
            });
        let muted = cx.theme().muted_foreground;
        let checked = row.checked;
        Checkbox::new(("call-add-person", user_id as u64))
            .debug_selector(move || format!("call-add-person-{user_id}"))
            .checked(checked)
            .disabled(row.already_in)
            .items_center()
            .accessibility_label(row.name.clone())
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(super::message_text::kit_avatar_element(
                        &row.name,
                        photo.as_deref(),
                        px(USERPIC),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().text_sm().truncate().child(row.name.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .truncate()
                                    .child(row.status.clone()),
                            ),
                    ),
            )
            .on_click(cx.listener(move |this, &on, _, cx| {
                if on != checked {
                    this.toggle_call_add_person(user_id, from_search, cx);
                }
            }))
            .into_any_element()
    }

    /// The "Add People" box over the call window.
    pub(super) fn call_add_people_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let search = self.call_add_people_search(window, cx)?;
        let view = self.call_add_people_view(cx)?;
        let none_picked = self
            .calls
            .add_people
            .as_ref()
            .is_none_or(|open| open.picked.is_empty());
        let muted = cx.theme().muted_foreground;
        let mut rows = div()
            .id("call-add-people-rows")
            .flex()
            .flex_col()
            .gap_1()
            .h(px(240.))
            .overflow_y_scroll();
        for row in &view.rows {
            rows = rows.child(self.call_add_people_row(row, false, cx));
        }
        for row in &view.global {
            rows = rows.child(self.call_add_people_row(row, true, cx));
        }
        if let Some(empty) = view.empty {
            rows = rows.child(
                div()
                    .id("call-add-people-empty")
                    .debug_selector(|| "call-add-people-empty".into())
                    .py_2()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(match empty {
                        AddPeopleEmpty::ContactsLoading | AddPeopleEmpty::SearchLoading => {
                            keys::CONTACTS_LOADING
                        }
                        AddPeopleEmpty::NoContacts => keys::CONTACTS_NOT_FOUND,
                        AddPeopleEmpty::NoResults => keys::BLOCKED_LIST_NOT_FOUND,
                    })),
            );
        }
        let field = Input::new(&search)
            .small()
            .cleanable(true)
            .prefix(Icon::new(IconName::Search).size(px(14.)).text_color(muted))
            .aria_label(tr(keys::PARTICIPANT_FILTER));
        // tdesktop hides the list's above-widget while searching.
        let link = (!view.searching).then(|| {
            Button::new("call-add-people-link")
                .debug_selector(|| "call-add-people-link".into())
                .label(tr(keys::CONFCALL_CREATE_LINK))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.finish_call_add_people(true, cx);
                }))
        });
        Some(
            div()
                .id("call-add-people")
                .debug_selector(|| "call-add-people".into())
                .absolute()
                .inset_0()
                .bg(cx.theme().overlay)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(340.))
                        .rounded(cx.theme().radius_lg)
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().background)
                        .text_color(cx.theme().foreground)
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .text_base()
                                .font_semibold()
                                .child(tr(keys::GROUP_CALL_INVITE_CONF)),
                        )
                        .child(field)
                        .children(link)
                        .child(rows)
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("call-add-people-cancel")
                                        .label(tr(keys::CANCEL))
                                        .ghost()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_call_add_people(cx);
                                        })),
                                )
                                .child(
                                    Button::new("call-add-people-invite")
                                        .label(tr(keys::GROUP_CALL_INVITE_BUTTON))
                                        .primary()
                                        .disabled(none_picked)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.finish_call_add_people(false, cx);
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "call_add_people_tests.rs"]
mod tests;
