//! Story statistics, public forwards and public story search reducer tests.
use super::common::*;
use crate::ids::ChatId;
use crate::state::{RequestPurpose, StorySearchQuery, StoryStatsFetch};

const GRAPH_DATA: &str = r#"{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""}"#;

#[test]
fn statistics_and_forwards_land_for_the_open_story_only() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_story_insights(11, 5);
    let stats = session.request_for_story(RequestPurpose::GetStoryStatistics, ChatId(11), 5);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"storyStatistics","@extra":"{}","story_interaction_graph":{GRAPH_DATA},"story_reaction_graph":{GRAPH_DATA}}}"#,
            stats.0
        ),
    );
    let state = session.stories.insights.as_ref().unwrap();
    assert!(matches!(state.statistics, StoryStatsFetch::Loaded(_)));

    let page = |extra: u64, offset: &str| {
        format!(
            r#"{{"@type":"publicForwards","@extra":"{extra}","total_count":3,"next_offset":"{offset}","forwards":[{{"@type":"publicForwardMessage","message":{{"id":40,"chat_id":-100,"date":9}}}}]}}"#
        )
    };
    let first = session.request_for_story(RequestPurpose::GetStoryPublicForwards, ChatId(11), 5);
    session.story_forwards_sent(11, 5, first);
    apply_json(&mut session, &seq, &sink, &page(first.0, "n"));
    let second = session.request_for_story(RequestPurpose::GetStoryPublicForwards, ChatId(11), 5);
    session.story_forwards_sent(11, 5, second);
    apply_json(&mut session, &seq, &sink, &page(second.0, ""));
    let state = session.stories.insights.as_ref().unwrap();
    assert_eq!(state.forwards.len(), 2);
    assert_eq!(state.forwards_total, 3);
    assert!(state.forwards_next_offset.is_empty());

    // The viewer moved to another story: a late answer is dropped.
    session.begin_story_insights(11, 6);
    let late = session.request_for_story(RequestPurpose::GetStoryPublicForwards, ChatId(11), 5);
    apply_json(&mut session, &seq, &sink, &page(late.0, "z"));
    assert!(
        session
            .stories
            .insights
            .as_ref()
            .unwrap()
            .forwards
            .is_empty()
    );
}

#[test]
fn a_forwards_page_from_an_earlier_visit_is_not_appended() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let page = |extra: u64| {
        format!(
            r#"{{"@type":"publicForwards","@extra":"{extra}","total_count":3,"next_offset":"n","forwards":[{{"@type":"publicForwardMessage","message":{{"id":40,"chat_id":-100,"date":9}}}}]}}"#
        )
    };
    session.begin_story_insights(11, 5);
    let old = session.request_for_story(RequestPurpose::GetStoryPublicForwards, ChatId(11), 5);
    session.story_forwards_sent(11, 5, old);
    // Another story and back: the new visit starts clean and asks again.
    session.begin_story_insights(11, 6);
    session.begin_story_insights(11, 5);
    let fresh = session.request_for_story(RequestPurpose::GetStoryPublicForwards, ChatId(11), 5);
    session.story_forwards_sent(11, 5, fresh);

    apply_json(&mut session, &seq, &sink, &page(old.0));
    let state = session.stories.insights.as_ref().unwrap();
    assert!(state.forwards.is_empty(), "the old page lands in nothing");
    assert!(
        state.forwards_page.is_loading(),
        "still waiting for its own"
    );

    apply_json(&mut session, &seq, &sink, &page(fresh.0));
    let state = session.stories.insights.as_ref().unwrap();
    assert_eq!(state.forwards.len(), 1);
    assert!(!state.forwards_page.is_loading());
}

#[test]
fn statistics_error_is_kept_for_the_panel() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_story_insights(11, 5);
    let stats = session.request_for_story(RequestPurpose::GetStoryStatistics, ChatId(11), 5);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"STATS_NOT_AVAILABLE"}}"#,
            stats.0
        ),
    );
    let state = session.stories.insights.as_ref().unwrap();
    assert!(matches!(state.statistics, StoryStatsFetch::Failed(_)));
}

#[test]
fn found_stories_fill_the_cache_and_page() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_story_search(StorySearchQuery::Tag("#sunset".into()));
    let story = |id: i32, chat: i64| {
        format!(
            r#"{{"@type":"story","id":{id},"poster_chat_id":{chat},"date":1,"content":{{"@type":"storyContentUnsupported"}}}}"#
        )
    };
    let extra = session.request(RequestPurpose::SearchPublicStories, None);
    session.story_search_sent(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundStories","@extra":"{}","total_count":3,"next_offset":"p2","stories":[{},{}]}}"#,
            extra.0,
            story(1, 21),
            story(2, 22)
        ),
    );
    let search = session.stories.search.as_ref().unwrap();
    assert_eq!(search.stories, vec![(21, 1), (22, 2)]);
    assert_eq!(search.next_offset, "p2");
    assert!(!search.page.is_loading());
    assert!(session.stories.stories.contains_key(&(22, 2)));

    session.story_search_page_requested();
    let extra = session.request(RequestPurpose::SearchPublicStories, None);
    session.story_search_sent(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundStories","@extra":"{}","total_count":3,"next_offset":"","stories":[{},{}]}}"#,
            extra.0,
            story(2, 22),
            story(3, 23)
        ),
    );
    let search = session.stories.search.as_ref().unwrap();
    assert_eq!(search.stories, vec![(21, 1), (22, 2), (23, 3)]);
    assert!(search.next_offset.is_empty());
}

#[test]
fn search_error_lands_in_the_panel() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.begin_story_search(StorySearchQuery::Tag("#x".into()));
    let extra = session.request(RequestPurpose::SearchPublicStories, None);
    session.story_search_sent(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"TAG_INVALID"}}"#,
            extra.0
        ),
    );
    let search = session.stories.search.as_ref().unwrap();
    assert!(search.page.error().is_some());
}
