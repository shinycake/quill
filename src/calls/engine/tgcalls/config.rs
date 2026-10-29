//! TDLib's `callStateReady` as `qc_p2p_create`'s JSON config
//! (`native/quill-calls/include/quill_calls.h`). The library maps it onto
//! tgcalls the way Telegram Desktop maps `phoneCall` (calls_call.cpp,
//! `Call::createAndStartController`); this side only carries it over.
use super::super::{ConnectParams, RtcServer};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

/// TDLib's defaults for the `call_connect_timeout_ms` and
/// `call_packet_timeout_ms` options, which tdesktop passes as the call's
/// timeouts.
pub(super) const CONNECT_TIMEOUT_MS: i64 = 30_000;
pub(super) const PACKET_TIMEOUT_MS: i64 = 10_000;

/// Why a `ConnectParams` can't start a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConfigError {
    /// TDLib agreed on no protocol version.
    NoVersion,
    /// The key is not the 256 bytes tgcalls takes.
    KeyLength(usize),
}

/// The config JSON for `params`, speaking `params.library_versions[0]`
/// (the version TDLib agreed on) up to `max_layer`.
pub(super) fn p2p_config(params: &ConnectParams, max_layer: i32) -> Result<String, ConfigError> {
    let version = params
        .library_versions
        .first()
        .ok_or(ConfigError::NoVersion)?;
    if params.encryption_key.len() != 256 {
        return Err(ConfigError::KeyLength(params.encryption_key.len()));
    }
    let mut config = json!({
        "version": version,
        "outgoing": params.is_outgoing,
        "encryption_key": STANDARD.encode(&params.encryption_key),
        "allow_p2p": params.p2p_allowed,
        "max_layer": max_layer,
        "connect_timeout_ms": CONNECT_TIMEOUT_MS,
        "packet_timeout_ms": PACKET_TIMEOUT_MS,
        "servers": params.servers.iter().map(server).collect::<Vec<_>>(),
    });
    if !params.custom_parameters.is_empty()
        && let Some(object) = config.as_object_mut()
    {
        object.insert(
            "custom_parameters".to_owned(),
            Value::String(params.custom_parameters.clone()),
        );
    }
    Ok(config.to_string())
}

/// One `callServer`: a Telegram reflector carries a peer tag
/// (`callServerTypeTelegramReflector`), anything else is WebRTC
/// (`callServerTypeWebrtc`).
fn server(server: &RtcServer) -> Value {
    let kind = if server.peer_tag.is_empty() {
        (
            "webrtc",
            json!({
                "username": server.username,
                "password": server.password,
                "turn": server.turn,
                "stun": server.stun,
            }),
        )
    } else {
        (
            "reflector",
            json!({
                "peer_tag": STANDARD.encode(&server.peer_tag),
                "tcp": server.tcp,
            }),
        )
    };
    json!({
        "id": server.id.to_string(),
        "ipv4": server.ipv4,
        "ipv6": server.ipv6,
        "port": server.port,
        kind.0: kind.1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> ConnectParams {
        ConnectParams {
            encryption_key: vec![7; 256],
            custom_parameters: String::new(),
            is_outgoing: true,
            servers: vec![
                RtcServer {
                    id: 1,
                    ipv4: "1.2.3.4".to_owned(),
                    ipv6: String::new(),
                    port: 1400,
                    username: String::new(),
                    password: String::new(),
                    turn: false,
                    stun: false,
                    tcp: true,
                    peer_tag: vec![9; 16],
                },
                RtcServer {
                    id: 9_000_000_000,
                    ipv4: "5.6.7.8".to_owned(),
                    ipv6: "::1".to_owned(),
                    port: 3478,
                    username: "user".to_owned(),
                    password: "pass".to_owned(),
                    turn: true,
                    stun: true,
                    tcp: false,
                    peer_tag: Vec::new(),
                },
            ],
            library_versions: vec!["11.0.0".to_owned(), "9.0.0".to_owned()],
            p2p_allowed: false,
            mic_input: None,
            speaker_input: None,
            camera_input: None,
        }
    }

    fn parsed(params: &ConnectParams) -> Value {
        serde_json::from_str(&p2p_config(params, 92).unwrap()).unwrap()
    }

    #[test]
    fn the_agreed_version_key_and_flags_carry_over() {
        let config = parsed(&params());
        assert_eq!(config["version"], "11.0.0");
        assert_eq!(config["outgoing"], true);
        assert_eq!(config["allow_p2p"], false);
        assert_eq!(config["max_layer"], 92);
        assert_eq!(config["connect_timeout_ms"], 30_000);
        assert_eq!(config["packet_timeout_ms"], 10_000);
        let key = STANDARD
            .decode(config["encryption_key"].as_str().unwrap())
            .unwrap();
        assert_eq!(key, vec![7; 256]);
        assert!(config.get("custom_parameters").is_none());
    }

    #[test]
    fn reflectors_and_webrtc_servers_keep_their_kind() {
        let config = parsed(&params());
        let reflector = &config["servers"][0];
        assert_eq!(reflector["id"], "1");
        assert_eq!(reflector["port"], 1400);
        assert_eq!(reflector["reflector"]["tcp"], true);
        assert_eq!(
            STANDARD
                .decode(reflector["reflector"]["peer_tag"].as_str().unwrap())
                .unwrap(),
            vec![9; 16]
        );
        assert!(reflector.get("webrtc").is_none());

        let webrtc = &config["servers"][1];
        assert_eq!(webrtc["id"], "9000000000");
        assert_eq!(webrtc["ipv6"], "::1");
        assert_eq!(webrtc["webrtc"]["username"], "user");
        assert_eq!(webrtc["webrtc"]["password"], "pass");
        assert_eq!(webrtc["webrtc"]["turn"], true);
        assert_eq!(webrtc["webrtc"]["stun"], true);
        assert!(webrtc.get("reflector").is_none());
    }

    #[test]
    fn custom_parameters_pass_through_as_text() {
        let mut params = params();
        params.custom_parameters = r#"{"network_use_tcponly":true}"#.to_owned();
        assert_eq!(
            parsed(&params)["custom_parameters"],
            r#"{"network_use_tcponly":true}"#
        );
    }

    #[test]
    fn a_call_needs_a_version_and_a_full_key() {
        let mut params = params();
        params.encryption_key.truncate(255);
        assert_eq!(p2p_config(&params, 92), Err(ConfigError::KeyLength(255)));
        params.library_versions.clear();
        assert_eq!(p2p_config(&params, 92), Err(ConfigError::NoVersion));
    }
}
