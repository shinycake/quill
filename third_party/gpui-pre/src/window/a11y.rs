//! Accessibility support, provided by [AccessKit][accesskit].
//!
//! There are user-facing guide-level docs [here](crate::_accessibility).
//!
//! ## Architecture
//!
//! ```text
//!                              ┌────────────────────────────────┐   ┌─────────────────────┐
//!                           ┌─▶│ AccessKit Adapter (MacOS)      │◀─▶│ MacOS System APIs   │
//!                           │  └────────────────────────────────┘   └─────────────────────┘
//!                           │
//! ┌──────┐   ┌───────────┐  │  ┌────────────────────────────────┐   ┌─────────────────────┐
//! │ GPUI │◀─▶│ AccessKit │◀─┼─▶│ AccessKit Adapter (Windows)    │◀─▶│ Windows System APIs │
//! └──────┘   └───────────┘  │  └────────────────────────────────┘   └─────────────────────┘
//!                           │
//!                           │  ┌────────────────────────────────┐   ┌─────────────────────┐
//!                           └─▶│ AccessKit Adapter (Linux)      │◀─▶│ dbus                │
//!                              └────────────────────────────────┘   └─────────────────────┘
//! ```
//!
//! In order for GPUI apps to be usable for people using assistive technology,
//! we must do a few things:
//! - Inform the system when the UI changes meaningfully. This includes:
//!   - Reporting new/removed/changed UI elements
//!   - *Not* reporting irrelevant UI changes, e.g. an invisible `div()` being
//!     added.
//!   - Reporting the appearance and capabilities of each UI element. For example:
//!     - What does this piece of text say?
//!     - How far along is this progress bar?
//!     - Can this node be focused?
//!     - Can this node have a value directly assigned? (e.g. a slider)
//! - Allowing the system to interact with the UI by dispatching actions to
//!   nodes. Note that AccessKit has its own [`Action`] type, which is not the
//!   [`crate::Action`] trait.
//! - Activate and deactivate accessibility features when requested by the
//!   system.
//!
//! Activating and deactivating at the right time is trivial, so I won't go into
//! detail here. The other two are almost orthogonal in implementation.
//!
//! The state for both lives in the [`A11y`] struct in this module.
//!
//! ### Reporting UI changes
//!
//! Every frame, we build a [`TreeUpdate`] and send it to the platform-specific
//! adapter. A [`TreeUpdate`] is a representation of a subset of the UI tree.
//! When the adapter receives the update, it diffs it against the previous
//! update, and calls platform-specific APIs to inform screen readers about the
//! changes. Nodes may have been created, destroyed, or updated.
//!
//! Each node has an ID, and this ID *should* be stable across frames. If a
//! node's ID changes, then, from AccessKit's point of view, it is a different
//! node.
//!
//! We derive the node ID from the [`GlobalElementId`] in
//! [`GlobalElementId::accesskit_node_id`]. Nodes without [`GlobalElementId`]s
//! cannot produce an AccessKit [`NodeId`], and so are not included in the
//! accessibility tree. We try to warn when using accessibility APIs on
//! [`div()`] without setting an ID.
//!
//! This all happens in [`Drawable::prepaint`]. The [`A11y`] struct maintains a
//! stack of nodes during prepainting, which we can use to calculate the
//! [`NodeId`]s, and record parent-child relationships. Once all [`Element`]s in
//! a frame have been prepainted, we send the resulting [`TreeUpdate`] object to
//! the adapter and the screen reader can announce the changes.
//!
//! #### Synthetic children
//!
//! Additionally, some nodes can register "synthetic children" using
//! [`Element::a11y_synthetic_children`]. Normally, one accesskit node is pushed
//! for every [`Element`] with a role and id. However, sometimes a single
//! element may want to produce many accesskit nodes. These extra nodes are
//! referred to as "synthetic children" of the element providing a non-default
//! [`Element::a11y_synthetic_children`] implementation.
//!
//! The user is provided a builder-style API using [`A11ySubtreeBuilder`], which
//! allows them to create push nodes that are children of the current node, as
//! well as modify the current node itself.
//!
//! GPUI calls this callback *after* prepainting (and just before popping the
//! corresponding element), since this step may need prepaint information to be
//! available. In the future, we may want to add prepaint information more
//! generally to [`Element::write_a11y_info`], but for now that's not necessary.
//!
//! ### Responding to actions
//!
//! On adapter creation, we provide a callback to the adapter, which can be used
//! to dispatch actions. This callback forwards to [`A11y::action_listeners`], a
//! mapping from [`NodeId`]s to action handlers (basically just `Box<dyn
//! Fn()>`).
//!
//! This is populated in:
//! - [`Window::on_a11y_action`], which is called by:
//! - [`Interactivity::paint`], which is called by:
//! - [`StatefulInteractiveElement::on_a11y_action`], which is a public-facing API
//!
//! These are cleared at the start of a frame, and re-populated during painting.
//!
//! [`NodeId`]: accesskit::NodeId

use crate::*;

pub(crate) mod debug;

use crate::{App, Bounds, FocusId, Pixels, SharedString, Window};
use accesskit::{Action, NodeId, TreeUpdate};
use collections::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
use std::mem;
use std::ops::Range;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// The fixed AccessKit node ID used for the root of every window's a11y tree.
pub(crate) const ROOT_NODE_ID: NodeId = NodeId(0);

/// A listener for an accessibility action on a specific node.
pub(crate) type A11yActionListener =
    Box<dyn FnMut(Option<&accesskit::ActionData>, &mut Window, &mut App) + 'static>;

