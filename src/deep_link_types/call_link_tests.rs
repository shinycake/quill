use super::{InternalLink, LinkRoute, parse_internal_link, route};
use crate::state::DeepLinkAction;
use serde_json::{Value, json};

fn parsed(value: Value) -> InternalLink {
    parse_internal_link(&value).expect("internal link")
}

fn route_of(value: Value) -> LinkRoute {
    route(&parsed(value), "https://t.me/g?voicechat=h")
}

#[test]
fn voice_video_and_live_links_keep_their_hash() {
    assert_eq!(
        route_of(json!({
            "@type":"internalLinkTypeVideoChat",
            "chat_username":"group",
            "invite_hash":"AbC",
            "is_live_stream":false
        })),
        LinkRoute::Resolve(DeepLinkAction::OpenVideoChat {
            domain: "group".into(),
            invite_hash: "AbC".into(),
            live_stream: false,
        })
    );
    assert_eq!(
        route_of(json!({
            "@type":"internalLinkTypeVideoChat",
            "chat_username":"news",
            "invite_hash":"",
            "is_live_stream":true
        })),
        LinkRoute::Resolve(DeepLinkAction::OpenVideoChat {
            domain: "news".into(),
            invite_hash: String::new(),
            live_stream: true,
        })
    );
}

#[test]
fn a_call_link_without_a_chat_is_broken() {
    assert_eq!(
        route_of(json!({
            "@type":"internalLinkTypeVideoChat",
            "chat_username":"",
            "invite_hash":"x",
            "is_live_stream":true
        })),
        LinkRoute::Message("This live stream link is broken.".into())
    );
    assert_eq!(
        route_of(json!({
            "@type":"internalLinkTypeVideoChat",
            "chat_username":"",
            "invite_hash":"x",
            "is_live_stream":false
        })),
        LinkRoute::Message("This video chat link is broken.".into())
    );
}
