//! The element tree an agent reads: GPUI's accessibility tree as plain
//! data, with the lookups the tools need (filter, hit test, the controls
//! an agent may not click).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

/// One element of the window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiNode {
    /// Stable while the element stays on screen (AccessKit's id, in hex).
    pub id: String,
    /// `Button`, `TextInput`, `MenuItem`, `Label`, `ListItem`...
    pub role: String,
    /// The accessible name (a button's text, a message's summary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An input's text, a slider's value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// `[x, y, width, height]` in window logical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f32; 4]>,
    /// `focused`, `selected`, `expanded`, `checked`, `disabled`, `modal`...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<String>,
    /// `click`, `focus`...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    /// Ids of the child nodes, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
}

/// The whole window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiTree {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    /// GPUI's frame counter when the tree was read.
    pub frame: u64,
    /// Parents before their children.
    pub nodes: Vec<UiNode>,
}

/// Node ids are AccessKit's 64-bit ids written in hex.
pub fn id_string(id: u64) -> String {
    format!("{id:x}")
}

/// The AccessKit id behind an id string; `None` for anything else.
pub fn parse_id(text: &str) -> Option<u64> {
    let text = text.trim();
    (!text.is_empty()).then(|| u64::from_str_radix(text, 16).ok())?
}

/// Why a control may not be clicked through the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guarded {
    /// Sends a message (would skip the paced queue and the confirmations).
    Send,
    /// Deletes messages or history.
    Delete,
    /// Spends Stars, Premium or money.
    Pay,
}

impl Guarded {
    /// The write tool an agent uses instead.
    pub fn advice(self) -> &'static str {
        match self {
            Guarded::Send => {
                "Sending through the window skips Quill's pacing and confirmations: use send_message, send_file or set_draft instead."
            }
            Guarded::Delete => {
                "Deleting through the window skips Quill's checks: use delete_messages (your own messages only) instead."
            }
            Guarded::Pay => "Agents cannot spend Stars, Premium or money through Quill.",
        }
    }
}

/// English words that mark a guarded control, matched as whole words in a
/// short name. The user's language is covered by [`guarded_phrases`].
const SEND_WORDS: &[&str] = &["send", "schedule", "resend"];
const DELETE_WORDS: &[&str] = &["delete", "unsend", "clear history"];
const PAY_WORDS: &[&str] = &[
    "pay", "buy", "purchase", "subscribe", "stars", "star", "gift", "premium", "top up", "donate",
    "transfer", "checkout",
];
/// Names longer than this are content (a message), not a control.
const CONTROL_NAME_WORDS: usize = 6;

/// The translated names of the controls Quill draws, so the check holds in
/// every interface language.
fn guarded_phrases() -> Vec<(String, Guarded)> {
    use crate::lang::{keys, tr};
    vec![
        (tr(keys::CONTEXT_SEND_MESSAGE).to_string(), Guarded::Send),
        (tr(keys::Q_MONEY_SCHEDULE_MESSAGE).to_string(), Guarded::Send),
        (tr(keys::SCHEDULE_BUTTON).to_string(), Guarded::Send),
        (tr(keys::BOX_DELETE).to_string(), Guarded::Delete),
    ]
}

fn has_word(name: &str, word: &str) -> bool {
    if word.contains(' ') {
        return name.contains(word);
    }
    name.split(|c: char| !c.is_alphanumeric())
        .any(|part| part == word)
}

/// Whether a control with `name` is one agents may not click.
pub fn guarded_by_name(name: &str) -> Option<Guarded> {
    let lower = name.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }
    for (phrase, kind) in guarded_phrases() {
        let phrase = phrase.trim().to_lowercase();
        if !phrase.is_empty() && (lower == phrase || lower.starts_with(&format!("{phrase} "))) {
            return Some(kind);
        }
    }
    if lower.split_whitespace().count() > CONTROL_NAME_WORDS {
        return None;
    }
    if SEND_WORDS.iter().any(|w| has_word(&lower, w)) {
        return Some(Guarded::Send);
    }
    if DELETE_WORDS.iter().any(|w| has_word(&lower, w)) {
        return Some(Guarded::Delete);
    }
    if PAY_WORDS.iter().any(|w| has_word(&lower, w)) {
        return Some(Guarded::Pay);
    }
    None
}

