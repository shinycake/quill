use super::*;

/// Slice A3: `getActiveSessions` answer (schema 1.8.67 lines 9144 /
/// 9147 / 15102) — current, other, and an incomplete attempt
/// (`is_password_pending`) all parse; the `@type` guard and the
/// id-required rule drop malformed entries.
#[test]
fn sessions_parse_current_other_and_password_pending() {
    let json = r#"{"@type":"sessions","inactive_session_ttl_days":180,"sessions":[
{"@type":"session","id":11,"is_current":true,"is_password_pending":false,"is_unconfirmed":false,"can_accept_secret_chats":true,"can_accept_calls":true,"device_type":{"@type":"sessionDeviceTypeDesktop"},"api_id":1,"application_name":"Quill","application_version":"0.1","is_official_application":false,"device_model":"Linux desktop","platform":"Linux","system_version":"6.8","log_in_date":1700000000,"last_active_date":1759000000,"ip_address":"1.2.3.4","location":"Austin, United States"},
{"@type":"session","id":22,"is_current":false,"is_password_pending":false,"device_model":"iPhone","application_name":"Telegram iOS","application_version":"12.0","platform":"iOS","system_version":"18.0","last_active_date":1758900000,"ip_address":"5.6.7.8","location":"Tel Aviv, Israel"},
{"@type":"session","id":33,"is_current":false,"is_password_pending":true,"device_model":"Unknown","application_name":"Telegram Desktop","application_version":"5.0","platform":"Windows","system_version":"11","last_active_date":1758800000,"ip_address":"9.9.9.9","location":""},
{"@type":"bogus","id":44},
{"@type":"session","id":0,"device_model":"Ghost"}
]}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Sessions { sessions } => {
            assert_eq!(sessions.len(), 3);
            let current = sessions.iter().find(|s| s.id == 11).expect("current");
            assert!(current.is_current);
            assert!(!current.is_password_pending);
            assert_eq!(current.device_model, "Linux desktop");
            assert_eq!(current.application_name, "Quill");
            assert_eq!(current.application_version, "0.1");
            assert_eq!(current.platform, "Linux");
            assert_eq!(current.system_version, "6.8");
            assert_eq!(current.ip_address, "1.2.3.4");
            assert_eq!(current.location, "Austin, United States");
            let other = sessions.iter().find(|s| s.id == 22).expect("other");
            assert!(!other.is_current);
            assert!(!other.is_password_pending);
            let pending = sessions.iter().find(|s| s.id == 33).expect("pending");
            assert!(pending.is_password_pending);
            assert_eq!(pending.location, "");
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Slice A3: every constructor the slice relies on must exist
/// verbatim in the pinned schema (1.8.67).
#[test]
fn schema_pins_session_constructors() {
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "session id:int64 is_current:Bool is_password_pending:Bool is_unconfirmed:Bool can_accept_secret_chats:Bool can_accept_calls:Bool device_type:SessionDeviceType api_id:int32 application_name:string application_version:string is_official_application:Bool device_model:string platform:string system_version:string log_in_date:int32 last_active_date:int32 ip_address:string location:string = Session;",
        "sessions sessions:vector<session> inactive_session_ttl_days:int32 = Sessions;",
        "getActiveSessions = Sessions;",
        "terminateSession session_id:int64 = Ok;",
        "terminateAllOtherSessions = Ok;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
}
