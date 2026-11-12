use super::*;
use AutoDownloadKind as K;
use AutoDownloadSource as S;

fn from_json(text: &str) -> AutoDownloadSettings {
    serde_json::from_str::<AutoDownloadPrefs>(text).unwrap().0
}

#[test]
fn defaults_follow_telegram_desktop() {
    let settings = AutoDownloadSettings::default();
    for source in S::ALL {
        let limits = settings.source(source);
        assert_eq!(limits.photo, DEFAULT_DOWNLOAD_LIMIT);
        assert_eq!(limits.video, DEFAULT_AUTOPLAY_LIMIT);
        assert!(limits.any_enabled());
    }
    assert_eq!(settings.channels.file, 0);
    assert_eq!(settings.channels.music, 0);
    assert_eq!(settings.private.file, DEFAULT_DOWNLOAD_LIMIT);
}

#[test]
fn the_size_limit_decides() {
    let settings = AutoDownloadSettings::default();
    let chat = ChatId(5);
    assert!(settings.allows(chat, S::Private, K::Photo, DEFAULT_DOWNLOAD_LIMIT));
    assert!(!settings.allows(chat, S::Private, K::Photo, DEFAULT_DOWNLOAD_LIMIT + 1));
    // A size not known yet does not block.
    assert!(settings.allows(chat, S::Private, K::File, 0));
    assert!(!settings.allows(chat, S::Channels, K::File, 0));
}

#[test]
fn switching_a_source_off_stops_every_kind() {
    let mut settings = AutoDownloadSettings::default();
    settings.set_source_enabled(S::Groups, false);
    assert!(!settings.groups.any_enabled());
    for kind in K::ALL {
        assert!(!settings.allows(ChatId(1), S::Groups, kind, 1));
    }
    settings.set_source_enabled(S::Groups, true);
    assert_eq!(settings.groups, SourceLimits::defaults(S::Groups));
}

#[test]
fn exceptions_win_over_the_source() {
    let mut settings = AutoDownloadSettings::default();
    settings.set_source_enabled(S::Channels, false);
    settings.exceptions.insert(7, AutoDownloadOverride::Always);
    settings.exceptions.insert(8, AutoDownloadOverride::Never);
    // Always falls back to the default limit of a kind that is off.
    assert!(settings.allows(ChatId(7), S::Channels, K::Photo, 1));
    assert!(!settings.allows(ChatId(7), S::Channels, K::Photo, DEFAULT_DOWNLOAD_LIMIT + 1));
    // Channels download no files even by default.
    assert!(!settings.allows(ChatId(7), S::Channels, K::File, 1));
    assert!(!settings.allows(ChatId(8), S::Private, K::Photo, 1));
}

#[test]
fn exception_lists_keep_to_their_source() {
    let mut settings = AutoDownloadSettings::default();
    settings.exceptions.insert(1, AutoDownloadOverride::Always);
    settings.exceptions.insert(2, AutoDownloadOverride::Always);
    settings.exceptions.insert(3, AutoDownloadOverride::Never);
    let source_of = |chat: ChatId| match chat.0 {
        1 | 3 => Some(S::Groups),
        2 => Some(S::Private),
        _ => None,
    };
    let always = settings.exception_chats(AutoDownloadOverride::Always, S::Groups, source_of);
    assert_eq!(always, vec![ChatId(1)]);
    let never = settings.exception_chats(AutoDownloadOverride::Never, S::Groups, source_of);
    assert_eq!(never, vec![ChatId(3)]);
}

#[test]
fn a_file_without_settings_gets_the_defaults() {
    assert_eq!(from_json("{}"), AutoDownloadSettings::default());
}

#[test]
fn old_bit_grids_carry_over() {
    // Photos only in private chats, everything off in channels.
    let settings = from_json(
        r#"{"auto_download_private": 1, "auto_download_groups": 51, "auto_download_channels": 0}"#,
    );
    assert_eq!(settings.private.photo, DEFAULT_DOWNLOAD_LIMIT);
    assert_eq!(settings.private.voice, 0);
    assert_eq!(settings.private.gif, 0);
    assert!(!settings.channels.any_enabled());
    assert!(!settings.channels.allows(K::Voice, 1));
    // 51 = photo, voice, music and GIF.
    assert_eq!(settings.groups.music, DEFAULT_DOWNLOAD_LIMIT);
    assert_eq!(settings.groups.gif, DEFAULT_AUTOPLAY_LIMIT);
    assert_eq!(settings.groups.video, 0);
}