/// Per-window accessibility state.
///
/// Manages the AccessKit tree that is built each frame and the mappings
/// needed to dispatch incoming action requests back to the right elements.
pub(crate) struct A11y {
    /// Whether accessibility has been [forcibly disabled] for this window.
    ///
    /// [forcibly disabled]: crate::Application::new_inaccessible
    force_disabled: bool,
    /// Whether a11y features have been requested by the system.
    ///
    /// Updated by AccessKit using callbacks provided to the adapter. Can change
    /// halfway through a frame.
    active_flag: Arc<AtomicBool>,
    /// Whether a11y features are active for *this specific frame*.
    ///
    /// At the start of each frame, we load [`Self::active_flag`] (using
    /// [`Self::sync_active_flag`]) and use this to determine whether we
    /// should construct a [`TreeUpdate`] for this frame. It's important that
    /// this value is stable within a frame, because the builder API exposed by
    /// this type maintains a stack of nodes and each must be pushed and popped
    /// exactly once.
    ///
    /// At the end of the frame, we re-call [`Self::sync_active_flag`] to
    /// determine whether we should actually send the finished [`TreeUpdate`].
    active_this_frame: bool,
    /// Whether the system (assistive technology) asked for the tree this
    /// frame, as opposed to a tree built only because it was [forced].
    /// Only a tree the system asked for is sent to the platform adapter.
    ///
    /// [forced]: Self::set_forced
    system_active_this_frame: bool,
    /// Build the tree every frame even while no assistive technology is
    /// connected, for an in-process reader ([`crate::Window::set_a11y_forced`]).
    forced: bool,
    pub(crate) nodes: A11yNodeBuilder,
    pub(crate) focus_ids: FxHashMap<NodeId, FocusId>,
    pub(crate) node_bounds: FxHashMap<NodeId, Bounds<Pixels>>,
    /// [`crate::InteractiveElement::a11y_tag`] per node, for the snapshot
    /// only; never part of the [`TreeUpdate`].
    pub(crate) node_tags: FxHashMap<NodeId, SharedString>,
    pub(crate) action_listeners: FxHashMap<NodeId, Vec<(Action, A11yActionListener)>>,
    /// The tables above as the last frame filled them, for [`Self::replay`].
    rendered: RenderedTables,
    /// The last frame built a tree, the one [`Self::replay`] reads.
    replayable: bool,
    /// A range this frame could not be replayed ([`Self::replay`]).
    replay_refused: bool,
    /// The window's title, used to label the root node so assistive
    /// technology can tell windows apart.
    window_title: Option<SharedString>,
    /// The focus id we most recently reported as having no accessibility node,
    /// used to log at most once per focus change rather than every frame.
    last_focus_without_node: Option<FocusId>,
    /// Retains the last tree update (and, in debug builds, per-node provenance)
    /// so it can be dumped via [`crate::Window::debug_a11y_tree_json`].
    debug: debug::A11yDebug,
    /// Maps a view's [`EntityId`] to its `Render` type name
    #[cfg(debug_assertions)]
    pub(crate) view_type_names: FxHashMap<EntityId, &'static str>,
}

impl A11y {
    pub(crate) fn new(
        active_flag: Arc<AtomicBool>,
        force_disabled: bool,
        window_title: Option<SharedString>,
    ) -> Self {
        Self {
            force_disabled,
            active_flag,
            active_this_frame: false,
            system_active_this_frame: false,
            forced: false,
            nodes: A11yNodeBuilder::new(),
            focus_ids: FxHashMap::default(),
            node_bounds: FxHashMap::default(),
            node_tags: FxHashMap::default(),
            action_listeners: FxHashMap::default(),
            rendered: RenderedTables::default(),
            replayable: false,
            replay_refused: false,
            window_title,
            last_focus_without_node: None,
            debug: debug::A11yDebug::default(),
            #[cfg(debug_assertions)]
            view_type_names: FxHashMap::default(),
        }
    }

    /// Logs (once per focus change) that the focused element is not exposed to
    /// assistive technology because it has no accessibility node. When this
    /// happens, screen readers fall back to announcing the whole window instead
    /// of the focused element. The fix is to give the element both an
    /// `.id(...)` and a `.role(...)`.
    pub(crate) fn note_focus_without_node(&mut self, focus_id: FocusId, reason: &str) {
        if self.last_focus_without_node != Some(focus_id) {
            self.last_focus_without_node = Some(focus_id);
            log::info!(
                "a11y: focused element ({focus_id:?}) has no accessibility node \
                 ({reason}); assistive technology will announce the whole window \
                 instead. Give it both an `.id(...)` and a `.role(...)` to expose it."
            );
        }
    }

    pub(crate) fn set_window_title(&mut self, title: impl Into<SharedString>) {
        self.window_title = Some(title.into());
    }

    /// Ensures that [`Self::is_active`] returns up to date information.
    ///
    /// See the docs for [`Self::active_flag`] and [`Self::active_this_frame`]
    /// for more commentary.
    pub(crate) fn sync_active_flag(&mut self) {
        self.system_active_this_frame =
            self.is_enabled() && self.active_flag.load(Ordering::SeqCst);
        self.active_this_frame = self.system_active_this_frame || (self.is_enabled() && self.forced);
    }

    /// Whether assistive technology asked for the tree this frame (so the
    /// finished update goes to the platform adapter).
    pub(crate) fn is_system_active(&self) -> bool {
        self.system_active_this_frame
    }

    /// Build the tree every frame whether or not assistive technology is
    /// connected. Returns whether the setting changed.
    pub(crate) fn set_forced(&mut self, on: bool) -> bool {
        let changed = self.forced != on;
        self.forced = on;
        changed
    }

    pub(crate) fn is_forced(&self) -> bool {
        self.forced
    }

    pub(crate) fn is_enabled(&self) -> bool {
        !self.force_disabled
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active_this_frame
    }

    pub(crate) fn set_focusable(&mut self, node_id: NodeId, focus_id: FocusId) {
        self.focus_ids.insert(node_id, focus_id);
    }

