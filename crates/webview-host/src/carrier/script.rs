//! What the carrier puts into its page: the bridge script with its
//! settings, the Content-Security-Policy, and the WebKit content rules that
//! enforce the same scope natively (macOS and Linux).

use quill_webview_protocol::carrier::CarrierScope;

/// The secrets and names one carrier's bridge script needs.
pub(super) struct BridgeKeys {
    /// Starts every message the bridge sends the helper.
    pub token: String,
    /// The random global the helper calls into.
    pub hook: String,
    /// Passed with every helper call, so the page cannot make them.
    pub key: String,
}

impl BridgeKeys {
    /// Fresh random keys; `None` when the system has no randomness.
    pub(super) fn random() -> Option<Self> {
        Some(Self {
            token: random_hex(16)?,
            hook: format!("__quillCarrier{}", random_hex(8)?),
            key: random_hex(16)?,
        })
    }
}

/// `bytes` random bytes as lower-case hex.
fn random_hex(bytes: usize) -> Option<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).ok()?;
    Some(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// The page's Content-Security-Policy: everything from the carrier's path
/// prefix, nothing from anywhere else, no frames, no forms. Workers only
/// from `blob:`, which is how the bridge starts every worker; a service
/// worker needs an `https` script, so none can be registered.
pub(super) fn content_policy(scope: &CarrierScope) -> String {
    let page = scope.page_source();
    let socket = scope.socket_source();
    format!(
        "default-src {page}; script-src {page} 'unsafe-inline' 'unsafe-eval' blob:; \
         style-src {page} 'unsafe-inline'; img-src {page} data: blob:; font-src {page} data:; \
         media-src {page} blob:; connect-src {page} {socket}; worker-src blob:; \
         frame-src 'none'; child-src 'none'; object-src 'none'; form-action 'none'; \
         base-uri 'none'"
    )
}

/// A string as a JavaScript literal (JSON, with the two line separators
/// escaped so the script stays one statement on any engine).
pub(super) fn js_string(text: &str) -> String {
    serde_json::to_string(text)
        .unwrap_or_else(|_| "\"\"".into())
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// The bridge script for one carrier. Every value goes in through one
/// JSON object, so no value can be mistaken for a placeholder.
pub(super) fn bridge_script(scope: &CarrierScope, keys: &BridgeKeys) -> String {
    let config = serde_json::json!({
        "origin": scope.origin(),
        "page": scope.page_source(),
        "token": keys.token,
        "policy": content_policy(scope),
        "name": scope.bridge_object_name(),
        "hook": keys.hook,
        "key": keys.key,
    })
    .to_string()
    .replace('\u{2028}', "\\u2028")
    .replace('\u{2029}', "\\u2029");
    include_str!("bridge.js").replace("__CONFIG__", &config)
}

/// A call into the bridge hook, for `evaluate_script`.
pub(super) fn hook_call(keys: &BridgeKeys, method: &str, args: &str) -> String {
    let hook = js_string(&keys.hook);
    let key = js_string(&keys.key);
    let separator = if args.is_empty() { "" } else { ", " };
    format!("window[{hook}] && window[{hook}].{method}({key}{separator}{args});")
}

/// Asks the page for the HTTP status of its document; 0 when the engine
/// does not say (`responseStatus` is newer than some engines).
pub(super) const STATUS_SCRIPT: &str = "(function () { try { \
    var entry = performance.getEntriesByType('navigation')[0]; \
    return entry && typeof entry.responseStatus === 'number' ? entry.responseStatus : 0; \
    } catch (error) { return 0; } })()";

/// `url-filter` regex for URLs starting with `prefix` (WebKit content
/// blockers take a small regex subset; dots are the only special
/// character a validated scope contains).
#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn starts_with(prefix: &str) -> String {
    format!("^{}", prefix.replace('.', "\\."))
}

/// WebKit content rules (Safari content blocker JSON, used by
/// `WKContentRuleList` and `WebKitUserContentFilter`): block every load,
/// then let through the prefix, its WebSocket form and the page's own
/// `data:` and `blob:` URLs. Unlike the policy this also holds after
/// redirects and for loads the policy does not see.
#[cfg(any(target_os = "macos", target_os = "linux", test))]
pub(super) fn content_rules(scope: &CarrierScope) -> String {
    let allow = |filter: String| {
        serde_json::json!({
            "trigger": {"url-filter": filter, "url-filter-is-case-sensitive": true},
            "action": {"type": "ignore-previous-rules"},
        })
    };
    serde_json::json!([
        {"trigger": {"url-filter": ".*"}, "action": {"type": "block"}},
        allow(starts_with(&scope.page_source())),
        allow(starts_with(&scope.socket_source())),
        allow("^data:".into()),
        allow(starts_with(&format!("blob:{}/", scope.origin()))),
    ])
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(prefix: &str) -> CarrierScope {
        CarrierScope::new(
            "https://proxy.example.com",
            prefix,
            "TelegramWebProxy",
            false,
        )
        .unwrap()
    }

    fn keys() -> BridgeKeys {
        BridgeKeys {
            token: "00112233445566778899aabbccddeeff".into(),
            hook: "__quillCarrier0011".into(),
            key: "ffeeddccbbaa99887766554433221100".into(),
        }
    }

    #[test]
    fn the_bridge_script_gets_every_value_as_a_literal() {
        let script = bridge_script(&scope("/relay/"), &keys());
        assert!(!script.contains("__CONFIG__"));
        assert!(script.contains(r#""origin":"https://proxy.example.com""#));
        assert!(script.contains(r#""page":"https://proxy.example.com/relay/""#));
        assert!(script.contains(r#""name":"TelegramWebProxy""#));
        assert!(script.contains(r#""token":"00112233445566778899aabbccddeeff""#));
        assert!(script.contains(r#""key":"ffeeddccbbaa99887766554433221100""#));
    }

    #[test]
    fn a_prefix_that_looks_like_a_placeholder_stays_a_value() {
        // Segments may hold anything in [A-Za-z0-9_-], placeholder-like
        // names included; one JSON object never re-reads what it wrote.
        let script = bridge_script(&scope("/a__CONFIG__/b__NAME__/"), &keys());
        assert!(script.contains("https://proxy.example.com/a__CONFIG__/b__NAME__/;"));
        assert_eq!(script.matches("})({").count(), 1);
    }

    #[test]
    fn hook_calls_carry_the_key() {
        assert_eq!(
            hook_call(&keys(), "probe", ""),
            r#"window["__quillCarrier0011"] && window["__quillCarrier0011"].probe("ffeeddccbbaa99887766554433221100");"#
        );
        assert!(
            hook_call(&keys(), "deliver", "7, 'b', 'AQID'")
                .ends_with(r#".deliver("ffeeddccbbaa99887766554433221100", 7, 'b', 'AQID');"#)
        );
    }

    #[test]
    fn the_policy_limits_everything_to_the_prefix() {
        let policy = content_policy(&scope("/relay/"));
        assert!(policy.starts_with("default-src https://proxy.example.com/relay/;"));
        assert!(policy.contains(
            "connect-src https://proxy.example.com/relay/ wss://proxy.example.com/relay/;"
        ));
        assert!(policy.contains("frame-src 'none'"));
        assert!(policy.contains("worker-src blob:;"));
        assert!(!policy.contains("'self'"));
        assert!(!policy.contains('*'));
    }

    #[test]
    fn content_rules_block_all_but_the_scope() {
        let rules: serde_json::Value =
            serde_json::from_str(&content_rules(&scope("/relay/"))).unwrap();
        let filters: Vec<(&str, &str)> = rules
            .as_array()
            .unwrap()
            .iter()
            .map(|rule| {
                (
                    rule["trigger"]["url-filter"].as_str().unwrap(),
                    rule["action"]["type"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            filters,
            [
                (".*", "block"),
                (
                    r"^https://proxy\.example\.com/relay/",
                    "ignore-previous-rules"
                ),
                (
                    r"^wss://proxy\.example\.com/relay/",
                    "ignore-previous-rules"
                ),
                ("^data:", "ignore-previous-rules"),
                (
                    r"^blob:https://proxy\.example\.com/",
                    "ignore-previous-rules"
                ),
            ]
        );
    }
}
