//! Proxy request builders (`parity:proxy-settings`), schema 1.8.67
//! (`schema/td_api.tl` :16202–16229): `getProxies`, `addProxy`,
//! `editProxy`, `enableProxy`, `disableProxy`, `removeProxy`,
//! `pingProxy`, plus `setOption("prefer_ipv6")`. All of them may be
//! called before authorization.

use crate::ids::RequestId;
use crate::proxy::ProxyDraft;
use serde_json::json;

pub fn get_proxies(extra: RequestId) -> String {
    json!({"@type": "getProxies", "@extra": extra.as_extra()}).to_string()
}

/// `addProxy proxy:proxy enable:Bool comment:string = AddedProxy`.
pub fn add_proxy(extra: RequestId, proxy: &ProxyDraft, enable: bool, comment: &str) -> String {
    json!({
        "@type": "addProxy",
        "@extra": extra.as_extra(),
        "proxy": proxy.to_json(),
        "enable": enable,
        "comment": comment,
    })
    .to_string()
}

/// `editProxy proxy_id:int32 proxy:proxy enable:Bool comment:string =
/// AddedProxy`.
pub fn edit_proxy(
    extra: RequestId,
    proxy_id: i32,
    proxy: &ProxyDraft,
    enable: bool,
    comment: &str,
) -> String {
    json!({
        "@type": "editProxy",
        "@extra": extra.as_extra(),
        "proxy_id": proxy_id,
        "proxy": proxy.to_json(),
        "enable": enable,
        "comment": comment,
    })
    .to_string()
}

pub fn enable_proxy(extra: RequestId, proxy_id: i32) -> String {
    json!({"@type": "enableProxy", "@extra": extra.as_extra(), "proxy_id": proxy_id}).to_string()
}

pub fn disable_proxy(extra: RequestId) -> String {
    json!({"@type": "disableProxy", "@extra": extra.as_extra()}).to_string()
}

pub fn remove_proxy(extra: RequestId, proxy_id: i32) -> String {
    json!({"@type": "removeProxy", "@extra": extra.as_extra(), "proxy_id": proxy_id}).to_string()
}

/// `pingProxy proxy:proxy = Seconds`. Reveals the user's IP to the proxy
/// (tdesktop warns once before the first check).
pub fn ping_proxy(extra: RequestId, proxy: &ProxyDraft) -> String {
    json!({"@type": "pingProxy", "@extra": extra.as_extra(), "proxy": proxy.to_json()}).to_string()
}

/// `reportWebProxyCarrierError carrier_id:int32 error:WebProxyCarrierError =
/// Ok`.
pub fn report_web_proxy_carrier_error(
    extra: RequestId,
    carrier_id: i32,
    error: &crate::proxy_carrier::CarrierError,
) -> String {
    json!({
        "@type": "reportWebProxyCarrierError",
        "@extra": extra.as_extra(),
        "carrier_id": carrier_id,
        "error": error.to_json(),
    })
    .to_string()
}

/// `processWebProxyCarrierMessage carrier_id:int32
/// message:WebProxyCarrierMessage = Ok`: what the page posted to its bridge
/// object, binary data as base64 (`bytes`).
pub fn process_web_proxy_carrier_message(
    extra: RequestId,
    carrier_id: i32,
    message: &crate::proxy_carrier::CarrierMessage,
) -> String {
    use crate::proxy_carrier::CarrierMessage;
    let message = match message {
        CarrierMessage::Binary(data) => {
            json!({"@type": "webProxyCarrierMessageBinary", "data": data})
        }
        CarrierMessage::Text(text) => json!({"@type": "webProxyCarrierMessageText", "text": text}),
    };
    json!({
        "@type": "processWebProxyCarrierMessage",
        "@extra": extra.as_extra(),
        "carrier_id": carrier_id,
        "message": message,
    })
    .to_string()
}