    /// Report `node_id` as the currently-focused node, if it is present in the
    /// tree.
    ///
    /// Must only be called once per frame.
    pub(crate) fn set_focus(&mut self, node_id: NodeId) {
        // A focused node must have been registered as focusable this frame.
        if !self.focus_ids.contains_key(&node_id) {
            if cfg!(debug_assertions) {
                panic!("set_focus called for a node that was not registered with set_focusable");
            } else {
                log::warn!(
                    "a11y: set_focus called for a node that was not registered with \
                     set_focusable ({node_id:?})"
                );
            }
        }
        if self.nodes.has_node(node_id) {
            // The focused element is properly exposed; reset the dedup so a
            // later focus on a node-less element logs again.
            self.last_focus_without_node = None;
            self.nodes.claim(FocusClaim::Owner(node_id));
        } else if !self.focus_nearest_focusable_ancestor() {
            // The element registered a focus handle and an id, but never got a
            // node because it has no role.
            if let Some(focus_id) = self.focus_ids.get(&node_id).copied() {
                self.note_focus_without_node(focus_id, "it has an id but no role");
            }
        }
    }

    /// The focused element has no node of its own (no role, or no id):
    /// report the nearest enclosing node that is focusable this frame
    /// instead. A composite control puts its role on a frame that tracks a
    /// focus handle of its own around the element that takes the focus (a
    /// text input's frame around its editor), and that frame is what has
    /// the focus to assistive technology. Returns whether the focus is
    /// reported now.
    pub(crate) fn focus_nearest_focusable_ancestor(&mut self) -> bool {
        if self.nodes.focus.is_some() {
            return true;
        }
        let ancestor = self
            .nodes
            .ids_stack
            .iter()
            .rev()
            .find(|id| **id != ROOT_NODE_ID && self.focus_ids.contains_key(*id))
            .copied();
        match ancestor {
            Some(id) => {
                self.last_focus_without_node = None;
                self.nodes.claim(FocusClaim::Fallback(id));
                true
            }
            None => false,
        }
    }

    pub(crate) fn set_active_descendant(&mut self, node_id: NodeId) {
        // The active descendant must be a descendant of the focused container,
        // not the focused node itself.
        if self.nodes.node_is_focused(node_id) {
            if cfg!(debug_assertions) {
                panic!("set_active_descendant called on the focused node");
            } else {
                log::warn!("a11y: set_active_descendant called on the focused node ({node_id:?})");
            }
            return;
        }
        if self.nodes.has_node(node_id) && self.nodes.focus_is_ancestor_of_current() {
            self.nodes.claim(FocusClaim::ActiveDescendant(node_id));
        }
    }

    /// Clear per-frame state and push the root node to start a new frame.
    /// Returns whether a cached view can replay its part of the last
    /// frame's tree ([`Self::replay`]): not when the last frame built none.
    pub(crate) fn begin_frame(&mut self) -> bool {
        let rendered = &mut self.rendered;
        mem::swap(&mut self.focus_ids, &mut rendered.focus_ids);
        mem::swap(&mut self.node_bounds, &mut rendered.node_bounds);
        mem::swap(&mut self.node_tags, &mut rendered.node_tags);
        mem::swap(&mut self.action_listeners, &mut rendered.action_listeners);
        self.focus_ids.clear();
        self.node_bounds.clear();
        self.node_tags.clear();
        self.action_listeners.clear();
        self.nodes.begin_frame(self.window_title.as_ref());
        mem::take(&mut self.replayable)
    }

    /// A frame is drawn without a tree.
    pub(crate) fn skip_frame(&mut self) {
        self.replayable = false;
    }

    /// How far the frame being built has got.
    pub(crate) fn index(&self) -> A11yIndex {
        self.nodes.index()
    }

    /// Adds again what the last frame added between `range`'s ends, for a
    /// cached view that replays its last frame instead of prepainting it:
    /// the nodes (those whose parent is outside the range become children
    /// of the node being built now), their bounds, tags, focus handles and
    /// action listeners, and the focus claims made in between. Only after
    /// [`Self::begin_frame`] said the last frame can be replayed.
    pub(crate) fn replay(&mut self, range: Range<A11yIndex>) {
        // The last frame's nodes, in the order `all_nodes` had them.
        let Some(nodes) = self
            .debug
            .last_update()
            .and_then(|update| update.nodes.get(range.start.nodes..range.end.nodes))
        else {
            return;
        };
        // Fresh content already holds one of these ids (an element drawn
        // twice): adding the range would list a node under two parents,
        // which AccessKit rejects. It is left out, and the next frame
        // renders every view.
        if nodes.iter().any(|(id, _)| self.nodes.has_node(*id)) {
            self.replay_refused = true;
            return;
        }
        let nested: FxHashSet<NodeId> = nodes
            .iter()
            .flat_map(|(_, node)| node.children().iter().copied())
            .collect();
        for (id, node) in nodes {
            self.nodes.replay(*id, node.clone(), !nested.contains(id));
            let rendered = &mut self.rendered;
            if let Some(bounds) = rendered.node_bounds.get(id) {
                self.node_bounds.insert(*id, *bounds);
            }
            if let Some(tag) = rendered.node_tags.get(id) {
                self.node_tags.insert(*id, tag.clone());
            }
            if let Some(focus_id) = rendered.focus_ids.get(id) {
                self.focus_ids.insert(*id, *focus_id);
            }
            if let Some(listeners) = rendered.action_listeners.remove(id) {
                self.action_listeners.insert(*id, listeners);
            }
            #[cfg(debug_assertions)]
            if let Some(info) = self.debug.node_info(*id) {
                self.nodes.record_node_info(*id, info.clone());
            }
        }
        self.nodes
            .replay_claims(range.start.claims..range.end.claims);
    }

