//! Sweep fixes (s4-links-tdlib-1): connect-driver behaviour that needs a
//! sender which can refuse requests.
use super::support::toggle_driver;
use crate::ids::{ChatId, RequestId};
use crate::state::{RequestPurpose, ShutdownPhase};
use crate::telegram::client::copy_and_parse;

/// Sweep 02-038: a close whose send fails keeps the logOut answer matched.
#[test]
fn a_refused_close_keeps_the_logout_request() {
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    driver.session.auth_state.logout_request = Some(RequestId(9));
    sender.refuse(true);
    assert!(driver.request_close().is_err());
    assert_eq!(driver.session.auth_state.logout_request, Some(RequestId(9)));
    assert_eq!(driver.session.shutdown, ShutdownPhase::Running);
}

/// Sweep 02-039: parameters that could not be sent leave no dead entry.
#[test]
fn refused_parameters_leave_no_pending_entry() {
    let (_dir, prepared) = super::prepared_tmp(&crate::platform::MemorySecretStore::new());
    let sink: std::sync::Arc<dyn crate::diagnostics::DiagnosticSink> =
        std::sync::Arc::new(crate::diagnostics::MemorySink::new());
    let sender = std::sync::Arc::new(super::support::ToggleSender::default());
    let session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink.clone());
    let mut driver = super::super::ConnectDriver::new(
        session,
        sender.clone(),
        super::test_credentials(),
        prepared,
    );
    sender.refuse(true);
    let seq = std::sync::atomic::AtomicU64::new(0);
    let wait = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#;
    let _ = driver.ingest(copy_and_parse(wait, &seq, &sink).expect("parse"));
    assert!(!driver.parameters_sent());
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::SetParameters)
    );
}

/// Sweep 02-061: the GIF search asks with the chat of the window that
/// searched, not the main window's.
#[test]
fn a_gif_search_carries_the_asking_windows_chat() {
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    driver.session.open_chat = Some(ChatId(5));
    driver.session.stickers.gifs.search_bot_username = "gif".into();
    driver.session.stickers.gifs.search_bot_user_id = Some(77);
    driver
        .search_gifs("cats", Some(ChatId(8)))
        .expect("sent")
        .expect("a request");
    let sent = sender.snapshot();
    let query = sent
        .iter()
        .find(|json| json.contains("getInlineQueryResults"))
        .expect("the inline query");
    assert!(query.contains(r#""chat_id":8"#), "{query}");
}

/// Sweep 04-043: a withdrawal whose request cannot be sent says so in the
/// box instead of looking as if Submit did nothing.
#[test]
fn a_refused_withdrawal_shows_an_error_in_the_box() {
    use crate::premium_hub::ton::GramWithdraw;
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    let grams = &mut driver.session.payments.hub.grams;
    grams.open = true;
    grams.withdraw = Some(GramWithdraw::idle(None));
    sender.refuse(true);
    assert!(driver.submit_gram_withdrawal("pw").is_err());
    assert!(matches!(
        driver.session.payments.hub.grams.withdraw,
        Some(GramWithdraw::Password {
            request: None,
            error: Some(_)
        })
    ));
}

/// Sweep 02-171: no own user, no profile colour: the icon is not cleared.
#[test]
fn a_profile_colour_without_the_own_user_is_refused() {
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    driver.session.my_user_id = Some(7);
    assert!(driver.set_profile_accent_color(3).is_err());
    assert!(
        !sender
            .snapshot()
            .iter()
            .any(|json| json.contains("setProfileAccentColor"))
    );
}

/// Sweep 02-139: a mask set whose request could not be sent is asked again.
#[test]
fn a_mask_set_that_was_not_sent_is_asked_again() {
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    let masks = &mut driver.session.stickers.masks;
    masks.strip_wanted = true;
    masks.installed_loaded = true;
    masks
        .installed
        .push(crate::telegram::envelope::StickerSetInfo {
            id: 11,
            title: String::new(),
            name: String::new(),
            size: 1,
            is_installed: true,
            is_official: false,
            is_owned: false,
            covers: Vec::new(),
            cover_files: Vec::new(),
            is_viewed: true,
        });
    sender.refuse(true);
    let _ = driver.load_editor_masks();
    assert!(driver.session.stickers.masks.set_requested.is_empty());
    sender.refuse(false);
    driver.load_editor_masks().expect("sent");
    assert!(
        sender
            .snapshot()
            .iter()
            .any(|json| json.contains("getStickerSet"))
    );
}
