//! GPUI's accessibility tree, as the UI driver hands it to agents
//! (`quill::agent::ui_driver::tree::UiTree`). Read from the last frame
//! GPUI built a tree for (`Window::set_a11y_forced` makes it build one
//! every frame while an agent drives) and cut to the grants at once
//! (`quill::agent::ui_driver::redact`), in the same update that decided
//! the surface, so no unredacted tree ever leaves this module.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::accesskit::{NodeId, Toggled};
use gpui_kit::{A11yNodeSnapshot, Window};
use quill::agent::ui_driver::redact;
use quill::agent::ui_driver::surface::{self, Surface};
use quill::agent::ui_driver::tree::{UiNode, UiTree, id_string};

impl QuillApp {
    /// The surface and, unless it is blocked, the tree of the last frame
    /// that built one, cut to the grants. `Err` carries the note the agent
    /// gets instead.
    pub(super) fn agent_tree(&self, window: &Window) -> Result<(Surface, Option<UiTree>), String> {
        if quill::screen_reader::running() {
            return Err(surface::screen_reader_active());
        }
        let surface = self.driver_surface();
        if let Some(note) = surface.read_refusal() {
            return Err(note);
        }
        let tree = read(window).map(|tree| {
            redact::redact(tree, self.driver_scope(), &|chat_id| {
                self.driver_may_open_chat(chat_id)
            })
        });
        Ok((surface, tree))
    }
}

/// The tree of the last frame that built one, uncut.
fn read(window: &Window) -> Option<UiTree> {
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

/// The uncut tree, for tests and the frame-cost measurement.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(super) fn raw_tree(window: &Window) -> Option<UiTree> {
    read(window)
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
        tag: node.tag.clone(),
    }
}