    /// Finalize the tree and produce a [`TreeUpdate`] for the platform adapter.
    pub(crate) fn end_frame(&mut self, frame: debug::FrameDebugInfo) -> TreeUpdate {
        self.replayable = !mem::take(&mut self.replay_refused);
        let update = self.nodes.finalize();
        self.debug.capture(
            &update,
            self.nodes.focus,
            self.nodes.active_descendant,
            self.window_title.as_ref(),
            frame,
        );
        #[cfg(debug_assertions)]
        self.debug.capture_node_info(&self.nodes.node_info);
        update
    }

    pub(crate) fn debug_tree_json(&self) -> Option<String> {
        self.debug.to_json()
    }

    /// How many frames built a tree so far.
    pub(crate) fn frame_number(&self) -> u64 {
        self.debug.frame_number()
    }

    /// The last finished tree as plain data, with each node's bounds in
    /// window logical pixels. `None` before the first frame built a tree.
    pub(crate) fn snapshot(&self, scale_factor: f32) -> Option<A11yTreeSnapshot> {
        let update = self.debug.last_update()?;
        let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };
        let nodes = update
            .nodes
            .iter()
            .map(|(id, node)| {
                // Elements record logical bounds; synthetic children carry
                // only the physical rectangle AccessKit sees, if any.
                let bounds = self.node_bounds.get(id).copied().or_else(|| {
                    node.bounds().map(|rect| Bounds {
                        origin: point(px(rect.x0 as f32 / scale), px(rect.y0 as f32 / scale)),
                        size: size(
                            px((rect.x1 - rect.x0) as f32 / scale),
                            px((rect.y1 - rect.y0) as f32 / scale),
                        ),
                    })
                });
                let mut next_action = 0u8;
                let actions = std::iter::from_fn(move || {
                    let action = Action::n(next_action)?;
                    next_action += 1;
                    Some(action)
                })
                .filter(|action| node.supports_action(*action))
                .collect();
                A11yNodeSnapshot {
                    id: *id,
                    tag: self.node_tags.get(id).map(|tag| tag.to_string()),
                    role: node.role(),
                    label: node.label().map(str::to_string),
                    description: node.description().map(str::to_string),
                    value: node.value().map(str::to_string),
                    placeholder: node.placeholder().map(str::to_string),
                    tooltip: node.tooltip().map(str::to_string),
                    bounds,
                    actions,
                    children: node.children().to_vec(),
                    selected: node.is_selected(),
                    expanded: node.is_expanded(),
                    toggled: node.toggled(),
                    disabled: node.is_disabled(),
                    hidden: node.is_hidden(),
                    read_only: node.is_read_only(),
                    modal: node.is_modal(),
                }
            })
            .collect();
        Some(A11yTreeSnapshot {
            root: update.tree.as_ref().map(|tree| tree.root).unwrap_or(ROOT_NODE_ID),
            focus: self.debug.last_focus(),
            frame_number: self.debug.frame_number(),
            nodes,
        })
    }
}

/// One node of an [`A11yTreeSnapshot`].
#[derive(Debug, Clone)]
pub struct A11yNodeSnapshot {
    /// AccessKit's id, stable across frames for the same element.
    pub id: NodeId,
    /// The element's [`crate::InteractiveElement::a11y_tag`], if it set one.
    pub tag: Option<String>,
    /// The node's role.
    pub role: accesskit::Role,
    /// The accessible name.
    pub label: Option<String>,
    /// The longer description, when the element set one.
    pub description: Option<String>,
    /// The value of an input, slider or status.
    pub value: Option<String>,
    /// The placeholder of an empty input.
    pub placeholder: Option<String>,
    /// The tooltip text.
    pub tooltip: Option<String>,
    /// In window logical pixels; `None` for a node without a rectangle.
    pub bounds: Option<Bounds<Pixels>>,
    /// The actions the node says it supports (Click, Focus, ...).
    pub actions: Vec<Action>,
    /// The child nodes, in order.
    pub children: Vec<NodeId>,
    /// Whether a selectable node is selected.
    pub selected: Option<bool>,
    /// Whether an expandable node is expanded.
    pub expanded: Option<bool>,
    /// A checkbox's or toggle's state.
    pub toggled: Option<accesskit::Toggled>,
    /// The node does not take input.
    pub disabled: bool,
    /// The node is excluded from assistive technology and hit testing.
    pub hidden: bool,
    /// A text node that allows focus and selection but not input.
    pub read_only: bool,
    /// An explicitly modal dialog.
    pub modal: bool,
}

/// The accessibility tree of the last frame that built one, as plain data
/// ([`crate::Window::a11y_tree_snapshot`]).
#[derive(Debug, Clone)]
pub struct A11yTreeSnapshot {
    /// The window node.
    pub root: NodeId,
    /// The node GPUI reported as focused, if any.
    pub focus: Option<NodeId>,
    /// Counts the frames that built a tree; two reads with the same number
    /// saw the same frame.
    pub frame_number: u64,
    /// Every node, parents before their children.
    pub nodes: Vec<A11yNodeSnapshot>,
}

/// How far a frame's tree has got, so a cached view can record the part it
/// added ([`A11y::index`]) and add it again next frame ([`A11y::replay`]).
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct A11yIndex {
    /// Finished nodes (`A11yNodeBuilder::all_nodes`).
    nodes: usize,
    /// Focus claims (`A11yNodeBuilder::claims`).
    claims: usize,
}

/// What a frame reports as focused, claimed by the elements in the order
/// they were prepainted, so a cached view's claims can be made again.
#[derive(Clone, Copy, Debug)]
enum FocusClaim {
    /// The node owns the focused handle; it takes over from a fallback.
    Owner(NodeId),
    /// The focused element has no node; its nearest focusable frame stands
    /// in ([`A11y::focus_nearest_focusable_ancestor`]).
    Fallback(NodeId),
    /// The node is the focused container's active descendant.
    ActiveDescendant(NodeId),
}

