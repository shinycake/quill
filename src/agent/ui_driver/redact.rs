//! Cutting the element tree to what the grants allow. The UI tags the
//! containers it draws (`a11y_tag`): the regions of the window, the parts
//! of the chat list, and every row that stands for a chat. The cut reads
//! those tags only, never a label, so no text in the window can widen it.
//!
//! With [`DriverScope::Granted`] the tree keeps the chat list's search
//! field, folder tabs and rows, the open chat, the forum column and any
//! context menu; rows of ungranted chats go, and so does everything else.
//! With [`DriverScope::Everything`] the whole tree stays, less the rows of
//! chats that still have no grant (secret chats, blocked ones).

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
pub const PART_ROWS: &str = "part=rows";
pub const PART_RESULTS: &str = "part=search-results";

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
        return if granted(chat_id) {
            parent
        } else {
            Zone::Pruned
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
        PART_SEARCH | PART_FOLDERS | PART_ROWS | PART_RESULTS if parent == Zone::ChatList => {
            Zone::Allowed
        }
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
/// allowed region, and in every scope never over an ungranted chat's row.
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
    for tag in tags {
        match tag.as_str() {
            ARCHIVE => return false,
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
            ["w", "list", "search", "rows", "chat", "composer"]
        );
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