impl UiNode {
    /// The point a click lands on.
    pub fn center(&self) -> Option<(f32, f32)> {
        let [x, y, w, h] = self.bounds?;
        Some((x + w / 2.0, y + h / 2.0))
    }

    fn matches_text(&self, needle: &str) -> bool {
        let hit = |text: &Option<String>| {
            text.as_deref()
                .is_some_and(|t| t.to_lowercase().contains(needle))
        };
        hit(&self.name) || hit(&self.value) || self.role.to_lowercase().contains(needle)
    }
}

impl UiTree {
    pub fn node(&self, id: &str) -> Option<&UiNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// Child id to parent id.
    pub fn parents(&self) -> HashMap<&str, &str> {
        let mut parents = HashMap::new();
        for node in &self.nodes {
            for child in &node.children {
                parents.insert(child.as_str(), node.id.as_str());
            }
        }
        parents
    }

    /// The node and its ancestors, nearest first.
    pub fn lineage<'a>(&'a self, id: &'a str) -> Vec<&'a UiNode> {
        let parents = self.parents();
        let mut out = Vec::new();
        let mut current = Some(id);
        while let Some(id) = current {
            if out.len() > 64 {
                break;
            }
            match self.node(id) {
                Some(node) => out.push(node),
                None => break,
            }
            current = parents.get(id).copied();
        }
        out
    }