/// [`A11y`]'s per-node tables as the last frame filled them.
#[derive(Default)]
struct RenderedTables {
    focus_ids: FxHashMap<NodeId, FocusId>,
    node_bounds: FxHashMap<NodeId, Bounds<Pixels>>,
    node_tags: FxHashMap<NodeId, SharedString>,
    action_listeners: FxHashMap<NodeId, Vec<(Action, A11yActionListener)>>,
}

/// Builder API for synthetic children. See the docs for
/// [`Element::a11y_synthetic_children`].
pub struct A11ySubtreeBuilder<'a> {
    parent_id: NodeId,
    nodes: &'a mut A11yNodeBuilder,
    /// Provenance of the real element whose `a11y_synthetic_children` is
    /// running.
    #[cfg(debug_assertions)]
    creator: debug::NodeCreator,
}

impl<'a> A11ySubtreeBuilder<'a> {
    pub(crate) fn new(parent_id: NodeId, nodes: &'a mut A11yNodeBuilder) -> Self {
        Self {
            parent_id,
            nodes,
            #[cfg(debug_assertions)]
            creator: debug::NodeCreator::default(),
        }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn with_creator(mut self, creator: debug::NodeCreator) -> Self {
        self.creator = creator;
        self
    }

    /// Derive a [`NodeId`] for a synthetic child.
    ///
    /// The generated ID is based on the hash of `key`, as well as the parent's
    /// ID. This means that `key`s must be unique within the same
    /// [`Element::a11y_synthetic_children`] call, but may be duplicated across
    /// different calls.
    pub fn synthetic_node_id(&self, key: impl Hash) -> NodeId {
        let mut hasher = std::hash::DefaultHasher::default();
        self.parent_id.0.hash(&mut hasher);
        key.hash(&mut hasher);
        NodeId(hasher.finish())
    }

    /// Append a synthetic leaf node as a child of this element's node.
    ///
    /// Returns `false` if a node with this id is already present in the tree,
    /// in which case the node is discarded.
    pub fn push_child(&mut self, id: NodeId, node: accesskit::Node) -> bool {
        let pushed = self.nodes.push_leaf(id, node);
        #[cfg(debug_assertions)]
        if pushed {
            self.nodes.record_node_info(
                id,
                debug::NodeDebugInfo {
                    synthetic: true,
                    view: self.creator.view,
                    element_id: self.creator.element_id.clone(),
                    source_location: self.creator.source_location,
                },
            );
        }
        pushed
    }

    /// A mutable reference to the parent node.
    pub fn parent_node(&mut self) -> &mut accesskit::Node {
        self.nodes
            .current_node_mut()
            .expect("A11ySubtreeBuilder exists only while its element's node is on the stack")
    }
}

pub(crate) struct A11yNodeBuilder {
    ids_stack: SmallVec<[NodeId; 16]>,
    nodes_stack: SmallVec<[accesskit::Node; 16]>,
    /// This is the exact type required by accesskit, so we can't just make it a
    /// `HashMap<NodeId, Node>` to remove the need for `seen_ids`
    all_nodes: Vec<(NodeId, accesskit::Node)>,
    seen_ids: FxHashSet<NodeId>,
    /// The node that GPUI considers focused. Note that this may be different to
    /// what is reported to accesskit - see [`Self::active_descendant`]
    focus: Option<NodeId>,
    /// Whether [`Self::focus`] is a frame reported for a focused element
    /// with no node of its own, which a node that owns the focus replaces.
    focus_is_fallback: bool,
    /// If a node calls `.aria_active_descendant()`, AND an ancestor is focused,
    /// override it as the focused node. This supports the "active descendant"
    /// pattern, which allows a focused container to act as if a descendant is
    /// focused.
    active_descendant: Option<NodeId>,
    /// The focus claims of the frame being built, in order.
    claims: Vec<FocusClaim>,
    /// The last frame's claims, for [`A11y::replay`].
    rendered_claims: Vec<FocusClaim>,
    #[cfg(debug_assertions)]
    node_info: FxHashMap<NodeId, debug::NodeDebugInfo>,
}

impl A11yNodeBuilder {
    fn new() -> Self {
        Self {
            ids_stack: SmallVec::new(),
            nodes_stack: SmallVec::new(),
            all_nodes: Vec::new(),
            seen_ids: FxHashSet::default(),
            focus: None,
            focus_is_fallback: false,
            active_descendant: None,
            claims: Vec::new(),
            rendered_claims: Vec::new(),
            #[cfg(debug_assertions)]
            node_info: FxHashMap::default(),
        }
    }

    fn index(&self) -> A11yIndex {
        A11yIndex {
            nodes: self.all_nodes.len(),
            claims: self.claims.len(),
        }
    }

    /// Adds a finished node the last frame built, as a child of the node
    /// on top of the stack when `top_level`. The caller checked that the
    /// frame has no node with this id yet.
    fn replay(&mut self, id: NodeId, node: accesskit::Node, top_level: bool) {
        self.seen_ids.insert(id);
        if top_level && let Some(parent) = self.nodes_stack.last_mut() {
            parent.push_child(id);
        }
        self.all_nodes.push((id, node));
    }

    /// Makes the last frame's claims in `range` again.
    fn replay_claims(&mut self, range: Range<usize>) {
        let claims = mem::take(&mut self.rendered_claims);
        for claim in claims.get(range).unwrap_or_default() {
            self.claim(*claim);
        }
        self.rendered_claims = claims;
    }

    fn claim(&mut self, claim: FocusClaim) {
        self.claims.push(claim);
        match claim {
            FocusClaim::Owner(id) => {
                self.clear_fallback_focus();
                self.set_focus(id);
            }
            FocusClaim::Fallback(id) => self.set_fallback_focus(id),
            FocusClaim::ActiveDescendant(id) => self.set_active_descendant(id),
        }
    }

