//! Presentation state: what the user is looking at. Domain state lives in
//! the application `Workspace`; this is only navigation and transient UI.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tessera_app::cartography::MapLayout;
use tessera_domain::{Inventory, NodeId};
use tessera_voronoi::{Polygon, Site};

use crate::theme;

/// A full list opened from an overview card.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum List {
    Regenerable,
    Largest,
}

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
    /// Open (or, with `None`, close) a full list on the overview.
    ShowList(Option<List>),
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
    pub clock: Clock,
    pub asked: Instant,
    /// Progress already covered by a scroll preview, so a gesture flows
    /// into the zoom without a jump.
    pub pre: f32,
}

/// Whether a transition is moving yet.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Clock {
    /// Holding the first frame until the new layout arrives (zooming out
    /// and fading need it to aim) or patience runs out.
    Waiting,
    Running(Instant),
}

impl Transition {
    /// Clock progress at `now`: 0 while waiting, 1 when the zoom is over.
    pub fn progress(&self, now: Instant) -> f32 {
        match self.clock {
            Clock::Waiting => 0.0,
            Clock::Running(since) => (now - since).as_secs_f32() / ZOOM_DURATION,
        }
    }
}

/// The entry a scope change was about (the cell clicked, or the folder just
/// left). It keeps its label and pulses once when the new map first shows
/// it, so the eye can follow it through the zoom.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Arrival {
    /// The new scope is not on screen yet.
    Awaited(NodeId),
    /// On screen since the instant; over after the pulse.
    Pulsing(NodeId, Instant),
}

impl Arrival {
    pub fn node(self) -> NodeId {
        match self {
            Arrival::Awaited(id) | Arrival::Pulsing(id, _) => id,
        }
    }
}

/// Scroll or pinch on the map, building toward a scope change (ADR-0012).
#[derive(Default)]
pub struct Gesture {
    lean: Lean,
    last_input: Option<Instant>,
    last_frame: Option<Instant>,
    /// Preview progress handed to the transition a step starts.
    pub handoff: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum Lean {
    /// Signed progress toward a step: +1 opens the scope under the pointer,
    /// -1 goes up.
    Building(f32),
    /// A step fired. Input is ignored until it goes quiet, so one flick
    /// (and its momentum tail) moves exactly one level.
    Latched,
}

impl Default for Lean {
    fn default() -> Self {
        Lean::Building(0.0)
    }
}

/// One level of map navigation (ADR-0012).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Step {
    In,
    Up,
}

/// Input pause that ends a gesture (and releases its latch).
const QUIET: Duration = Duration::from_millis(220);
/// Input pause after which an unfinished lean springs back.
const RELEASE: Duration = Duration::from_millis(90);
/// How far a lean goes when there is nowhere to step to.
const RUBBER_BAND: f32 = 0.22;

impl Gesture {
    /// Signed progress toward a step, in (-1, 1).
    pub fn progress(&self) -> f32 {
        match self.lean {
            Lean::Building(acc) => acc,
            Lean::Latched => 0.0,
        }
    }

    /// Advances the gesture by one frame. `delta` is this frame's input in
    /// steps; `can_in` / `can_up` say whether there is a level to step to.
    /// Returns the step that fired, if any.
    pub fn feed(&mut self, now: Instant, delta: f32, can_in: bool, can_up: bool) -> Option<Step> {
        let dt = self.last_frame.map_or(0.0, |t| (now - t).as_secs_f32().min(0.1));
        self.last_frame = Some(now);
        let quiet_for = self.last_input.map(|t| now - t);
        if delta != 0.0 {
            self.last_input = Some(now);
        }
        let mut acc = match self.lean {
            Lean::Latched if delta != 0.0 && quiet_for.is_none_or(|q| q > QUIET) => delta,
            Lean::Latched => return None,
            // A reversal starts over from rest.
            Lean::Building(acc) if delta != 0.0 => delta + if acc != 0.0 && acc.signum() != delta.signum() { 0.0 } else { acc },
            // Released short of a step: spring back.
            Lean::Building(acc) if quiet_for.is_none_or(|q| q > RELEASE) => {
                let acc = acc * (-dt * 14.0).exp();
                if acc.abs() < 0.002 { 0.0 } else { acc }
            }
            Lean::Building(acc) => acc,
        };
        // Nowhere to go: resist like a rubber band.
        let open = if acc > 0.0 { can_in } else { can_up };
        if !open {
            acc = acc.clamp(-RUBBER_BAND, RUBBER_BAND);
        }
        if open && acc.abs() >= 1.0 {
            self.lean = Lean::Latched;
            return Some(if acc > 0.0 { Step::In } else { Step::Up });
        }
        self.lean = Lean::Building(acc);
        None
    }

