//! Presentation state: what the user is looking at. Domain state lives in
//! the application `Workspace`; this is only navigation and transient UI.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tessera_app::cartography::MapLayout;
use tessera_domain::{Inventory, NodeId};
use tessera_voronoi::{Polygon, Site};

use crate::theme;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Map,
}

/// Intent emitted by views; applied by the app after the frame is built.
#[derive(Clone, Debug)]
pub enum Action {
    Select(NodeId),
    /// Make `id` the map's scope. The map animates the change: zooming in
    /// to a descendant, out to an ancestor, cross-fading otherwise.
    Focus(NodeId),
    /// Map scope one level up (Esc, Backspace, mouse back button).
    FocusUp,
    ToggleMark(NodeId),
    Unmark(NodeId),
    ToggleExpand(NodeId),
    SetTab(Tab),
    AskCommit,
    CancelCommit,
    Commit,
    Rescan,
    OpenFolder,
    Reveal(NodeId),
    OpenPrivacySettings,
    DragWindow,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub text: String,
    pub ok: bool,
    pub at: Instant,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ZoomDir {
    /// Into `pivot`, a cell of the old layout.
    In,
    /// Out of `pivot`, which is a cell of the new layout.
    Out,
    /// Unrelated scopes: cross-fade.
    Fade,
}

/// Animated change of map scope (ADR-0010). The camera moves through the
/// old layout while the new one grows out of (or shrinks into) the pivot
/// cell, so the eye never loses its place in the hierarchy.
pub struct Transition {
    pub old: Arc<MapLayout>,
    pub dir: ZoomDir,
    pub pivot: NodeId,
    /// Clock start; zooming out waits for the new layout so it can aim.
    pub start: Option<Instant>,
    pub asked: Instant,
    /// Progress already covered by a scroll preview, so a gesture flows
    /// into the zoom without a jump.
    pub pre: f32,
}

/// Scroll or pinch on the map, building toward a scope change (ADR-0010).
/// `acc` is signed progress: +1 opens the scope under the pointer, -1 goes
/// up. Once a step fires the gesture is `latched` until input goes quiet,
/// so one flick (and its momentum tail) moves exactly one level.
#[derive(Default)]
pub struct Gesture {
    pub acc: f32,
    pub last_input: Option<Instant>,
    pub last_frame: Option<Instant>,
    pub latched: bool,
    /// Preview progress handed to the transition a step starts.
    pub handoff: f32,
}

pub const ZOOM_DURATION: f32 = 0.46;
pub const FADE_DURATION: f32 = 0.22;
/// Longest a transition waits for the new layout before cross-fading.
pub const ZOOM_PATIENCE: Duration = Duration::from_millis(450);

#[derive(Default)]
pub struct Composition {
    pub key: (u64, u32),
    pub cells: Vec<Polygon>,
    pub sites: Vec<Site>,
}

pub struct ViewState {
    pub tab: Tab,
    pub focus: NodeId,
    pub selected: Option<NodeId>,
    pub expanded: HashSet<NodeId>,
    pub tree_scroll: f32,
    pub overview_scroll: f32,
    pub reveal_selected: bool,
    pub confirm: bool,
    pub toast: Option<Toast>,
    pub transition: Option<Transition>,
    pub gesture: Gesture,
    /// What the pointer is over in the map: (entry, scope a click enters).
    pub map_hover: Option<(NodeId, NodeId)>,
    /// Smooth tree scrolling: (from, to, start).
    pub tree_glide: Option<(f32, f32, Instant)>,
    /// Scrollbar drag: tree offset when the drag began.
    pub tree_drag: Option<f32>,
    pub overview_drag: Option<f32>,
    /// Layout currently on screen and when it first appeared.
    pub shown: Option<(usize, NodeId, Instant)>,
    pub last_request: Option<(NodeId, u64, (u32, u32), Instant)>,
    sorted: HashMap<NodeId, (u64, Instant, Arc<[NodeId]>)>,
    pub composition: Composition,
    hues: HashMap<NodeId, (u64, u16)>,
}

impl Default for ViewState {
    fn default() -> Self {
        let mut expanded = HashSet::new();
        expanded.insert(NodeId::ROOT);
        ViewState {
            tab: Tab::Overview,
            focus: NodeId::ROOT,
            selected: None,
            expanded,
            tree_scroll: 0.0,
            overview_scroll: 0.0,
            reveal_selected: false,
            confirm: false,
            toast: None,
            transition: None,
            gesture: Gesture::default(),
            map_hover: None,
            tree_glide: None,
            tree_drag: None,
            overview_drag: None,
            shown: None,
            last_request: None,
            sorted: HashMap::new(),
            composition: Composition::default(),
            hues: HashMap::new(),
        }
    }
}

impl ViewState {
    /// Children of `id` largest first. Cached; while a scan is mutating the
    /// tree the order is refreshed at most every 400 ms to keep rows stable.
    pub fn sorted_children(&mut self, inv: &Inventory, id: NodeId) -> Arc<[NodeId]> {
        let generation = inv.generation();
        if let Some((g, at, v)) = self.sorted.get(&id)
            && (*g == generation || at.elapsed() < Duration::from_millis(400))
        {
            return v.clone();
        }
        let v: Arc<[NodeId]> = inv.children_by_size(id).into();
        self.sorted.insert(id, (generation, Instant::now(), v.clone()));
        v
    }

    pub fn reset(&mut self) {
        *self = ViewState { tab: self.tab, ..ViewState::default() };
    }

    pub fn toast(&mut self, text: impl Into<String>, ok: bool) {
        self.toast = Some(Toast { text: text.into(), ok, at: Instant::now() });
    }

    /// Expands every ancestor of `id` so the tree can show it.
    pub fn reveal(&mut self, inv: &Inventory, id: NodeId) {
        for a in inv.ancestors(id).skip(1) {
            self.expanded.insert(a);
        }
        self.reveal_selected = true;
    }

    /// Colour family of `id`, defined by the hierarchy alone (ADR-0011):
    /// `hue(root) = 0`, `hue(child) = hue(parent) + rank among siblings`.
    /// The largest child keeps its parent's hue, so zooming in or out never
    /// repaints what stays on screen, and tree and map always agree.
    pub fn hue(&mut self, inv: &Inventory, id: NodeId) -> u16 {
        let generation = inv.generation();
        if let Some(&(g, h)) = self.hues.get(&id)
            && g == generation
        {
            return h;
        }
        let h = match inv.get(id).and_then(|n| n.parent()) {
            None => 0,
            Some(p) => {
                let rank = self.sorted_children(inv, p).iter().position(|&k| k == id).unwrap_or(0);
                (self.hue(inv, p) as usize + rank) as u16 % theme::BRANCH.len() as u16
            }
        };
        self.hues.insert(id, (generation, h));
        h
    }

    /// The family `id` is drawn in right now: that of its ancestor-or-self
    /// which is a child of the nearest scope containing it (the focus, or an
    /// ancestor of the focus for entries outside it).
    pub fn family(&mut self, inv: &Inventory, id: NodeId) -> u16 {
        let scope: Vec<NodeId> = inv.ancestors(self.focus).collect();
        let mut head = id;
        for a in inv.ancestors(id) {
            if scope.contains(&a) {
                break;
            }
            head = a;
        }
        self.hue(inv, head)
    }
}