    /// The smallest visible node whose bounds contain `(x, y)`.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<&UiNode> {
        self.nodes
            .iter()
            .filter(|node| !node.states.iter().any(|s| s == "hidden"))
            .filter(|node| {
                node.bounds.is_some_and(|[bx, by, bw, bh]| {
                    x >= bx && y >= by && x <= bx + bw && y <= by + bh
                })
            })
            .min_by(|a, b| {
                let area = |n: &UiNode| n.bounds.map(|[_, _, w, h]| w * h).unwrap_or(f32::MAX);
                area(a).total_cmp(&area(b))
            })
    }

    /// Nodes whose name, value or role contains `text` (case-insensitive)
    /// and whose role is `role` (case-insensitive), when given.
    pub fn filter(&self, text: Option<&str>, role: Option<&str>) -> Vec<&UiNode> {
        let text = text.map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty());
        let role = role.map(|r| r.trim().to_lowercase()).filter(|r| !r.is_empty());
        self.nodes
            .iter()
            .filter(|node| text.as_deref().is_none_or(|t| node.matches_text(t)))
            .filter(|node| role.as_deref().is_none_or(|r| node.role.to_lowercase() == r))
            .collect()
    }

    /// Ids present here and not in `before`.
    pub fn new_since<'a>(&'a self, before: &UiTree) -> Vec<&'a UiNode> {
        let known: std::collections::HashSet<&str> =
            before.nodes.iter().map(|n| n.id.as_str()).collect();
        self.nodes
            .iter()
            .filter(|node| !known.contains(node.id.as_str()))
            .collect()
    }

    /// Why clicking `id` is refused, if it is: the node or an ancestor is a
    /// guarded control, or it is disabled.
    pub fn click_refusal(&self, id: &str) -> Option<String> {
        let lineage = self.lineage(id);
        let node = lineage.first()?;
        if node.states.iter().any(|s| s == "disabled") {
            return Some(format!(
                "{} \"{}\" is disabled right now, so a click would do nothing.",
                node.role,
                node.name.as_deref().unwrap_or("")
            ));
        }
        lineage.iter().take(3).find_map(|node| {
            let name = node.name.as_deref()?;
            let kind = guarded_by_name(name)?;
            Some(format!(
                "Quill does not let agents click \"{name}\" through the window. {}",
                kind.advice()
            ))
        })
    }

    /// The tree as the tool returns it, `nodes` cut to `limit`.
    pub fn to_json(&self, nodes: &[&UiNode], limit: usize) -> Value {
        let shown: Vec<Value> = nodes
            .iter()
            .take(limit)
            .map(|node| serde_json::to_value(node).unwrap_or(Value::Null))
            .collect();
        json!({
            "window": {"width": self.width, "height": self.height, "scale": self.scale},
            "focus": self.focus,
            "frame": self.frame,
            "total": nodes.len(),
            "truncated": nodes.len() > limit,
            "nodes": shown,
        })
    }

    /// One line per node for the model: `id role "name" [x,y,w,h] states`.
    pub fn text_lines(&self, nodes: &[&UiNode], limit: usize) -> String {
        let depth = self.depths();
        let mut out = String::new();
        for node in nodes.iter().take(limit) {
            let indent = "  ".repeat(depth.get(node.id.as_str()).copied().unwrap_or(0));
            out.push_str(&indent);
            out.push_str(&node.id);
            out.push(' ');
            out.push_str(&node.role);
            if let Some(name) = &node.name {
                out.push_str(&format!(" \"{}\"", shorten(name, 80)));
            }
            if let Some(value) = &node.value {
                out.push_str(&format!(" = \"{}\"", shorten(value, 80)));
            }
            if let Some([x, y, w, h]) = node.bounds {
                out.push_str(&format!(" [{x:.0},{y:.0},{w:.0},{h:.0}]"));
            }
            if !node.states.is_empty() {
                out.push_str(&format!(" ({})", node.states.join(", ")));
            }
            out.push('\n');
        }
        if nodes.len() > limit {
            out.push_str(&format!(
                "... {} more nodes; pass filter or role to narrow, or a higher limit.\n",
                nodes.len().saturating_sub(limit)
            ));
        }
        out
    }

    fn depths(&self) -> HashMap<&str, usize> {
        let parents = self.parents();
        let mut depths: HashMap<&str, usize> = HashMap::new();
        for node in &self.nodes {
            let mut depth = 0;
            let mut current = parents.get(node.id.as_str()).copied();
            while let Some(parent) = current {
                depth += 1;
                if depth > 64 {
                    break;
                }
                current = parents.get(parent).copied();
            }
            depths.insert(node.id.as_str(), depth);
        }
        depths
    }
}

