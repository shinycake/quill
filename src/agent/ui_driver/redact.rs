//! Cutting the element tree to what the grants allow. The UI tags the
//! containers it draws (`a11y_tag`): the regions of the window, the parts
//! of the chat list, the lists whose rows stand for chats, and every row
//! that stands for a chat. The cut reads those tags only, never a label,
//! so no text in the window can widen it.
//!
//! With [`DriverScope::Granted`] the tree keeps the chat list's search
//! field, folder tabs and rows, the open chat, the forum column and any
//! context menu; rows of ungranted chats go, and so does everything else.
//! With [`DriverScope::Everything`] the whole tree stays, less the rows of
//! chats that still have no grant (secret chats, blocked ones).
//!
//! Lists are default-deny. A container tagged [`LIST`] (the chat rows
//! and search results parts count as one) holds rows that stand for
//! chats or for messages of chats: chat rows, search hits, media hits,
//! downloads, reply and forward destinations, contacts. Inside it, in the
//! granted scope, a node reaches the agent only under a `chat=<id>` tag of
//! a granted chat, or under a [`CONTROLS`] tag (a filter strip, a "Load
//! more" button: controls that name no chat). Everything else in a list
//! (a row that forgot its tag, a heading, a "remove from recent" button
//! whose label names the chat) is cut like a region outside the grants:
//! kept nameless when it is an ancestor of something allowed, gone
//! otherwise. So a new row leaks nothing until it says which chat it is.

use super::surface::DriverScope;
use super::tree::{UiNode, UiTree};
use std::collections::{HashMap, HashSet};

/// The chat list column.
pub const REGION_CHAT_LIST: &str = "region=chat-list";
/// The open chat: history, composer and header.
pub const REGION_CHAT: &str = "region=chat";
/// The forum topics column of the open chat.
pub const REGION_FORUM: &str = "region=forum";
/// A context menu's panel.
pub const REGION_MENU: &str = "region=menu";
/// The side panels (chat info, shared media), allowed only with All chats.
pub const REGION_PANELS: &str = "region=panels";
/// The "Archived chats" row, whose preview names archived chats.
pub const ARCHIVE: &str = "archive";
/// Parts of the chat list the granted scope shows.
pub const PART_SEARCH: &str = "part=search";
pub const PART_FOLDERS: &str = "part=folders";
/// The chat rows and the search results: lists (see [`LIST`]).
pub const PART_ROWS: &str = "part=rows";
pub const PART_RESULTS: &str = "part=search-results";
/// A container of rows that stand for chats, anywhere in the window: in
/// the granted scope only `chat=` rows of granted chats and [`CONTROLS`]
/// survive inside it.
pub const LIST: &str = "list";
/// Controls inside a list that name no chat (a filter strip, paging).
pub const CONTROLS: &str = "controls";

/// The tag of a row that stands for `chat_id` (a chat row, a search hit, a
/// contact: a private chat's id is its user's id).
pub fn chat_tag(chat_id: i64) -> String {
    format!("chat={chat_id}")
}

/// The chat a `chat=<id>` tag names.
pub fn chat_of_tag(tag: &str) -> Option<i64> {
    tag.strip_prefix("chat=")?.parse().ok()
}

/// Where a node stands once the tags above it are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zone {
    /// Not in any allowed region: kept only as a nameless ancestor.
    Outside,
    /// Shown as it is.
    Allowed,
    /// Inside the chat list, outside its allowed parts.
    ChatList,
    /// Inside a list of chat rows, under no granted `chat=` or controls
    /// tag: kept only as a nameless ancestor, like [`Zone::Outside`].
    List,
    /// A chat without a grant: gone with its whole subtree.
    Pruned,
}

fn zone_below(
    parent: Zone,
    tag: Option<&str>,
    scope: DriverScope,
    granted: &dyn Fn(i64) -> bool,
) -> Zone {
    if parent == Zone::Pruned {
        return Zone::Pruned;
    }
    let Some(tag) = tag else {
        return parent;
    };
    if let Some(chat_id) = chat_of_tag(tag) {
        return match (granted(chat_id), parent) {
            (false, _) => Zone::Pruned,
            // A granted chat's row is what a list is allowed to show.
            (true, Zone::List) => Zone::Allowed,
            (true, parent) => parent,
        };
    }
    if tag == ARCHIVE {
        return match scope {
            DriverScope::Everything => parent,
            DriverScope::Granted => Zone::Pruned,
        };
    }
    if scope == DriverScope::Everything {
        return parent;
    }
    match tag {
        REGION_CHAT | REGION_FORUM | REGION_MENU => Zone::Allowed,
        REGION_CHAT_LIST => Zone::ChatList,
        REGION_PANELS => Zone::Outside,
        PART_SEARCH | PART_FOLDERS if parent == Zone::ChatList => Zone::Allowed,
        PART_ROWS | PART_RESULTS if parent == Zone::ChatList => Zone::List,
        // A list where content would otherwise show is default-deny; one
        // in a zone that shows nothing changes nothing.
        LIST if parent == Zone::Allowed => Zone::List,
        CONTROLS if parent == Zone::List => Zone::Allowed,
        _ => parent,
    }
}