    /// Records provenance for a node already pushed this frame. Debug builds only.
    #[cfg(debug_assertions)]
    pub(crate) fn record_node_info(&mut self, id: NodeId, info: debug::NodeDebugInfo) {
        self.node_info.insert(id, info);
    }

    #[must_use]
    fn can_push(&mut self, id: NodeId) -> bool {
        debug_assert!(!self.ids_stack.is_empty(), "node pushed before push_root");

        if !self.seen_ids.insert(id) {
            debug_assert!(
                false,
                "Duplicate a11y node id: {id:?}. In a release build, this node would be silently discarded from the a11y tree."
            );
            return false;
        }

        true
    }

    /// Push a new node onto the stack. It becomes a child of the current
    /// top-of-stack node.
    ///
    /// Returns `true` if the node was successfully pushed.
    pub(crate) fn push(&mut self, id: NodeId, node: accesskit::Node) -> bool {
        if !self.can_push(id) {
            return false;
        }

        if let Some(parent) = self.nodes_stack.last_mut() {
            parent.push_child(id);
        }
        self.ids_stack.push(id);
        self.nodes_stack.push(node);
        true
    }

    /// Add a leaf node as a child of the current top-of-stack node, without
    /// pushing it onto the stack. Semantically equivalent to a [`Self::push`]
    /// followed by a [`Self::pop`].
    ///
    /// Returns `true` if the node was successfully pushed.
    pub(crate) fn push_leaf(&mut self, id: NodeId, node: accesskit::Node) -> bool {
        if !self.can_push(id) {
            return false;
        }

        if let Some(parent) = self.nodes_stack.last_mut() {
            parent.push_child(id);
        }
        self.all_nodes.push((id, node));
        true
    }

    pub(crate) fn current_node_mut(&mut self) -> Option<&mut accesskit::Node> {
        self.nodes_stack.last_mut()
    }

    /// Pop the current node off the stack and finalize it into the all_nodes
    /// list.
    pub(crate) fn pop(&mut self) {
        debug_assert!(self.ids_stack.len() > 1, "pop would remove the root node");

        if let (Some(id), Some(node)) = (self.ids_stack.pop(), self.nodes_stack.pop()) {
            self.all_nodes.push((id, node));
        }
    }

    /// Push the root node to start a new frame.
    fn begin_frame(&mut self, window_title: Option<&SharedString>) {
        self.all_nodes.clear();
        self.ids_stack.clear();
        self.nodes_stack.clear();
        self.seen_ids.clear();
        #[cfg(debug_assertions)]
        self.node_info.clear();
        let mut root_node = accesskit::Node::new(accesskit::Role::Window);
        if let Some(title) = window_title {
            root_node.set_label(title.to_string());
        }

        self.ids_stack.push(ROOT_NODE_ID);
        self.nodes_stack.push(root_node);
        self.focus = None;
        self.focus_is_fallback = false;
        self.active_descendant = None;
        mem::swap(&mut self.claims, &mut self.rendered_claims);
        self.claims.clear();
    }

    /// Returns whether a node with the given ID has been pushed in this frame.
    pub(crate) fn has_node(&self, id: NodeId) -> bool {
        id == ROOT_NODE_ID || self.seen_ids.contains(&id)
    }

    /// Returns whether `id` is the node currently reported as focused.
    pub(crate) fn node_is_focused(&self, id: NodeId) -> bool {
        self.focus == Some(id)
    }

    pub(crate) fn focus_is_ancestor_of_current(&self) -> bool {
        let Some(focus) = self.focus else {
            return false;
        };

        // The current node is on top of the stack; everything below it is an
        // ancestor.
        let ancestor_count = self.ids_stack.len().saturating_sub(1);
        self.ids_stack[..ancestor_count].contains(&focus)
    }

    pub(crate) fn set_active_descendant(&mut self, id: NodeId) {
        if self
            .active_descendant
            .is_some_and(|existing| existing != id)
        {
            if cfg!(debug_assertions) {
                panic!("active descendant claimed by multiple nodes in one frame");
            } else {
                log::warn!(
                    "a11y: multiple nodes claimed the active descendant this frame; \
                     using last-wins ({id:?})"
                );
            }
        }
        self.active_descendant = Some(id);
    }

    /// Report `id`, an enclosing frame, for a focused element that has no
    /// node; a later [`Self::set_focus`] replaces it without counting as a
    /// second focus.
    pub(crate) fn set_fallback_focus(&mut self, id: NodeId) {
        self.set_focus(id);
        self.focus_is_fallback = true;
    }

    /// Drop a fallback focus so the node that owns the focus can report it.
    pub(crate) fn clear_fallback_focus(&mut self) {
        if self.focus_is_fallback {
            self.focus = None;
            self.focus_is_fallback = false;
        }
    }

    pub(crate) fn set_focus(&mut self, id: NodeId) {
        if self.focus.is_some() {
            if cfg!(debug_assertions) {
                panic!("set_focus called more than once in a single frame");
            } else {
                log::warn!(
                    "a11y: set_focus called more than once in a single frame; \
                     using last-wins ({id:?})"
                );
            }
        }
        self.focus = Some(id);
        self.focus_is_fallback = false;
    }

