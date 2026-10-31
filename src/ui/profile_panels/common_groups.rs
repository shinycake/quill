//! "N groups in common" on a user profile: the list loads a page at a time
//! (`getGroupsInCommon` with `offset_chat_id`), gets a search field once it
//! is longer than a screenful, and shows a Load more row while the server
//! has more (tdesktop pages it as you scroll, `info_common_groups`).
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::TestSupportExt;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use quill::state::GROUPS_IN_COMMON_SEARCH_MIN;

/// The search field above the list, made when the profile opens.
#[derive(Default)]
pub(in crate::ui) struct CommonGroupsUi {
    search: Option<CommonGroupsSearch>,
}

struct CommonGroupsSearch {
    user_id: i64,
    input: Entity<TextareaState>,
    _subscription: Subscription,
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
impl CommonGroupsUi {
    /// The search field, for tests that type into it.
    pub(super) fn search_input(&self) -> Option<&Entity<TextareaState>> {
        self.search.as_ref().map(|search| &search.input)
    }
}

impl CommonGroupsUi {
    /// The profile closed: its search field goes with it (opening a
    /// profile makes a fresh, empty one).
    pub(in crate::ui) fn close(&mut self) {
        self.search = None;
    }
}

impl QuillApp {
    /// A fresh, empty search field for `user_id`'s groups in common.
    pub(in crate::ui) fn reset_common_groups_search(
        &mut self,
        user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::PARTICIPANT_FILTER))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |_this, _state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        );
        self.common_groups.search = Some(CommonGroupsSearch {
            user_id,
            input,
            _subscription: subscription,
        });
    }

    /// The titled list of groups shared with `user_id`: rows, the search
    /// field and the Load more row.
    pub(in crate::ui) fn groups_in_common_section(
        &self,
        user_id: i64,
        total: i32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let key = ProfileChatsKind::GroupsInCommon;
        let ids = session.profile_chat_list(key, user_id);
        if ids.is_empty() {
            // Loading shows nothing yet; a failed fetch offers Retry.
            return self.profile_chat_list_section(
                key,
                user_id,
                trn(keys::PROFILE_COMMON_GROUPS, total.max(0)),
                "info-common-group",
                "info-common-groups-retry",
                cx,
            );
        }
        let paging = session.common_groups_paging(user_id);
        let count = i64::try_from(ids.len()).unwrap_or(i64::MAX);
        let heading = div()
            .px_2()
            .text_xs()
            .font_semibold()
            .text_color(cx.theme().muted_foreground)
            .child(trn(
                keys::PROFILE_COMMON_GROUPS,
                i64::from(total.max(0)).max(count),
            ));
        let search =
            self.common_groups.search.as_ref().filter(|search| {
                search.user_id == user_id && ids.len() > GROUPS_IN_COMMON_SEARCH_MIN
            });
        let query = search
            .map(|search| search.input.read(cx).value().to_string())
            .unwrap_or_default();
        let rows: Vec<AnyElement> =
            ids.iter()
                .filter(|id| {
                    query.trim().is_empty()
                        || session.chats.get(id).is_some_and(|chat| {
                            quill::lang::search::matches(&query, &[&chat.title])
                        })
                })
                .filter_map(|id| self.profile_chat_row("info-common-group", *id, cx))
                .collect();
        let empty_search = rows.is_empty() && !query.trim().is_empty();
        let more = (!paging.exhausted).then(|| {
            if paging.loading_more {
                div()
                    .px_2()
                    .py_1p5()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PEOPLE_LOADING))
                    .into_any_element()
            } else {
                action_row(
                    "info-common-groups-more",
                    Some(IconName::ChevronDown),
                    tr(keys::Q_PEOPLE_LOAD_MORE),
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.load_more_common_groups(user_id, cx);
                }))
                .test_support()
                .into_any_element()
            }
        });
        Some(
            div()
                .flex()
                .flex_col()
                .w_full()
                .gap_0p5()
                .pt_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(heading)
                .children(search.map(|search| {
                    div()
                        .id("info-common-groups-search")
                        .px_2()
                        .child(
                            Textarea::new(&search.input)
                                .aria_label(tr(keys::PARTICIPANT_FILTER))
                                .h(px(40.)),
                        )
                        .test_support()
                }))
                .children(rows)
                .when(empty_search, |column| {
                    column.child(
                        div()
                            .id("info-common-groups-empty")
                            .aria_label(tr(keys::BOT_GROUPS_NOT_FOUND))
                            .test_support()
                            .px_2()
                            .py_1p5()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::BOT_GROUPS_NOT_FOUND)),
                    )
                })
                .children(more)
                .into_any_element(),
        )
    }

    /// Ask for the next page of groups in common.
    fn load_more_common_groups(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_more_groups_in_common(user_id);
        }
        cx.notify();
    }
}