    /// While a transition runs, input only keeps the latch alive.
    pub fn hold(&mut self, now: Instant, input: bool) {
        if input {
            self.last_input = Some(now);
        }
        if let Lean::Building(acc) = &mut self.lean {
            *acc = 0.0;
        }
        self.last_frame = None;
    }
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
    pub arrival: Option<Arrival>,
    /// What the pointer is over in the map: (entry, scope a click enters).
    pub map_hover: Option<(NodeId, NodeId)>,
    /// Smooth tree scrolling: (from, to, start).
    pub tree_glide: Option<(f32, f32, Instant)>,
    /// Scrollbar drag: tree offset when the drag began.
    pub tree_drag: Option<f32>,
    pub overview_drag: Option<f32>,
    /// Full list shown in place of the overview, with its own scroll.
    pub list: Option<List>,
    pub list_scroll: f32,
    pub list_drag: Option<f32>,
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
            arrival: None,
            map_hover: None,
            tree_glide: None,
            tree_drag: None,
            overview_drag: None,
            list: None,
            list_scroll: 0.0,
            list_drag: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(16);

    /// One flick, with its momentum tail, moves exactly one level; after a
    /// pause the next flick steps again.
    #[test]
    fn a_flick_steps_once_until_input_goes_quiet() {
        let mut g = Gesture::default();
        let mut now = Instant::now();
        let mut steps = 0;
        for _ in 0..30 {
            steps += g.feed(now, 0.3, true, true).is_some() as usize;
            now += FRAME;
        }
        assert_eq!(steps, 1);
        assert_eq!(g.progress(), 0.0);
        now += QUIET + FRAME;
        assert_eq!(g.feed(now, 0.6, true, true), None);
        assert_eq!(g.feed(now + FRAME, 0.6, true, true), Some(Step::In));
    }

    #[test]
    fn a_reversal_starts_over_and_steps_the_other_way() {
        let mut g = Gesture::default();
        let now = Instant::now();
        assert_eq!(g.feed(now, 0.8, true, true), None);
        assert_eq!(g.feed(now + FRAME, -0.5, true, true), None);
        assert_eq!(g.progress(), -0.5);
        assert_eq!(g.feed(now + FRAME * 2, -0.5, true, true), Some(Step::Up));
    }

    #[test]
    fn nowhere_to_go_resists_and_an_unfinished_lean_springs_back() {
        let mut g = Gesture::default();
        let mut now = Instant::now();
        assert_eq!(g.feed(now, 5.0, false, true), None);
        assert_eq!(g.progress(), RUBBER_BAND);
        now += RELEASE + FRAME;
        for _ in 0..120 {
            g.feed(now, 0.0, false, true);
            now += FRAME;
        }
        assert_eq!(g.progress(), 0.0);
    }

    /// Momentum arriving during the zoom keeps the latch, so the tail of
    /// the flick that started the zoom cannot start another.
    #[test]
    fn input_during_a_transition_keeps_the_latch() {
        let mut g = Gesture::default();
        let mut now = Instant::now();
        assert_eq!(g.feed(now, 1.0, true, true), Some(Step::In));
        for _ in 0..40 {
            now += FRAME;
            g.hold(now, true);
        }
        assert_eq!(g.feed(now + FRAME, 1.0, true, true), None);
    }
}
