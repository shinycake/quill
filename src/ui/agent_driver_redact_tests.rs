//! Lists are default-deny in the granted scope: the search tabs (media,
//! downloads, channels, posts) and the reply chooser render rows for
//! ungranted chats, and nothing from those rows reaches the tree. Positive
//! checks keep the test honest: the rows of a granted chat stay, and with
//! All chats on the same rows all show.
use super::tests::{grant, new_app, read_tree, settle, turn_on};
use crate::ui::agent_state::TEST_LOCK;
use crate::ui::app::QuillApp;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::agent::activity;
use quill::agent::grants::{self, Scope};
use quill::agent::ui_driver::tree::UiTree;
use quill::composer::ComposerReplyTo;
use quill::ids::{AccountKey, ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::search_filters::GlobalSearchFilters;
use quill::search_tabs::SearchTab;
use quill::state::{PublicPostLimits, SearchMessageHit, SearchStatus};
use quill::telegram::envelope::MessageContent;

const GRANTED: &str = "Demo chat A";
const UNGRANTED: &str = "Demo chat B";
const UNGRANTED_CHANNEL: &str = "Demo channel";
const SECRET_FILE: &str = "secret-demo-plan.pdf";

fn hit(chat: i64, id: i64, preview: &str) -> SearchMessageHit {
    SearchMessageHit {
        sender: None,
        chat_id: ChatId(chat),
        message_id: MessageId(id),
        preview: preview.to_string(),
        is_outgoing: false,
        content: MessageContent::Text(preview.into()),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        is_pinned: false,
        media_album_id: 0,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
        author_signature: None,
        date: 1_790_000_000,
    }
}

/// The reveal button of a finished download row: the row's only named
/// node (its file name is plain text), so it is what a leak would show.
fn download_button() -> &'static str {
    tr(keys::Q_OPS_SHOW_DOWNLOADED_FILE_IN_FOLDER)
}

/// Every name and value in the tree, one string.
fn text_of(tree: &UiTree) -> String {
    tree.nodes
        .iter()
        .flat_map(|node| node.name.iter().chain(node.value.iter()))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Show the global search on `tab` with the demo session's results: hits
/// in chats 11 (granted) and 12 (not), a finished download of a file no
/// loaded message carries, and the spent public-posts quota.
fn show_search_tab(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, tab: SearchTab) {
    app.update_in(vcx, |app, window, cx| {
        if !app.search_is_open() {
            app.open_search_ui(window, cx);
        }
        let Some(session) = app.demo_session.as_mut() else {
            panic!("the demo session");
        };
        let search = &mut session.search.search;
        search.query = if tab == SearchTab::Channels {
            String::new()
        } else {
            "demo".to_string()
        };
        search.status = SearchStatus::Ready;
        search.recents = false;
        search.filters = tab.apply(GlobalSearchFilters::default());
        search.messages = vec![hit(12, 1, "top secret post"), hit(11, 2, "granted hit")];
        search.public_limits_exceeded = true;
        search.tabs.limits = Some(PublicPostLimits {
            daily_free: 3,
            remaining_free: 0,
            next_free_in: 3600,
            star_count: 50,
            current_query_free: false,
        });
        session.media.downloads.record_finished(77);
        session
            .sync
            .download_names
            .insert(77, SECRET_FILE.to_string());
        cx.notify();
    });
    settle(vcx);
}

#[gpui_kit::test]
fn list_parts_hide_rows_of_ungranted_chats_and_untagged_rows(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);

    // The reply chooser lists every destination, server search included.
    app.update_in(vcx, |app, _, cx| {
        app.composer_ui
            .set_reply(Some(ComposerReplyTo::new(ChatId(11), MessageId(101))));
        app.share.reply_elsewhere_open = true;
        cx.notify();
    });
    settle(vcx);
    let tree = read_tree(&app, vcx);
    let text = text_of(&tree);
    assert!(
        text.contains(GRANTED),
        "the granted destination is listed:\n{text}"
    );
    assert!(
        !text.contains(UNGRANTED) && !text.contains(UNGRANTED_CHANNEL),
        "no ungranted destination in the chooser:\n{text}"
    );
    assert!(
        !text.contains(tr(keys::REPLY_IN_CHATS_LIST)),
        "a heading inside the list is cut with the rows:\n{text}"
    );
    app.update_in(vcx, |app, window, cx| {
        app.close_reply_panels(window, cx);
        app.composer_ui.set_reply(None);
    });

    // The search tabs: channels (joined channel 13), photos (media rows
    // naming their chat), downloads (an untagged row), posts (hits and the
    // paid-search block).
    for tab in [
        SearchTab::Channels,
        SearchTab::Photos,
        SearchTab::Downloads,
        SearchTab::Posts,
    ] {
        show_search_tab(&app, vcx, tab);
        let tree = read_tree(&app, vcx);
        let text = text_of(&tree);
        assert!(
            !text.contains(UNGRANTED) && !text.contains(UNGRANTED_CHANNEL),
            "{tab:?}: an ungranted chat reached the tree:\n{text}"
        );
        assert!(
            !text.contains("top secret post"),
            "{tab:?}: an ungranted chat's hit reached the tree:\n{text}"
        );
        assert!(
            !text.contains(SECRET_FILE) && !text.contains(download_button()),
            "{tab:?}: a download row without a chat tag reached the tree:\n{text}"
        );
        assert!(
            !text.contains(tr(keys::Q_CHAT_SEARCHING_PUBLIC_POSTS_PAST_THE)),
            "{tab:?}: the paid-search block reached the tree:\n{text}"
        );
        match tab {
            SearchTab::Photos => assert!(
                text.contains(GRANTED),
                "photos: the granted chat's media row stays:\n{text}"
            ),
            SearchTab::Posts => assert!(
                text.contains("granted hit"),
                "posts: the granted chat's hit stays:\n{text}"
            ),
            _ => {}
        }
    }

    // All chats on: the same screen shows the rows the grants cut.
    grants::update(&AccountKey::primary(), |g| g.all_chats = Some(Scope::Read));
    settle(vcx);
    let tree = read_tree(&app, vcx);
    let text = text_of(&tree);
    assert!(text.contains(UNGRANTED), "{text}");
    assert!(text.contains("top secret post"), "{text}");
    show_search_tab(&app, vcx, SearchTab::Downloads);
    let text = text_of(&read_tree(&app, vcx));
    assert!(text.contains(download_button()), "{text}");
    show_search_tab(&app, vcx, SearchTab::Channels);
    let text = text_of(&read_tree(&app, vcx));
    assert!(text.contains(UNGRANTED_CHANNEL), "{text}");
    grants::update(&AccountKey::primary(), |g| g.all_chats = None);
}
