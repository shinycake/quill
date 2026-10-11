//! Requests of the global search tabs: a page of `searchMessages`, the
//! Channels tab's recommendations and the Posts tab's quota check.
use super::SearchMessagesFilters;
use crate::ids::RequestId;
use serde_json::{Value, json};

/// `searchMessages` (schema: `searchMessages chat_list:ChatList query:string ...`) with the narrowing of the
/// search bar and the `offset` of a later page (`""` = the first one).
pub fn search_messages_page(
    extra: RequestId,
    query: &str,
    offset: &str,
    limit: i32,
    filters: &SearchMessagesFilters,
) -> String {
    let chat_type_filter = match (filters.community_id, filters.chat_type.constructor()) {
        (Some(id), _) => json!({
            "@type": "searchMessagesChatTypeFilterCommunity",
            "community_id": id,
        }),
        (None, Some(constructor)) => json!({ "@type": constructor }),
        (None, None) => Value::Null,
    };
    let filter = filters
        .media
        .constructor()
        .map_or(Value::Null, super::search_messages_filter_json);
    json!({
        "@type": "searchMessages",
        "@extra": extra.as_extra(),
        "chat_list": if filters.archived {
            json!({ "@type": "chatListArchive" })
        } else {
            Value::Null
        },
        "query": query,
        "offset": offset,
        "limit": limit,
        "filter": filter,
        "chat_type_filter": chat_type_filter,
        "min_date": filters.min_date,
        "max_date": filters.max_date,
    })
    .to_string()
}

/// `getRecommendedChats = Chats;` (schema line 12003): channels recommended
/// to the user, the "Recommended channels" of the Channels tab.
pub fn get_recommended_chats(extra: RequestId) -> String {
    json!({
        "@type": "getRecommendedChats",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `getPublicPostSearchLimits query:string = PublicPostSearchLimits;`
/// (schema line 12303): "Checks public post search limits without actually
/// performing the search", so asking never spends a search.
pub fn get_public_post_search_limits(extra: RequestId, query: &str) -> String {
    json!({
        "@type": "getPublicPostSearchLimits",
        "@extra": extra.as_extra(),
        "query": query,
    })
    .to_string()
}

/// `searchPublicPosts` that pays `star_count` Stars (`star_count:int53`,
/// schema line 12310: "the Telegram Star amount the user agreed to pay for
/// the search"). Only the confirmed paid search uses it; every other
/// public-posts request is [`super::search_public_posts`] with 0.
pub fn search_public_posts_paid(
    extra: RequestId,
    query: &str,
    limit: i32,
    star_count: i64,
) -> String {
    json!({
        "@type": "searchPublicPosts",
        "@extra": extra.as_extra(),
        "query": query,
        "offset": "",
        "limit": limit,
        "star_count": star_count,
    })
    .to_string()
}

#[cfg(test)]
#[path = "tests_search_tabs.rs"]
mod tests;