fn shorten(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut cut: String = flat.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::{Guarded, UiNode, UiTree, guarded_by_name, id_string, parse_id};

    fn node(id: &str, role: &str, name: &str, bounds: [f32; 4], children: &[&str]) -> UiNode {
        UiNode {
            id: id.into(),
            role: role.into(),
            name: (!name.is_empty()).then(|| name.to_string()),
            bounds: Some(bounds),
            children: children.iter().map(|c| c.to_string()).collect(),
            ..UiNode::default()
        }
    }

    fn tree() -> UiTree {
        UiTree {
            width: 800.0,
            height: 600.0,
            scale: 2.0,
            root: "0".into(),
            focus: Some("3".into()),
            frame: 7,
            nodes: vec![
                node("0", "Window", "Quill", [0.0, 0.0, 800.0, 600.0], &["1", "2", "4"]),
                node("1", "Button", "Search", [10.0, 10.0, 100.0, 30.0], &[]),
                node("2", "Button", "Send message", [700.0, 560.0, 40.0, 30.0], &["3"]),
                node("3", "Label", "Send message", [705.0, 565.0, 30.0, 20.0], &[]),
                node("4", "TextInput", "Message", [100.0, 560.0, 500.0, 30.0], &[]),
            ],
        }
    }

    #[test]
    fn ids_round_trip_as_hex() {
        assert_eq!(id_string(255), "ff");
        assert_eq!(parse_id("ff"), Some(255));
        assert_eq!(parse_id(" FF "), Some(255));
        assert_eq!(parse_id(""), None);
        assert_eq!(parse_id("zz"), None);
    }

    #[test]
    fn filter_matches_text_and_role() {
        let tree = tree();
        let ids = |nodes: Vec<&UiNode>| nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(tree.filter(Some("send"), None)), ["2", "3"]);
        assert_eq!(ids(tree.filter(Some("send"), Some("button"))), ["2"]);
        assert_eq!(ids(tree.filter(None, Some("TextInput"))), ["4"]);
        assert_eq!(tree.filter(None, None).len(), 5);
        assert_eq!(tree.filter(Some("  "), Some("")).len(), 5);
    }

    #[test]
    fn hit_test_picks_the_smallest_node_under_the_point() {
        let tree = tree();
        assert_eq!(tree.hit_test(710.0, 570.0).map(|n| n.id.as_str()), Some("3"));
        assert_eq!(tree.hit_test(50.0, 20.0).map(|n| n.id.as_str()), Some("1"));
        assert_eq!(tree.hit_test(400.0, 300.0).map(|n| n.id.as_str()), Some("0"));
        assert_eq!(tree.hit_test(900.0, 300.0), None);
    }

    #[test]
    fn guarded_controls_are_recognised_by_name() {
        assert_eq!(guarded_by_name("Send message"), Some(Guarded::Send));
        assert_eq!(guarded_by_name("Schedule"), Some(Guarded::Send));
        assert_eq!(guarded_by_name("Delete for everyone"), Some(Guarded::Delete));
        assert_eq!(guarded_by_name("Clear history"), Some(Guarded::Delete));
        assert_eq!(guarded_by_name("Buy Stars"), Some(Guarded::Pay));
        assert_eq!(guarded_by_name("Top up"), Some(Guarded::Pay));
        assert_eq!(guarded_by_name("Search"), None);
        assert_eq!(guarded_by_name("Sender settings"), None, "no substring match");
        assert_eq!(
            guarded_by_name("Ann, 14:02: please send me the file when you can, thanks"),
            None,
            "a message is content, not a control"
        );
        assert_eq!(guarded_by_name(""), None);
    }

    #[test]
    fn click_refusal_looks_at_the_node_and_its_parents() {
        let mut tree = tree();
        assert!(tree.click_refusal("3").is_some_and(|r| r.contains("send_message")));
        assert!(tree.click_refusal("2").is_some());
        assert_eq!(tree.click_refusal("1"), None);
        assert_eq!(tree.click_refusal("missing"), None);
        if let Some(search) = tree.nodes.get_mut(1) {
            search.states.push("disabled".into());
        }
        assert!(tree.click_refusal("1").is_some_and(|r| r.contains("disabled")));
    }

    #[test]
    fn new_since_lists_added_nodes() {
        let before = tree();
        let mut after = tree();
        after.nodes.push(node("9", "MenuItem", "Copy", [0.0, 0.0, 10.0, 10.0], &[]));
        let added: Vec<&str> = after.new_since(&before).iter().map(|n| n.id.as_str()).collect();
        assert_eq!(added, ["9"]);
    }

    #[test]
    fn text_lines_indent_by_depth_and_truncate() {
        let tree = tree();
        let all: Vec<&UiNode> = tree.nodes.iter().collect();
        let text = tree.text_lines(&all, 4);
        assert!(text.starts_with("0 Window \"Quill\" [0,0,800,600]\n  1 Button \"Search\""));
        assert!(text.contains("    3 Label"));
        assert!(text.contains("1 more node"));
        let json = tree.to_json(&all, 3);
        assert_eq!(json["truncated"], true);
        assert_eq!(json["total"], 5);
        assert_eq!(json["nodes"].as_array().map(Vec::len), Some(3));
        assert_eq!(json["window"]["scale"], 2.0);
    }
}
