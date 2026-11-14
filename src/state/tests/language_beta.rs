//! Settings → Language: beta pack lookups settle only their own pack.
use super::common::*;
use super::*;
use crate::state::fetch::Fetch;

fn info_json(extra: RequestId, id: &str, translated: i64, beta: bool) -> String {
    format!(
        r#"{{"@type":"languagePackInfo","@extra":"{}","id":"{id}","base_language_pack_id":"","name":"Name","native_name":"Native (beta)","plural_code":"vi","is_official":false,"is_rtl":false,"is_beta":{beta},"is_installed":false,"total_string_count":100,"translated_string_count":{translated},"local_string_count":0,"translation_url":""}}"#,
        extra.0
    )
}

#[test]
fn beta_packs_list_once_answered_and_half_translated() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let asked = RequestPurpose::Settings(SettingsPurpose::Language(LanguagePurpose::BetaInfo));
    let betas = session.settings.language.betas_mut();
    assert_eq!(betas.len(), crate::state::BETA_LANGUAGE_PACKS.len());
    assert!(betas.iter().all(|beta| beta.info.is_idle()));
    let requests = [(); 4].map(|()| session.request(asked, None));
    for (beta, request) in session.settings.language.betas.iter_mut().zip(requests) {
        beta.info.begin_with(request);
    }
    let [vi, ja, sv, gone] = requests;
    // Answers land on the pack that asked, whatever order they come in.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &info_json(ja, "ja-raw", 99, true),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &info_json(vi, "vi-raw", 88, true),
    );
    // Under half translated: answered, not listed.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &info_json(sv, "sv-raw", 49, true),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"Language pack not found"}}"#,
            gone.0
        ),
    );
    let listed: Vec<String> = session
        .settings
        .language
        .listed_betas()
        .into_iter()
        .map(|info| info.id)
        .collect();
    // Listed in table order, not answer order.
    assert_eq!(listed, ["vi-raw", "ja-raw"]);
    let stages: Vec<bool> = session
        .settings
        .language
        .betas
        .iter()
        .take(4)
        .map(|beta| matches!(beta.info, Fetch::Ready(_)))
        .collect();
    assert_eq!(stages, [true, true, true, false]);
    assert!(
        session
            .settings
            .language
            .betas
            .get(3)
            .is_some_and(|b| b.info.can_ask())
    );
    // The link box never sees these answers.
    assert!(session.settings.language_link.is_none());
}

#[test]
fn translated_share_rounds_down_and_survives_odd_counts() {
    use crate::state::translated_percent;
    let info = |total, translated| crate::telegram::envelope::LanguagePackInfoData {
        id: "vi-raw".into(),
        base_language_pack_id: String::new(),
        name: String::new(),
        native_name: String::new(),
        plural_code: String::new(),
        is_official: false,
        is_rtl: false,
        is_beta: true,
        is_installed: false,
        total_string_count: total,
        translated_string_count: translated,
    };
    assert_eq!(translated_percent(&info(8892, 7794)), 87);
    assert_eq!(translated_percent(&info(0, 10)), 0);
    assert_eq!(translated_percent(&info(10, 20)), 100);
    assert_eq!(translated_percent(&info(10, -1)), 0);
}
