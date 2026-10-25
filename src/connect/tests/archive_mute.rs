//! Driver tests: the "archive and mute new chats" suggestion and switch.
use super::paid_messages::{Fixture, cleanup, extra_of, fixture, ingest, requests};
use crate::archive_mute::{ACTION, suggestion_due};

fn suggest(f: &mut Fixture) {
    ingest(
        f,
        &format!(
            r#"{{"@type":"updateSuggestedActions","added_actions":[{{"@type":"{ACTION}"}}],"removed_actions":[]}}"#
        ),
    );
}

fn answer_settings(f: &mut Fixture, on: bool) {
    let fetches = requests(f, "getArchiveChatListSettings");
    let extra = extra_of(fetches.last().expect("a settings fetch"));
    ingest(
        f,
        &format!(
            r#"{{"@type":"archiveChatListSettings","archive_and_mute_new_chats_from_unknown_users":{on},"keep_unmuted_chats_archived":false,"keep_chats_from_folders_archived":false,"@extra":"{extra}"}}"#
        ),
    );
}

fn due(f: &Fixture) -> bool {
    let chat_list = &f.1.session.chat_list;
    suggestion_due(
        &chat_list.suggestions.actions,
        chat_list.archive_chat_list_settings.as_ref(),
    )
}

#[test]
fn the_suggestion_loads_the_setting_and_hiding_it_tells_the_server() {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateSuggestedActions","added_actions":[{"@type":"suggestedActionUpgradePremium"}],"removed_actions":[]}"#,
    );
    assert!(requests(&f, "getArchiveChatListSettings").is_empty());
    suggest(&mut f);
    assert_eq!(requests(&f, "getArchiveChatListSettings").len(), 1);
    assert!(!due(&f), "waits for the setting");
    answer_settings(&mut f, false);
    assert!(due(&f));

    f.1.hide_suggested_action(ACTION).unwrap();
    let hides = requests(&f, "hideSuggestedAction");
    assert_eq!(hides.len(), 1);
    let hide = hides.first().expect("a hide");
    assert_eq!(hide["action"]["@type"], ACTION);
    assert!(!due(&f), "gone at once");

    // A second suggestion uses the setting already loaded.
    suggest(&mut f);
    assert_eq!(requests(&f, "getArchiveChatListSettings").len(), 1);
    assert!(due(&f));
    cleanup(f);
}

#[test]
fn no_suggestion_when_the_setting_is_on_and_the_switch_stays() {
    let mut f = fixture();
    suggest(&mut f);
    answer_settings(&mut f, true);
    assert!(!due(&f));
    let settings = f.1.session.chat_list.archive_chat_list_settings.unwrap();
    let off = crate::telegram::requests::ArchiveChatListSettings {
        archive_and_mute_new_chats_from_unknown_users: false,
        ..settings
    };
    let state = f.1.session.chat_list.archive_mute;
    assert!(state.switch_shown(&off, false), "seen on, still offered");
    cleanup(f);
}

#[test]
fn the_server_option_offers_the_switch() {
    let mut f = fixture();
    let off = crate::telegram::requests::ArchiveChatListSettings::default();
    assert!(!f.1.session.chat_list.archive_mute.switch_shown(&off, false));
    ingest(
        &mut f,
        r#"{"@type":"updateOption","name":"can_archive_and_mute_new_chats_from_unknown_users","value":{"@type":"optionValueBoolean","value":true}}"#,
    );
    assert!(f.1.session.chat_list.archive_mute.switch_shown(&off, false));
    cleanup(f);
}

#[test]
fn a_refused_hide_brings_the_suggestion_back() {
    let mut f = fixture();
    suggest(&mut f);
    answer_settings(&mut f, false);
    f.1.hide_suggested_action(ACTION).unwrap();
    let hides = requests(&f, "hideSuggestedAction");
    let extra = extra_of(hides.last().expect("a hide"));
    ingest(
        &mut f,
        &format!(r#"{{"@type":"error","code":400,"message":"BAD","@extra":"{extra}"}}"#),
    );
    assert!(due(&f));
    cleanup(f);
}
