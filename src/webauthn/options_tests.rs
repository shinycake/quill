use super::*;

/// The shape `getPasskeyParameters` returns (field names as the probe saw
/// on the sign-in side; values made up).
const CREATION: &str = r#"{"publicKey":{
    "rp":{"id":"telegram.org","name":"Telegram"},
    "user":{"id":"AQIDBA==","name":"+15550100","displayName":"Quill Test"},
    "challenge":"3q2-7w",
    "pubKeyCredParams":[{"type":"public-key","alg":-257},{"type":"public-key","alg":-7}],
    "timeout":120000,
    "authenticatorSelection":{"residentKey":"required","userVerification":"required"},
    "attestation":"none"}}"#;

const REQUEST: &str = r#"{"publicKey":{
    "challenge":"AAEC_-8",
    "rpId":"telegram.org",
    "timeout":60000,
    "userVerification":"required",
    "allowCredentials":[{"type":"public-key","id":"qrvM"},{"type":"public-key","id":"3e7_"}]}}"#;

#[test]
fn parses_creation_options_like_tdesktop() {
    let options = CreationOptions::parse(CREATION).unwrap();
    assert_eq!(options.rp_id, "telegram.org");
    assert_eq!(options.rp_name, "Telegram");
    assert_eq!(options.user_id, vec![1, 2, 3, 4]);
    assert_eq!(options.user_name, "+15550100");
    assert_eq!(options.user_display_name, "Quill Test");
    assert_eq!(options.challenge, vec![0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(options.algorithms, vec![-257, -7]);
    assert_eq!(options.timeout_ms, 120_000);
    // ES256 wins wherever it is in the list.
    assert_eq!(options.preferred_algorithm(), COSE_ES256);
}

#[test]
fn creation_defaults_and_rejects() {
    // No `publicKey`, an empty one, or no object at all: nothing.
    assert_eq!(CreationOptions::parse(r#"{"rp":{}}"#), None);
    assert_eq!(CreationOptions::parse(r#"{"publicKey":{}}"#), None);
    assert_eq!(CreationOptions::parse("[1]"), None);
    assert_eq!(CreationOptions::parse("not json"), None);
    // Missing fields stay empty and the timeout falls back to 60 s.
    let bare = CreationOptions::parse(r#"{"publicKey":{"challenge":"AA"}}"#).unwrap();
    assert_eq!(bare.challenge, vec![0]);
    assert!(bare.rp_id.is_empty() && bare.user_id.is_empty());
    assert_eq!(bare.timeout_ms, DEFAULT_TIMEOUT_MS);
    assert_eq!(bare.preferred_algorithm(), COSE_ES256);
    let zero = CreationOptions::parse(r#"{"publicKey":{"timeout":0}}"#).unwrap();
    assert_eq!(zero.timeout_ms, DEFAULT_TIMEOUT_MS);
    let text = CreationOptions::parse(r#"{"publicKey":{"timeout":"5"}}"#).unwrap();
    assert_eq!(text.timeout_ms, DEFAULT_TIMEOUT_MS);
}

#[test]
fn preferred_algorithm_falls_back_in_tdesktop_order() {
    let with = |algorithms: Vec<i64>| CreationOptions {
        algorithms,
        ..Default::default()
    };
    assert_eq!(with(vec![-257, -8]).preferred_algorithm(), COSE_RS256);
    assert_eq!(with(vec![-8, -257]).preferred_algorithm(), COSE_EDDSA);
    assert_eq!(with(vec![-35]).preferred_algorithm(), COSE_ES256);
}

#[test]
fn parses_request_options_like_tdesktop() {
    let options = RequestOptions::parse(REQUEST).unwrap();
    assert_eq!(options.rp_id, "telegram.org");
    assert_eq!(options.challenge, vec![0, 1, 2, 0xff, 0xef]);
    assert_eq!(
        options.allow_credentials,
        vec![vec![0xaa, 0xbb, 0xcc], vec![0xdd, 0xee, 0xff]]
    );
    assert_eq!(options.user_verification, UserVerification::Required);
    assert_eq!(options.timeout_ms, 60_000);
    let other = RequestOptions::parse(r#"{"publicKey":{"userVerification":"whatever"}}"#).unwrap();
    assert_eq!(other.user_verification, UserVerification::Preferred);
    assert!(other.allow_credentials.is_empty());
    assert_eq!(RequestOptions::parse(r#"{"publicKey":[]}"#), None);
}

#[test]
fn base64_reads_both_alphabets_with_or_without_padding() {
    assert_eq!(decode_base64("-_8"), vec![0xfb, 0xff]);
    assert_eq!(decode_base64("+/8="), vec![0xfb, 0xff]);
    assert_eq!(decode_base64("+/8"), vec![0xfb, 0xff]);
    assert_eq!(decode_base64("AQID\nBA=="), vec![1, 2, 3, 4]);
    assert!(decode_base64("!!!").is_empty());
    assert_eq!(encode_base64url(&[0xfb, 0xff]), "-_8");
}

/// Byte for byte what tdesktop's `SerializeClientDataCreate` writes for
/// the challenge 0xdeadbeef: Qt sorts the object's keys.
#[test]
fn client_data_json_is_tdesktops() {
    let options = CreationOptions::parse(CREATION).unwrap();
    assert_eq!(
        options.client_data_json(),
        r#"{"challenge":"3q2-7w","crossOrigin":false,"origin":"https://telegram.org","type":"webauthn.create"}"#
    );
    let request = RequestOptions::parse(REQUEST).unwrap();
    assert_eq!(
        request.client_data_json(),
        r#"{"challenge":"AAEC_-8","crossOrigin":false,"origin":"https://telegram.org","type":"webauthn.get"}"#
    );
    // And it is valid JSON with the expected values.
    let value: Value = serde_json::from_str(&request.client_data_json()).unwrap();
    assert_eq!(value["crossOrigin"], false);
}