    fn finalize(&mut self) -> TreeUpdate {
        // Stack should contain only the root node
        debug_assert_eq!(self.ids_stack.len(), 1);
        debug_assert_eq!(self.ids_stack[0], ROOT_NODE_ID);

        if self.ids_stack.len() != 1 {
            log::error!(
                "a11y: Stack imbalance at end of frame: expected 1 (root), got {}. \
                 Some elements may have pushed without popping.",
                self.ids_stack.len()
            );
        }

        // Pop remaining nodes (should just be the root).
        while !self.ids_stack.is_empty() {
            if let (Some(id), Some(node)) = (self.ids_stack.pop(), self.nodes_stack.pop()) {
                self.all_nodes.push((id, node));
            }
        }

        let focus = match self.active_descendant {
            Some(id) if self.has_node(id) => id,
            Some(id) => {
                if cfg!(debug_assertions) {
                    panic!("active_descendant set to {id:?}, which is not in the tree");
                } else {
                    log::warn!("active_descendant set to {id:?}, which is not in the tree");
                    self.focus.unwrap_or(ROOT_NODE_ID)
                }
            }

            _ => self.focus.unwrap_or(ROOT_NODE_ID),
        };

        let nodes = std::mem::take(&mut self.all_nodes);
        let update = TreeUpdate {
            nodes,
            tree: Some(accesskit::Tree::new(ROOT_NODE_ID)),
            tree_id: accesskit::TreeId::ROOT,
            focus,
        };

        Self::repair_tree_update(update)
    }

    /// Accesskit panics on invalid [`TreeUpdate`]s. This function defensively
    /// checks invariants that accesskit panics on, and tries to fix them.
    fn repair_tree_update(mut update: TreeUpdate) -> TreeUpdate {
        let node_ids: FxHashSet<NodeId> = update.nodes.iter().map(|(id, _)| *id).collect();

        // Focus must point to a node in the tree.
        if !node_ids.contains(&update.focus) {
            log::error!(
                "a11y: Focused node {:?} is not in the tree ({} nodes). \
                 Falling back to root. This is a bug in the a11y tree builder.",
                update.focus,
                update.nodes.len()
            );
            update.focus = ROOT_NODE_ID;
        }

        // Every child reference must point to a node in the update.
        for (id, node) in &mut update.nodes {
            let has_invalid_child = node
                .children()
                .iter()
                .any(|child_id| !node_ids.contains(child_id));
            if has_invalid_child {
                let children = node.children();
                let invalid_count = children
                    .iter()
                    .filter(|child_id| !node_ids.contains(child_id))
                    .count();
                log::error!(
                    "a11y: Node {:?} references {} children not present in the tree. \
                     Stripping invalid child references.",
                    id,
                    invalid_count
                );
                let valid: Vec<NodeId> = children
                    .iter()
                    .copied()
                    .filter(|child_id| node_ids.contains(child_id))
                    .collect();
                node.set_children(valid);
            }
        }

        update
    }
}

#[cfg(test)]
mod tests {
    // Import specific items rather than glob-importing `super`, which would pull
    // in gpui's own `test` attribute macro and shadow the standard one.
    use super::{A11y, A11yNodeBuilder, ROOT_NODE_ID};
    use crate::FocusId;
    use accesskit::{NodeId, Role};
    use std::sync::{Arc, atomic::AtomicBool};

    fn test_node() -> accesskit::Node {
        accesskit::Node::new(Role::GenericContainer)
    }

    fn new_builder() -> A11yNodeBuilder {
        let mut builder = A11yNodeBuilder::new();
        builder.begin_frame(None);
        builder
    }

    fn new_a11y() -> A11y {
        let mut a11y = A11y::new(Arc::new(AtomicBool::new(true)), false, None);
        a11y.begin_frame();
        a11y
    }

    #[test]
    fn accessibility_enabled_is_independent_of_activation() {
        for force_disabled in [false, true] {
            for active in [false, true] {
                let mut a11y = A11y::new(Arc::new(AtomicBool::new(active)), force_disabled, None);
                a11y.sync_active_flag();

                assert_eq!(a11y.is_enabled(), !force_disabled);
                assert_eq!(a11y.is_active(), !force_disabled && active);
            }
        }
    }

    #[test]
    fn active_descendant_honored_when_container_focused() {
        let mut builder = new_builder();
        let container = NodeId(1);
        let item = NodeId(2);

        assert!(builder.push(container, test_node()));
        builder.set_focus(container);
        assert!(builder.push(item, test_node()));

        // The item is on top of the stack; the focused container is its
        // ancestor, so the claim is honored.
        assert!(builder.focus_is_ancestor_of_current());
        builder.set_active_descendant(item);

        builder.pop(); // item
        builder.pop(); // container
        let update = builder.finalize();
        assert_eq!(update.focus, item);
    }

    #[test]
    fn active_descendant_honored_for_deep_descendant() {
        let mut builder = new_builder();
        let container = NodeId(1);
        let group = NodeId(2);
        let item = NodeId(3);

        assert!(builder.push(container, test_node()));
        builder.set_focus(container);
        assert!(builder.push(group, test_node()));
        assert!(builder.push(item, test_node()));

        // The item is a grandchild of the focused container; depth doesn't
        // matter, the focused ancestor is still on the stack.
        assert!(builder.focus_is_ancestor_of_current());
        builder.set_active_descendant(item);

        builder.pop(); // item
        builder.pop(); // group
        builder.pop(); // container
        let update = builder.finalize();
        assert_eq!(update.focus, item);
    }

    #[test]
    fn active_descendant_ignored_when_focus_in_other_subtree() {
        let mut builder = new_builder();
        let focused_container = NodeId(1);
        let focused_leaf = NodeId(2);
        let other_container = NodeId(3);
        let other_item = NodeId(4);

        // First subtree holds real focus.
        assert!(builder.push(focused_container, test_node()));
        assert!(builder.push(focused_leaf, test_node()));
        builder.set_focus(focused_leaf);
        builder.pop(); // focused_leaf
        builder.pop(); // focused_container

        // Second subtree: its item would claim the active descendant, but the
        // focus is not on any of its ancestors, so the gate rejects it.
        assert!(builder.push(other_container, test_node()));
        assert!(builder.push(other_item, test_node()));
        assert!(!builder.focus_is_ancestor_of_current());
        builder.pop(); // other_item
        builder.pop(); // other_container

        let update = builder.finalize();
        assert_eq!(update.focus, focused_leaf);
    }

