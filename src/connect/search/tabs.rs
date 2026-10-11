//! The tab strip of the global search on the driver side: the media tabs
//! on an empty field, later pages, the Channels tab's recommendations and
//! the Posts tab's quota (tdesktop `Suggestions`, `PostsSearch`).
use super::*;
use crate::search_filters::SearchScope;
use crate::search_tabs::SearchTab;
use crate::telegram::requests::{
    SearchMessagesFilters, get_public_post_search_limits, get_recommended_chats,
    search_messages_page, search_public_posts_paid,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// The narrowing of the open search as `searchMessages` arguments.
    pub(super) fn current_message_filters(&self) -> SearchMessagesFilters {
        let search = &self.session.search.search;
        let now = crate::local_time::now_unix();
        SearchMessagesFilters {
            community_id: search.community_filter,
            chat_type: search.filters.chat_type,
            media: search.filters.media,
            min_date: search.filters.date.min_date(now),
            max_date: search.filters.date.max_date(now),
            archived: search.filters.archived,
        }
    }

    /// The tab the open search is on.
    pub(super) fn search_tab(&self) -> SearchTab {
        SearchTab::of(&self.session.search.search.filters)
    }

    pub(super) fn on_media_tab(&self) -> bool {
        self.search_tab().is_media()
    }

    /// A media tab with an empty field lists that kind of message from
    /// every chat (tdesktop's `GlobalMedia` section); skipped while the
    /// list is already there or on its way.
    pub(super) fn browse_media_if_needed(
        &mut self,
    ) -> Result<Option<SearchFlight>, ConnectSendError> {
        let search = &self.session.search.search;
        if search.open
            && search.query.is_empty()
            && !search.recents
            && matches!(
                search.status,
                SearchStatus::Searching
                    | SearchStatus::Ready
                    | SearchStatus::Empty
                    | SearchStatus::Failed
            )
        {
            return Ok(None);
        }
        self.browse_media()
    }

    /// `searchMessages` with the tab's filter and an empty query.
    pub(super) fn browse_media(&mut self) -> Result<Option<SearchFlight>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let search_gen = if self.search_tab().takes_query() {
            self.session.search.search.begin_query("")
        } else {
            self.session.search.search.begin_browse()
        };
        self.session.search.search.begin_public_scope();
        let extra = self
            .session
            .request_search(RequestPurpose::SearchMessages, search_gen);
        let filters = self.current_message_filters();
        match self
            .sender
            .send_json(&search_messages_page(extra, "", "", SEARCH_LIMIT, &filters))
        {
            Ok(()) => Ok(Some(SearchFlight::Media(extra))),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.search.search.accept_messages(Vec::new(), true);
                Err(err)
            }
        }
    }

    /// The next page of the open message list, from the saved
    /// `next_offset`. One page at a time; nothing once the list ended.
    pub fn load_more_search_results(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(offset) = self.session.search.search.tabs.begin_more() else {
            return Ok(None);
        };
        let search_gen = self.session.search.search.generation;
        let query = self.session.search.search.query.trim().to_string();
        let scope = self.session.search.search.filters.scope;
        let tag = crate::search_filters::tag_query(&query).map(str::to_owned);
        let purpose = match (scope, &tag) {
            (SearchScope::PublicPosts, None) => RequestPurpose::SearchPublicPostsMore,
            _ => RequestPurpose::SearchMessagesMore,
        };
        let extra = self.session.request_search(purpose, search_gen);
        let json = match (scope, &tag) {
            (SearchScope::PublicPosts, Some(tag)) => {
                search_public_messages_by_tag(extra, tag, &offset, SEARCH_LIMIT)
            }
            (SearchScope::PublicPosts, None) => {
                search_public_posts(extra, &query, &offset, SEARCH_LIMIT)
            }
            _ => search_messages_page(
                extra,
                &query,
                &offset,
                SEARCH_LIMIT,
                &self.current_message_filters(),
            ),
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.search.search.fail_page();
                Err(err)
            }
        }
    }

    /// The one place Quill spends Stars: a public-posts search for the
    /// price TDLib reported, once the free searches are gone. Only the
    /// user's explicit confirmation reaches it, and it refuses unless the
    /// open query is a plain one whose free search was refused.
    pub fn search_public_posts_paid(&mut self) -> Result<Option<SearchFlight>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let search = &self.session.search.search;
        let query = search.query.trim().to_string();
        let price = search.tabs.limits.map_or(0, |limits| limits.star_count);
        if search.filters.scope != SearchScope::PublicPosts
            || !search.public_limits_exceeded
            || price <= 0
            || query.is_empty()
            || crate::search_filters::tag_query(&query).is_some()
        {
            return Ok(None);
        }
        let search_gen = self.session.search.search.begin_query(&query);
        self.session.search.search.begin_public_scope();
        let extra = self
            .session
            .request_search(RequestPurpose::SearchPublicPosts, search_gen);
        match self.sender.send_json(&search_public_posts_paid(
            extra,
            &query,
            SEARCH_LIMIT,
            price,
        )) {
            Ok(()) => Ok(Some(SearchFlight::PublicPosts(extra))),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.search.search.accept_messages(Vec::new(), true);
                Err(err)
            }
        }
    }

    /// `getRecommendedChats` for the Channels tab, once per session unless
    /// the answer is still on its way.
    pub fn fetch_recommended_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let tabs = &mut self.session.search.search.tabs;
        if tabs.recommended_loading || tabs.recommended_loaded {
            return Ok(None);
        }
        tabs.recommended_loading = true;
        let extra = self
            .session
            .request(RequestPurpose::GetRecommendedChats, None);
        if let Err(err) = self.sender.send_json(&get_recommended_chats(extra)) {
            self.session.requests.take(extra);
            self.session.search.search.tabs.recommended_loading = false;
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// `getPublicPostSearchLimits`: the free searches left, without
    /// spending one.
    pub fn fetch_public_post_limits(
        &mut self,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetPublicPostSearchLimits, None);
        if let Err(err) = self
            .sender
            .send_json(&get_public_post_search_limits(extra, query))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }
}