/// `tree` cut to what `scope` and the grants allow. `granted` says whether
/// a chat has any grant.
pub fn redact(tree: UiTree, scope: DriverScope, granted: &dyn Fn(i64) -> bool) -> UiTree {
    let parents: HashMap<String, String> = tree
        .nodes
        .iter()
        .flat_map(|node| {
            node.children
                .iter()
                .map(move |child| (child.clone(), node.id.clone()))
        })
        .collect();
    let root_zone = match scope {
        DriverScope::Granted => Zone::Outside,
        DriverScope::Everything => Zone::Allowed,
    };
    // Walk down from the root, so a parent's zone is known before its
    // children's (the snapshot lists children first).
    let by_id: HashMap<&str, &UiNode> = tree
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut zones: HashMap<&str, Zone> = HashMap::new();
    let mut stack: Vec<(&str, Zone)> = vec![(tree.root.as_str(), root_zone)];
    while let Some((id, parent)) = stack.pop() {
        let Some(node) = by_id.get(id) else {
            continue;
        };
        if zones.contains_key(id) {
            continue;
        }
        let zone = zone_below(parent, node.tag.as_deref(), scope, granted);
        zones.insert(id, zone);
        stack.extend(node.children.iter().map(|child| (child.as_str(), zone)));
    }
    let visible: HashSet<&str> = zones
        .iter()
        .filter(|(_, zone)| **zone == Zone::Allowed)
        .map(|(id, _)| *id)
        .collect();
    // Ancestors of visible nodes stay, nameless, so the tree keeps its shape.
    let mut kept: HashSet<&str> = visible.clone();
    for id in &visible {
        let mut current = parents.get(*id);
        while let Some(parent) = current {
            if !kept.insert(parent.as_str()) {
                break;
            }
            current = parents.get(parent);
        }
    }
    let nodes = tree
        .nodes
        .iter()
        .filter(|node| kept.contains(node.id.as_str()))
        .map(|node| {
            let mut node = node.clone();
            if !visible.contains(node.id.as_str()) {
                node.name = None;
                node.value = None;
                node.actions.clear();
            }
            node.children.retain(|child| kept.contains(child.as_str()));
            node
        })
        .collect();
    let focus = tree.focus.filter(|focus| visible.contains(focus.as_str()));
    UiTree {
        nodes,
        focus,
        ..tree
    }
}

