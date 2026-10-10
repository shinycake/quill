use super::*;
use serde_json::json;

fn route_of(value: serde_json::Value) -> LinkRoute {
    route(&parse_internal_link(&value).expect("parsed"), "")
}

#[test]
fn app_links_resolve_the_bot_first() {
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeWebApp","bot_username":"shopbot","web_app_short_name":"shop","start_parameter":"ref1","mode":{"@type":"webAppOpenModeCompact"}})
        ),
        LinkRoute::Resolve(DeepLinkAction::OpenWebAppLink {
            domain: "shopbot".into(),
            short_name: "shop".into(),
            start_parameter: "ref1".into(),
        })
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeMainWebApp","bot_username":"shopbot","start_parameter":"","mode":{"@type":"webAppOpenModeFullSize"}})
        ),
        LinkRoute::Resolve(DeepLinkAction::OpenMainWebApp {
            domain: "shopbot".into(),
            start_parameter: String::new(),
        })
    );
    assert_eq!(
        route_of(
            json!({"@type":"internalLinkTypeAttachmentMenuBot","bot_username":"shopbot","url":"https://a/","target_chat":{"@type":"targetChatCurrent"}})
        ),
        LinkRoute::Resolve(DeepLinkAction::OpenAttachmentBot {
            domain: "shopbot".into(),
            url: "https://a/".into(),
        })
    );
    for broken in [
        json!({"@type":"internalLinkTypeWebApp","bot_username":"","web_app_short_name":"shop"}),
        json!({"@type":"internalLinkTypeWebApp","bot_username":"b","web_app_short_name":""}),
        json!({"@type":"internalLinkTypeMainWebApp","bot_username":""}),
        json!({"@type":"internalLinkTypeAttachmentMenuBot","bot_username":""}),
    ] {
        assert!(matches!(route_of(broken), LinkRoute::Message(_)));
    }
}
