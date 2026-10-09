//! Connect-driver integration tests (TDLib JSON injection via `RecordingSender`).
mod account_hygiene;
mod ai_tools;
mod bot_pending;
mod bots;
mod calls;
mod chat_list;
mod chat_state;
mod connect_flow;
mod deep_link_routing;
mod deep_links;
mod drafts_polls;
mod email_login;
mod emoji_sets;
mod find_in_history;
mod flood_retry;
mod group_admin;
mod group_calls;
mod groups;
mod history_window;
mod media_library;
mod message_menu;
mod message_ops;
mod messaging;
mod payments;
mod profile_panels;
mod proxy;
mod registration;
mod search;
mod search_upgrades;
mod settings;
mod share;
mod sponsored;
mod sticker_tabs;
mod stories;
mod subsection_tabs;
mod support;
mod threads;
mod translate;

pub(crate) use calls::{READY_CALL_JSON, group_call_test_driver};
pub(crate) use drafts_polls::{POLL_CLOSED_JSON, POLL_OPEN_JSON};
pub(crate) use support::{
    FailDownloadSender, FailFirstCallSender, FailFirstInlineQuerySender, TDJSON_ENV_LOCK,
    ViewCtlSender, assert_invalid, call_driver, call_state_for_rejoin, commit_typed_chat_search,
    commit_typed_search, failing_call_driver, group_participant_json, ingest_call_json,
    ingest_failing, poll_driver, prepared_tmp, ready_call_driver, ready_driver, ready_private_chat,
    ready_video_call_json, seed_ready_alice, seed_ready_call_user, seed_story, sent_request,
    session_fixture, sessions_driver, test_credentials, tracked_group_call,
};

pub(crate) use support::GroupCallDriverHarness;
