//! The Apps tab of the global search (tdesktop `dialogs_suggestions.cpp`:
//! "Apps you use", then "Grossing apps" from `getGrossingWebAppBots`).
//! A row opens the bot's main app in the mini app window.

use super::app::QuillApp;
use super::search_ui::search_result_row;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::search_filters::SearchScope;

/// One row of the tab.
struct AppRow {
    bot_id: i64,
    title: String,
    subtitle: String,
}

impl QuillApp {
    fn app_row(&self, bot_id: i64) -> Option<AppRow> {
        let session = self.session()?;
        let user = session.users.get(&bot_id)?;
        let subtitle = session
            .bot_info
            .get(&bot_id)
            .and_then(|info| info.as_ref())
            .map(|info| info.short_description.clone())
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| {
                if user.username.is_empty() {
                    "Mini app".to_string()
                } else {
                    format!("@{}", user.username)
                }
            });
        Some(AppRow {
            bot_id,
            title: user.display_name(),
            subtitle,
        })
    }

    fn rows_matching(&self, ids: &[i64], query: &str) -> Vec<AppRow> {
        let needle = query.trim().to_lowercase();
        ids.iter()
            .filter_map(|id| self.app_row(*id))
            .filter(|row| {
                needle.is_empty()
                    || row.title.to_lowercase().contains(&needle)
                    || row.subtitle.to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// The chip that reaches the tab from the empty, focused search.
    pub(super) fn apps_entry_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.search.recents
            || !session.search.query.trim().is_empty()
            || session.search.filters.scope == SearchScope::Apps
        {
            return None;
        }
        Some(
            div()
                .flex()
                .child(
                    Button::new("search-apps-entry")
                        .label("Apps")
                        .icon(IconName::Bot)
                        .ghost()
                        .small()
                        .tooltip("Mini apps you use and the most popular ones")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let mut next =
                                this.session().map(|s| s.search.filters).unwrap_or_default();
                            next.scope = SearchScope::Apps;
                            this.set_search_filters(next, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    /// The tab's body.
    pub(super) fn apps_tab_block(&self, query: &str, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let (used, grossing, loading) = self
            .session()
            .map(|session| {
                let used: Vec<i64> = session
                    .web_apps
                    .attachment_menu_bots
                    .iter()
                    .filter(|bot| bot.is_added)
                    .map(|bot| bot.bot_user_id)
                    .collect();
                (
                    used,
                    session.web_apps.grossing_bots.clone(),
                    session.web_apps.grossing_loading,
                )
            })
            .unwrap_or((Vec::new(), None, false));
        let used_rows = self.rows_matching(&used, query);
        let grossing_rows = grossing
            .as_deref()
            .map(|ids| self.rows_matching(ids, query))
            .unwrap_or_default();
        let mut block = div().id("search-apps").flex().flex_col().gap_2();
        let section =
            |heading: &str, rows: Vec<AppRow>, prefix: &'static str, cx: &mut Context<Self>| {
                let mut section = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().font_semibold().child(heading.to_string()));
                for row in rows {
                    let bot_id = row.bot_id;
                    let open = self.mini_app_open_for(bot_id);
                    let photo = self.chat_photo_for_row(ChatId(bot_id));
                    let subtitle = if open {
                        "Open now".to_string()
                    } else {
                        row.subtitle
                    };
                    section = section.child(search_result_row(
                        (prefix, bot_id as u64),
                        row.title,
                        subtitle,
                        None,
                        query,
                        photo,
                        cx,
                        move |this, _, cx| this.open_main_web_app(bot_id, "", cx),
                    ));
                }
                section
            };
        if !used_rows.is_empty() {
            block = block.child(section("Apps you use", used_rows, "search-app-used", cx));
        }
        if !grossing_rows.is_empty() {
            block = block.child(section(
                "Grossing apps",
                grossing_rows,
                "search-app-top",
                cx,
            ));
        } else {
            let note = if loading || grossing.is_none() {
                "Loading apps…".to_string()
            } else if query.trim().is_empty() {
                "No apps to show right now.".to_string()
            } else {
                format!("No apps match “{}”.", query.trim())
            };
            block = block.child(div().text_xs().text_color(muted).child(note));
        }
        block
            .child(div().text_xs().text_color(muted).child(
                "Mini apps open in their own window. Payments inside them aren't supported yet.",
            ))
            .into_any_element()
    }
}