#[test]
fn settings_round_trip_without_the_old_keys() {
    let mut settings = AutoDownloadSettings::default();
    settings.private.video = 0;
    settings
        .exceptions
        .insert(-10_042, AutoDownloadOverride::Never);
    let text = serde_json::to_string(&AutoDownloadPrefs(settings.clone())).unwrap();
    assert!(!text.contains("auto_download_private"));
    assert_eq!(from_json(&text), settings);
}

fn groups_only(chat: ChatId) -> Option<AutoDownloadSource> {
    (chat.0 > 0).then_some(S::Groups)
}

#[test]
fn the_draft_applies_its_sliders_on_save() {
    let mut settings = AutoDownloadSettings::default();
    let mut draft = AutoDownloadDraft::new(&settings, S::Groups, groups_only);
    assert!(draft.is_on(K::Photo));
    assert_eq!(
        draft.group_limit(AutoDownloadLimitGroup::Download),
        DEFAULT_DOWNLOAD_LIMIT
    );
    draft.set_on(K::File, false);
    draft.set_group_limit(AutoDownloadLimitGroup::Download, 20 * MIB);
    draft.set_group_limit(AutoDownloadLimitGroup::Autoplay, 5 * MIB);
    // Nothing changes before Save.
    assert_eq!(settings, AutoDownloadSettings::default());
    draft.apply(&mut settings, groups_only);
    assert_eq!(settings.groups.photo, 20 * MIB);
    assert_eq!(settings.groups.file, 0);
    // Voice and music were on: they follow the download slider.
    assert_eq!(settings.groups.voice, 20 * MIB);
    assert_eq!(settings.groups.music, 20 * MIB);
    assert_eq!(settings.groups.video, 5 * MIB);
    assert_eq!(settings.private, SourceLimits::defaults(S::Private));
}

#[test]
fn switching_every_shown_kind_off_turns_the_source_off() {
    let mut settings = AutoDownloadSettings::default();
    let mut draft = AutoDownloadDraft::new(&settings, S::Private, groups_only);
    for kind in K::SHOWN {
        draft.set_on(kind, false);
    }
    draft.apply(&mut settings, groups_only);
    assert_eq!(settings.private, SourceLimits::DISABLED);
    // Switching one back on starts from where its slider stood.
    let mut draft = AutoDownloadDraft::new(&settings, S::Private, groups_only);
    assert_eq!(
        draft.group_limit(AutoDownloadLimitGroup::Download),
        10 * MIB
    );
    draft.set_on(K::Photo, true);
    draft.apply(&mut settings, groups_only);
    assert_eq!(settings.private.photo, 10 * MIB);
    assert_eq!(settings.private.voice, 0);
}

#[test]
fn the_draft_owns_only_its_sources_exceptions() {
    let mut settings = AutoDownloadSettings::default();
    settings.exceptions.insert(1, AutoDownloadOverride::Always);
    // A chat of another source (or unknown) keeps its exception.
    settings.exceptions.insert(-5, AutoDownloadOverride::Never);
    let mut draft = AutoDownloadDraft::new(&settings, S::Groups, groups_only);
    assert_eq!(draft.always, [1].into());
    draft.set_exceptions(AutoDownloadOverride::Never, [1, 2].into());
    assert!(draft.always.is_empty());
    draft.apply(&mut settings, groups_only);
    assert_eq!(
        settings.exception(ChatId(1)),
        Some(AutoDownloadOverride::Never)
    );
    assert_eq!(
        settings.exception(ChatId(2)),
        Some(AutoDownloadOverride::Never)
    );
    assert_eq!(
        settings.exception(ChatId(-5)),
        Some(AutoDownloadOverride::Never)
    );
}
