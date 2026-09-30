//! Connect gate: credential/TDJSON gating and TDLib parameters.
use super::*;
use crate::calls::engine::{GroupVideoSource, GroupVideoSourceGroup};
use crate::credentials::TelegramCredentials;
use crate::ids::AccountKey;
use crate::lifecycle::plan_restore;
use crate::platform::{DatabaseKey, KeyDecision, SecretStore, load_or_create_key};
use crate::settings::AccountPaths;
use crate::telegram::envelope::{GroupCallVideoInfo, MessageSender, ParsedGroupCallParticipant};
use crate::telegram::ffi::resolve_tdjson_path;
use crate::telegram::requests::SetTdlibParameters;
use std::path::Path;

/// Classify credentials + tdjson availability. Does not open a client.
pub fn evaluate_gate(credentials_present: bool) -> ConnectGate {
    if !credentials_present {
        return ConnectGate::Blocked(ConnectBlocker::MissingCredentials);
    }
    match resolve_tdjson_path() {
        Some(tdjson) => ConnectGate::Ready { tdjson },
        None => ConnectGate::Blocked(ConnectBlocker::MissingTdjson),
    }
}

/// Resolve account paths and DB key. Credentials must already be validated.
pub fn prepare_connect<S: SecretStore + ?Sized>(
    app_root: &Path,
    account: AccountKey,
    store: &S,
    credentials: &TelegramCredentials,
) -> Result<PreparedConnect, ConnectBlocker> {
    let plan = plan_restore(
        app_root,
        account.clone(),
        store,
        Some(credentials.api_id),
        Some(credentials.api_hash.as_str()),
    )
    .map_err(ConnectBlocker::from)?;
    let key =
        load_or_create_key(store, &plan.account, plan.database_exists).map_err(|e| match e {
            KeyDecision::MissingAgainstExistingDb => ConnectBlocker::MissingKeyAgainstExistingDb,
            KeyDecision::Locked => ConnectBlocker::LockedStore,
            KeyDecision::Store(_) => ConnectBlocker::StoreError,
        })?;
    std::fs::create_dir_all(&plan.paths.tdlib_database).map_err(|_| ConnectBlocker::StoreError)?;
    std::fs::create_dir_all(&plan.paths.tdlib_files).map_err(|_| ConnectBlocker::StoreError)?;
    Ok(PreparedConnect {
        account: plan.account,
        paths: plan.paths,
        database_key: key,
        database_exists: plan.database_exists,
    })
}

/// Build `setTdlibParameters` from loaded credentials + local paths/key.
/// The returned JSON includes `api_hash`; callers must not log it.
pub fn build_set_tdlib_parameters(
    credentials: &TelegramCredentials,
    paths: &AccountPaths,
    database_key: &DatabaseKey,
) -> SetTdlibParameters {
    SetTdlibParameters {
        use_test_dc: std::env::var("QUILL_USE_TEST_DC")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
        database_directory: paths.tdlib_database.to_string_lossy().into_owned(),
        files_directory: paths.tdlib_files.to_string_lossy().into_owned(),
        database_encryption_key_b64: database_key.tdlib_base64(),
        api_id: credentials.api_id,
        api_hash: credentials.api_hash.clone(),
        device_model: "Desktop".into(),
        system_version: std::env::consts::OS.into(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        system_language_code: "en".into(),
    }
}

/// Phase C2g: build the engine's incoming-video subscription set from
/// the tracked participants' `video_info` / `screen_sharing_video_info`
/// (TDLib 1.8.67, `schema/td_api.tl:7163`). Skips the local user, paused
/// channels, and channels without a usable endpoint/ssrc — the engine
/// diffs this set against its subscriptions on every pump.
pub(crate) fn group_video_sources(
    participants: &[ParsedGroupCallParticipant],
) -> Vec<GroupVideoSource> {
    fn one(user_id: i64, info: &Option<GroupCallVideoInfo>, out: &mut Vec<GroupVideoSource>) {
        let Some(info) = info else { return };
        if info.is_paused || info.endpoint_id.is_empty() {
            return;
        }
        let ssrc_groups: Vec<GroupVideoSourceGroup> = info
            .source_groups
            .iter()
            .filter(|group| !group.source_ids.is_empty())
            .map(|group| GroupVideoSourceGroup {
                semantics: group.semantics.clone(),
                ssrcs: group.source_ids.clone(),
            })
            .collect();
        if ssrc_groups.is_empty() {
            return;
        }
        out.push(GroupVideoSource {
            user_id,
            endpoint: info.endpoint_id.clone(),
            ssrc_groups,
        });
    }

    let mut sources = Vec::new();
    for participant in participants {
        if participant.is_current_user {
            continue;
        }
        let MessageSender::User { user_id } = participant.participant_id else {
            continue;
        };
        one(user_id, &participant.video_info, &mut sources);
        one(
            user_id,
            &participant.screen_sharing_video_info,
            &mut sources,
        );
    }
    sources
}