    #[test]
    fn active_descendant_ignored_when_nothing_focused() {
        let mut builder = new_builder();
        let container = NodeId(1);
        let item = NodeId(2);

        assert!(builder.push(container, test_node()));
        assert!(builder.push(item, test_node()));

        // Nothing is focused (focus defaults to the root window node), so the
        // gate rejects the claim.
        assert!(!builder.focus_is_ancestor_of_current());
        builder.pop();
        builder.pop();

        let update = builder.finalize();
        assert_eq!(update.focus, ROOT_NODE_ID);
    }

    #[test]
    fn regular_focus_used_when_no_active_descendant() {
        let mut builder = new_builder();
        let focused = NodeId(1);

        assert!(builder.push(focused, test_node()));
        builder.set_focus(focused);
        builder.pop();

        let update = builder.finalize();
        assert_eq!(update.focus, focused);
    }

    #[test]
    fn focus_is_ancestor_excludes_self_and_non_ancestors() {
        let mut builder = new_builder();
        let container = NodeId(1);
        let item = NodeId(2);

        assert!(builder.push(container, test_node()));
        builder.set_focus(container);

        // With the focused container itself on top, it is not its own (strict)
        // ancestor, so the gate is false.
        assert!(!builder.focus_is_ancestor_of_current());

        assert!(builder.push(item, test_node()));
        // Now the focused container is a strict ancestor of the item on top.
        assert!(builder.focus_is_ancestor_of_current());

        builder.pop();
        builder.pop();
    }

    // The double-claim guard panics only in debug builds; in release it falls
    // back to last-wins with a warning.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "active descendant claimed by multiple nodes")
    )]
    fn multiple_active_descendant_claims_panic_in_debug() {
        let mut builder = new_builder();
        builder.set_active_descendant(NodeId(1));
        builder.set_active_descendant(NodeId(2));
    }

    // Setting focus twice in one frame means two elements both claimed window
    // focus; that panics in debug and falls back to last-wins in release.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "set_focus called more than once")
    )]
    fn setting_focus_twice_panics_in_debug() {
        let mut builder = new_builder();
        builder.set_focus(NodeId(1));
        builder.set_focus(NodeId(2));
    }

    // Focusing a node that was never registered as focusable is a bug: panic in
    // debug, warn in release.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "was not registered with set_focusable")
    )]
    fn set_focus_without_set_focusable() {
        let mut a11y = new_a11y();
        let node = NodeId(1);
        assert!(a11y.nodes.push(node, test_node()));
        // set_focusable was never called for `node`.
        a11y.set_focus(node);
    }

    // The focused node cannot also be its own active descendant: panic in
    // debug, warn in release.
    #[test]
    #[cfg_attr(debug_assertions, should_panic(expected = "on the focused node"))]
    fn set_active_descendant_on_focused_node() {
        let mut a11y = new_a11y();
        let node = NodeId(1);
        assert!(a11y.nodes.push(node, test_node()));
        a11y.set_focusable(node, FocusId::default());
        a11y.set_focus(node);
        a11y.set_active_descendant(node);
    }

    // Two sibling children of a focused container both claim the active
    // descendant (both pass the focus gate). The second claim is a bug: panic
    // in debug, last-wins + warn in release.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "active descendant claimed by multiple nodes")
    )]
    fn two_siblings_claiming_active_descendant() {
        let mut a11y = new_a11y();
        let container = NodeId(1);
        let first = NodeId(2);
        let second = NodeId(3);

        assert!(a11y.nodes.push(container, test_node()));
        a11y.set_focusable(container, FocusId::default());
        a11y.set_focus(container);

        assert!(a11y.nodes.push(first, test_node()));
        a11y.set_active_descendant(first);
        a11y.nodes.pop(); // first

        assert!(a11y.nodes.push(second, test_node()));
        a11y.set_active_descendant(second);
        a11y.nodes.pop(); // second

        a11y.nodes.pop(); // container
    }

    // A focused element with no node reports its focusable frame; a node
    // painted later that owns the focused handle takes over without the
    // "set_focus called more than once" assertion.
    #[test]
    fn a_node_that_owns_the_focus_replaces_the_fallback() {
        let mut a11y = new_a11y();
        let frame = NodeId(1);
        let owner = NodeId(2);

        assert!(a11y.nodes.push(frame, test_node()));
        a11y.set_focusable(frame, FocusId::default());
        assert!(a11y.focus_nearest_focusable_ancestor());
        a11y.nodes.pop(); // frame

        assert!(a11y.nodes.push(owner, test_node()));
        a11y.set_focusable(owner, FocusId::default());
        a11y.set_focus(owner);
        a11y.nodes.pop(); // owner

        let update = a11y.end_frame(Default::default());
        assert_eq!(update.focus, owner);
    }

    // Node A is focused; node C (a child of the unfocused node B) claims the
    // active descendant. The final tree must still report A as focused.
    #[test]
    fn active_descendant_in_unfocused_subtree_keeps_real_focus() {
        let mut a11y = new_a11y();
        let a = NodeId(1);
        let b = NodeId(2);
        let c = NodeId(3);

        assert!(a11y.nodes.push(a, test_node()));
        a11y.set_focusable(a, FocusId::default());
        a11y.set_focus(a);
        a11y.nodes.pop(); // a

        assert!(a11y.nodes.push(b, test_node()));
        assert!(a11y.nodes.push(c, test_node()));
        a11y.set_active_descendant(c);
        a11y.nodes.pop(); // c
        a11y.nodes.pop(); // b

        let update = a11y.end_frame(Default::default());
        assert_eq!(update.focus, a);
    }
}