/// `setOption("prefer_ipv6", optionValueBoolean)`.
pub fn set_prefer_ipv6(extra: RequestId, on: bool) -> String {
    json!({
        "@type": "setOption",
        "@extra": extra.as_extra(),
        "name": "prefer_ipv6",
        "value": {"@type": "optionValueBoolean", "value": on},
    })
    .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkType {
    /// Nothing to connect through: TDLib stops trying.
    None,
    /// A network, which may be a new one: TDLib drops its connections.
    Other,
}

pub fn set_network_type(extra: RequestId, network: NetworkType) -> String {
    let kind = match network {
        NetworkType::None => "networkTypeNone",
        NetworkType::Other => "networkTypeOther",
    };
    json!({
        "@type": "setNetworkType",
        "@extra": extra.as_extra(),
        "type": {"@type": kind},
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use crate::calls::proxy::ProxyKind;
    use crate::ids::RequestId;
    use crate::proxy::ProxyDraft;
    use crate::telegram::requests::{
        NetworkType, add_proxy, disable_proxy, edit_proxy, enable_proxy, get_proxies, ping_proxy,
        remove_proxy, set_network_type, set_prefer_ipv6,
    };
    use serde_json::Value;

    fn socks() -> ProxyDraft {
        let mut p = ProxyDraft::empty(ProxyKind::Socks5);
        p.server = "proxy.example".into();
        p.port = 1080;
        p.username = "u".into();
        p
    }

    fn parse(json: String) -> Value {
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn add_and_edit_carry_the_proxy_object() {
        let add = parse(add_proxy(RequestId(5), &socks(), true, "work"));
        assert_eq!(add["@type"], "addProxy");
        assert_eq!(add["enable"], true);
        assert_eq!(add["comment"], "work");
        assert_eq!(add["proxy"]["type"]["@type"], "proxyTypeSocks5");
        assert_eq!(add["proxy"]["type"]["username"], "u");
        let edit = parse(edit_proxy(RequestId(6), 3, &socks(), false, ""));
        assert_eq!(edit["@type"], "editProxy");
        assert_eq!(edit["proxy_id"], 3);
        assert_eq!(edit["proxy"]["server"], "proxy.example");
    }

    #[test]
    fn simple_requests_use_the_schema_names() {
        assert_eq!(parse(get_proxies(RequestId(1)))["@type"], "getProxies");
        assert_eq!(parse(disable_proxy(RequestId(1)))["@type"], "disableProxy");
        let enable = parse(enable_proxy(RequestId(1), 9));
        assert_eq!(enable["@type"], "enableProxy");
        assert_eq!(enable["proxy_id"], 9);
        let remove = parse(remove_proxy(RequestId(1), 9));
        assert_eq!(remove["@type"], "removeProxy");
        assert_eq!(remove["proxy_id"], 9);
        assert_eq!(
            parse(ping_proxy(RequestId(1), &socks()))["proxy"]["port"],
            1080
        );
    }

    #[test]
    fn prefer_ipv6_is_a_boolean_option() {
        let json = parse(set_prefer_ipv6(RequestId(2), true));
        assert_eq!(json["@type"], "setOption");
        assert_eq!(json["name"], "prefer_ipv6");
        assert_eq!(json["value"]["@type"], "optionValueBoolean");
        assert_eq!(json["value"]["value"], true);
    }

    #[test]
    fn network_type_names_tdlib_s_constructor() {
        let value: Value =
            serde_json::from_str(&set_network_type(RequestId(7), NetworkType::None)).unwrap();
        assert_eq!(value["@type"], "setNetworkType");
        assert_eq!(value["@extra"], "7");
        assert_eq!(value["type"]["@type"], "networkTypeNone");
        let value: Value =
            serde_json::from_str(&set_network_type(RequestId(8), NetworkType::Other)).unwrap();
        assert_eq!(value["type"]["@type"], "networkTypeOther");
    }
}