/// Whether a pointer over the elements tagged `tags` (topmost first, as
/// GPUI hit-tests them) may act: with the granted scope it must be over an
/// allowed region, inside a list over a granted chat's row or a control,
/// and in every scope never over an ungranted chat's row. The same rule
/// decides a key going to the focused element, from the tags above it.
pub fn point_allowed(tags: &[String], scope: DriverScope, granted: &dyn Fn(i64) -> bool) -> bool {
    if tags
        .iter()
        .filter_map(|tag| chat_of_tag(tag))
        .any(|chat_id| !granted(chat_id))
    {
        return false;
    }
    if scope == DriverScope::Everything {
        return true;
    }
    let mut in_part = false;
    // A granted chat's row or a control was seen below the list.
    let mut in_row = false;
    for tag in tags {
        match tag.as_str() {
            ARCHIVE => return false,
            CONTROLS => in_row = true,
            tag if chat_of_tag(tag).is_some() => in_row = true,
            LIST if !in_row => return false,
            LIST => {}
            PART_ROWS | PART_RESULTS if !in_row => return false,
            PART_SEARCH | PART_FOLDERS | PART_ROWS | PART_RESULTS => in_part = true,
            REGION_CHAT | REGION_FORUM | REGION_MENU => return true,
            REGION_CHAT_LIST => return in_part,
            REGION_PANELS => return false,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, name: &str, tag: Option<&str>, children: &[&str]) -> UiNode {
        UiNode {
            id: id.into(),
            role: "Group".into(),
            name: Some(name.to_string()),
            tag: tag.map(str::to_string),
            children: children.iter().map(|c| c.to_string()).collect(),
            ..UiNode::default()
        }
    }

    /// A window: a chat list with search, rows for chats 1 and 2 and an
    /// archive row; a chat region; a Settings button outside both.
    fn window() -> UiTree {
        UiTree {
            root: "w".into(),
            focus: Some("settings".into()),
            nodes: vec![
                node("w", "Quill", None, &["list", "chat", "settings"]),
                node(
                    "list",
                    "",
                    Some(REGION_CHAT_LIST),
                    &["search", "rows", "archive", "menu-button"],
                ),
                node("search", "Search", Some(PART_SEARCH), &[]),
                node("rows", "", Some(PART_ROWS), &["row1", "row2"]),
                node("row1", "Ann — hi", Some("chat=1"), &["row1-badge"]),
                node("row1-badge", "2 unread", None, &[]),
                node("row2", "Bob — secret", Some("chat=2"), &[]),
                node("archive", "Archived chats — Bob", Some(ARCHIVE), &[]),
                node("menu-button", "Main menu", None, &[]),
                node("chat", "", Some(REGION_CHAT), &["composer"]),
                node("composer", "Message", None, &[]),
                node("settings", "Settings", None, &[]),
            ],
            ..UiTree::default()
        }
    }

    /// Search results: a filter strip (controls), a heading, a tagged hit
    /// for chat 1, an untagged media hit naming chat 2, and a reply chooser
    /// in the chat region with one tagged and one untagged destination.
    fn lists() -> UiTree {
        UiTree {
            root: "w".into(),
            nodes: vec![
                node("w", "Quill", None, &["list", "chat"]),
                node("list", "", Some(REGION_CHAT_LIST), &["results"]),
                node(
                    "results",
                    "",
                    Some(PART_RESULTS),
                    &["filters", "heading", "hit1", "media2", "remove2"],
                ),
                node("filters", "", Some(CONTROLS), &["tab-media"]),
                node("tab-media", "Media", None, &[]),
                node("heading", "Messages", None, &[]),
                node("hit1", "Ann · hello", Some("chat=1"), &["hit1-date"]),
                node("hit1-date", "Mon", None, &[]),
                node("media2", "photo.jpg · Bob", None, &["media2-sub"]),
                node("media2-sub", "Bob · Mon", None, &[]),
                node("remove2", "Remove Bob from recent", None, &[]),
                node("chat", "", Some(REGION_CHAT), &["reply"]),
                node("reply", "", Some(LIST), &["reply-search", "dest1", "dest2"]),
                node("reply-search", "Search chats", Some(CONTROLS), &[]),
                node("dest1", "Reply in Ann", Some("chat=1"), &[]),
                node("dest2", "Reply in Bob", None, &[]),
            ],
            ..UiTree::default()
        }
    }

    fn ids(tree: &UiTree) -> Vec<&str> {
        tree.nodes.iter().map(|n| n.id.as_str()).collect()
    }

    #[test]
    fn the_granted_scope_keeps_granted_rows_and_the_chat_only() {
        let tree = redact(window(), DriverScope::Granted, &|chat| chat == 1);
        assert_eq!(
            ids(&tree),
            [
                "w",
                "list",
                "search",
                "rows",
                "row1",
                "row1-badge",
                "chat",
                "composer"
            ]
        );
        let root = tree.node("w").unwrap_or_else(|| panic!("root kept"));
        assert_eq!(
            root.name, None,
            "an ancestor outside the regions is nameless"
        );
        assert_eq!(root.children, ["list", "chat"]);
        assert_eq!(tree.focus, None, "the focus was on a hidden node");
        assert_eq!(
            tree.node("row1").and_then(|n| n.name.clone()).as_deref(),
            Some("Ann — hi")
        );
    }

    #[test]
    fn everything_keeps_the_window_but_not_ungranted_rows() {
        let tree = redact(window(), DriverScope::Everything, &|chat| chat != 2);
        let kept = ids(&tree);
        assert!(kept.contains(&"settings"));
        assert!(kept.contains(&"menu-button"));
        assert!(kept.contains(&"archive"));
        assert!(!kept.contains(&"row2"), "{kept:?}");
        assert_eq!(tree.focus.as_deref(), Some("settings"));
    }

    #[test]
    fn the_order_of_the_nodes_does_not_matter() {
        let mut tree = window();
        tree.nodes.reverse();
        let cut = redact(tree, DriverScope::Granted, &|chat| chat == 1);
        let mut kept = ids(&cut);
        kept.sort_unstable();
        assert_eq!(
            kept,
            [
                "chat",
                "composer",
                "list",
                "row1",
                "row1-badge",
                "rows",
                "search",
                "w"
            ]
        );
    }

    #[test]
    fn nothing_granted_leaves_the_shape_without_rows() {
        let tree = redact(window(), DriverScope::Granted, &|_| false);
        assert_eq!(
            ids(&tree),
            ["w", "list", "search", "chat", "composer"],
            "the rows list has nothing allowed under it, so it goes too"
        );
    }

    #[test]
    fn lists_show_only_tagged_granted_rows_and_controls() {
        let tree = redact(lists(), DriverScope::Granted, &|chat| chat == 1);
        let mut kept = ids(&tree);
        kept.sort_unstable();
        assert_eq!(
            kept,
            [
                "chat",
                "dest1",
                "filters",
                "hit1",
                "hit1-date",
                "list",
                "reply",
                "reply-search",
                "results",
                "tab-media",
                "w"
            ],
            "untagged rows, headings and the remove button are gone"
        );
        let names: Vec<&str> = tree
            .nodes
            .iter()
            .filter_map(|n| n.name.as_deref())
            .collect();
        assert!(!names.iter().any(|n| n.contains("Bob")), "{names:?}");
        assert_eq!(
            tree.node("results").and_then(|n| n.name.clone()),
            None,
            "a list stays as a nameless ancestor"
        );
        // With All chats on, lists are plain containers.
        let tree = redact(lists(), DriverScope::Everything, &|_| true);
        assert_eq!(tree.nodes.len(), lists().nodes.len());
    }

    #[test]
    fn a_point_in_a_list_needs_a_granted_row_or_a_control_under_it() {
        let granted = |chat: i64| chat == 1;
        let tags = |list: &[&str]| list.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        let scope = DriverScope::Granted;
        assert!(point_allowed(
            &tags(&["chat=1", PART_RESULTS, REGION_CHAT_LIST]),
            scope,
            &granted
        ));
        assert!(point_allowed(
            &tags(&[CONTROLS, PART_RESULTS, REGION_CHAT_LIST]),
            scope,
            &granted
        ));
        assert!(
            !point_allowed(&tags(&[PART_RESULTS, REGION_CHAT_LIST]), scope, &granted),
            "an untagged row in the results"
        );
        assert!(
            !point_allowed(&tags(&[PART_ROWS, REGION_CHAT_LIST]), scope, &granted),
            "the space between chat rows"
        );
        assert!(point_allowed(
            &tags(&["chat=1", LIST, REGION_CHAT]),
            scope,
            &granted
        ));
        assert!(
            !point_allowed(&tags(&[LIST, REGION_CHAT]), scope, &granted),
            "an untagged destination in the reply chooser"
        );
        assert!(point_allowed(
            &tags(&[LIST, REGION_CHAT]),
            DriverScope::Everything,
            &granted
        ));
    }

    #[test]
    fn a_point_needs_an_allowed_region_under_it() {
        let granted = |chat: i64| chat == 1;
        let tags = |list: &[&str]| list.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        assert!(point_allowed(
            &tags(&["chat=1", PART_ROWS, REGION_CHAT_LIST]),
            DriverScope::Granted,
            &granted
        ));
        assert!(!point_allowed(
            &tags(&["chat=2", PART_ROWS, REGION_CHAT_LIST]),
            DriverScope::Granted,
            &granted
        ));
        assert!(!point_allowed(
            &tags(&["chat=2", PART_ROWS, REGION_CHAT_LIST]),
            DriverScope::Everything,
            &granted
        ));
        assert!(!point_allowed(
            &tags(&[REGION_CHAT_LIST]),
            DriverScope::Granted,
            &granted
        ));
        assert!(point_allowed(
            &tags(&[REGION_CHAT]),
            DriverScope::Granted,
            &granted
        ));
        assert!(point_allowed(
            &tags(&[REGION_MENU, REGION_CHAT]),
            DriverScope::Granted,
            &granted
        ));
        assert!(!point_allowed(&tags(&[]), DriverScope::Granted, &granted));
        assert!(point_allowed(&tags(&[]), DriverScope::Everything, &granted));
        assert!(!point_allowed(
            &tags(&[REGION_PANELS]),
            DriverScope::Granted,
            &granted
        ));
        assert_eq!(chat_of_tag("chat=-100"), Some(-100));
        assert_eq!(chat_of_tag("region=chat"), None);
        assert_eq!(chat_tag(7), "chat=7");
    }
}
