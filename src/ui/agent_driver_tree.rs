//! GPUI's accessibility tree, as the UI driver hands it to agents
//! (`quill::agent::ui_driver::tree::UiTree`). Read from the last frame
//! GPUI built a tree for; `Window::set_a11y_forced` makes it build one
//! every frame while an agent drives.
#![deny(clippy::indexing_slicing)]

use gpui_kit::accesskit::{NodeId, Toggled};
use gpui_kit::{A11yNodeSnapshot, Window};
use quill::agent::ui_driver::tree::{UiNode, UiTree, id_string};

/// The tree of the last frame that built one.
pub(super) fn read(window: &Window) -> Option<UiTree> {
    let snapshot = window.a11y_tree_snapshot()?;
    let viewport = window.viewport_size();
    let nodes = snapshot
        .nodes
        .iter()
        .map(|node| convert(node, snapshot.focus))
        .collect();
    Some(UiTree {
        width: viewport.width.as_f32(),
        height: viewport.height.as_f32(),
        scale: window.scale_factor(),
        root: id_string(snapshot.root.0),
        focus: snapshot.focus.map(|id| id_string(id.0)),
        frame: snapshot.frame_number,
        nodes,
    })
}

fn convert(node: &A11yNodeSnapshot, focus: Option<NodeId>) -> UiNode {
    let mut states = Vec::new();
    if focus == Some(node.id) {
        states.push("focused");
    }
    if node.selected == Some(true) {
        states.push("selected");
    }
    match node.expanded {
        Some(true) => states.push("expanded"),
        Some(false) => states.push("collapsed"),
        None => {}
    }
    match node.toggled {
        Some(Toggled::True) => states.push("checked"),
        Some(Toggled::False) => states.push("unchecked"),
        Some(Toggled::Mixed) => states.push("mixed"),
        None => {}
    }
    if node.disabled {
        states.push("disabled");
    }
    if node.hidden {
        states.push("hidden");
    }
    if node.read_only {
        states.push("read_only");
    }
    if node.modal {
        states.push("modal");
    }
    let name = node
        .label
        .clone()
        .or_else(|| node.placeholder.clone())
        .or_else(|| node.tooltip.clone())
        .or_else(|| node.description.clone())
        .filter(|name| !name.is_empty());
    UiNode {
        id: id_string(node.id.0),
        role: format!("{:?}", node.role),
        name,
        value: node.value.clone().filter(|value| !value.is_empty()),
        bounds: node.bounds.map(|bounds| {
            [
                bounds.origin.x.as_f32(),
                bounds.origin.y.as_f32(),
                bounds.size.width.as_f32(),
                bounds.size.height.as_f32(),
            ]
        }),
        states: states.into_iter().map(str::to_string).collect(),
        actions: node
            .actions
            .iter()
            .map(|action| format!("{action:?}"))
            .collect(),
        children: node.children.iter().map(|id| id_string(id.0)).collect(),
    }
}
