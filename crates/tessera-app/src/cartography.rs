//! Cartography context: turns an inventory subtree into a nested Voronoi
//! treemap (ADR-0004).
//!
//! Work happens in two phases so the inventory lock is held only briefly:
//! 1. **snapshot** – under a read lock, pick the cells worth drawing using
//!    *estimated* areas (a cell's area is proportional to its size, so we
//!    know how big it will be before solving anything);
//! 2. **solve** – lock free, solve each level with the power-diagram solver,
//!    siblings in parallel, warm-started from the previous layout.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use parking_lot::{Mutex, RwLock};
use rayon::prelude::*;
use tessera_domain::{Inventory, NodeId};
use tessera_voronoi::{Polygon, Settings, Site, SplitMix64, Vec2};

use crate::scan::Waker;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CellKind {
    Dir,
    File,
    /// Aggregate of siblings too small to draw individually.
    Others {
        count: u32,
    },
}

#[derive(Clone, Debug)]
pub struct MapCell {
    /// The entry; for [`CellKind::Others`] the directory they belong to.
    pub node: NodeId,
    pub kind: CellKind,
    /// 1 = direct child of the focus.
    pub depth: u8,
    /// Index of the depth-1 ancestor; drives the colour family.
    pub branch: u16,
    /// Rank among siblings, 0 = largest.
    pub rank: u16,
    pub size: u64,
    /// Fraction of the parent's size.
    pub share: f64,
    pub polygon: Polygon,
    /// Index of the parent cell in [`MapLayout::cells`].
    pub parent: Option<u32>,
    pub has_children: bool,
}

#[derive(Clone, Debug)]
pub struct MapLayout {
    /// Identity of the inventory this layout was computed from; layouts
    /// from a previous scan must never be indexed into a new inventory.
    pub source: usize,
    pub focus: NodeId,
    pub generation: u64,
    pub width: f64,
    pub height: f64,
    /// Pre-order: every parent precedes its children.
    pub cells: Vec<MapCell>,
    pub elapsed: Duration,
}

impl MapLayout {
    /// Deepest cell containing `p`.
    pub fn hit(&self, p: Vec2) -> Option<usize> {
        // Pre-order + non-overlapping siblings ⇒ the first hit scanning
        // backwards is the deepest.
        self.cells.iter().rposition(|c| {
            let (min, max) = c.polygon.bbox();
            p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y && c.polygon.contains(p)
        })
    }
}

#[derive(Clone, Debug)]
pub struct MapParams {
    pub max_depth: u8,
    /// Cells smaller than this (px²) are folded into "others".
    pub min_cell_area: f64,
    /// Directories at least this large (px²) show their contents.
    pub recurse_area: f64,
    pub max_children: usize,
}

impl Default for MapParams {
    fn default() -> Self {
        MapParams { max_depth: 3, min_cell_area: 64.0, recurse_area: 3200.0, max_children: 64 }
    }
}

impl MapParams {
    /// Inset between a directory cell and the cells nested inside it.
    pub fn padding(&self, depth: u8) -> f64 {
        if depth <= 1 { 5.0 } else { 3.0 }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Node(NodeId),
    Others,
}

struct Snap {
    key: Key,
    node: NodeId,
    kind: CellKind,
    size: u64,
    children: Vec<Snap>,
}

fn snapshot(inv: &Inventory, id: NodeId, area: f64, depth: u8, p: &MapParams) -> Vec<Snap> {
    let parent_size = inv.node(id).size();
    if parent_size == 0 || area <= 0.0 {
        return Vec::new();
    }
    let mut kids: Vec<(u64, NodeId)> = inv.children(id).map(|c| (inv.node(c).size(), c)).filter(|k| k.0 > 0).collect();
    let min_size = (parent_size as f64 * p.min_cell_area / area).ceil() as u64;
    if kids.len() > p.max_children {
        kids.select_nth_unstable_by(p.max_children, |a, b| b.0.cmp(&a.0));
    }
    let head = kids.len().min(p.max_children);
    kids[..head].sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let keep = kids[..head].iter().take_while(|k| k.0 >= min_size).count();

    let mut out: Vec<Snap> = kids[..keep]
        .iter()
        .map(|&(size, c)| {
            let n = inv.node(c);
            let child_area = area * size as f64 / parent_size as f64;
            let children = if n.is_dir() && depth < p.max_depth && child_area >= p.recurse_area {
                // ~15% of a cell goes to padding and gaps.
                snapshot(inv, c, child_area * 0.85, depth + 1, p)
            } else {
                Vec::new()
            };
            Snap { key: Key::Node(c), node: c, kind: if n.is_dir() { CellKind::Dir } else { CellKind::File }, size, children }
        })
        .collect();
    let rest = &kids[keep..];
    let others: u64 = rest.iter().map(|k| k.0).sum();
    if others > 0 && others >= min_size {
        out.push(Snap { key: Key::Others, node: id, kind: CellKind::Others { count: rest.len() as u32 }, size: others, children: Vec::new() });
    }
    out
}

struct WarmEntry {
    min: Vec2,
    max: Vec2,
    keys: Vec<Key>,
    sites: Vec<Site>,
}

/// Previous solutions, keyed by the parent directory of each level.
#[derive(Default)]
struct WarmStart(HashMap<NodeId, WarmEntry>);

impl WarmStart {
    fn lookup(&self, parent: NodeId, container: &Polygon, keys: &[Key]) -> Option<Vec<Site>> {
        let e = self.0.get(&parent)?;
        let (min, max) = container.bbox();
        let sx = (max.x - min.x) / (e.max.x - e.min.x).max(1e-9);
        let sy = (max.y - min.y) / (e.max.y - e.min.y).max(1e-9);
        let index: HashMap<Key, &Site> = e.keys.iter().copied().zip(e.sites.iter()).collect();
        let mut found = 0;
        let mut min_w = f64::INFINITY;
        let mut sites: Vec<Option<Site>> = keys
            .iter()
            .map(|k| {
                index.get(k).map(|s| {
                    found += 1;
                    let w = s.weight * sx * sy;
                    min_w = min_w.min(w);
                    Site { pos: Vec2::new(min.x + (s.pos.x - e.min.x) * sx, min.y + (s.pos.y - e.min.y) * sy), weight: w }
                })
            })
            .collect();
        if found == 0 {
            return None;
        }
        let mut rng = SplitMix64::new(parent.0 as u64);
        Some(sites.iter_mut().map(|s| s.take().unwrap_or_else(|| Site { pos: container.random_point(&mut rng).unwrap_or_default(), weight: min_w })).collect())
    }
}

struct Solver<'a> {
    params: &'a MapParams,
    warm: &'a WarmStart,
    next_warm: Mutex<WarmStart>,
}

impl Solver<'_> {
    fn level(&self, snaps: &[Snap], container: &Polygon, parent: NodeId, depth: u8, branch: Option<u16>) -> Vec<MapCell> {
        if snaps.is_empty() {
            return Vec::new();
        }
        let values: Vec<f64> = snaps.iter().map(|s| s.size as f64).collect();
        let keys: Vec<Key> = snaps.iter().map(|s| s.key).collect();
        let init = self.warm.lookup(parent, container, &keys);
        let settings = if depth == 1 { Settings { max_iterations: 120, convergence: 0.008 } } else { Settings { max_iterations: 90, convergence: 0.015 } };
        let sol = tessera_voronoi::solve(container, &values, init.as_deref(), parent.0 as u64, &settings);
        let (min, max) = container.bbox();
        self.next_warm.lock().0.insert(parent, WarmEntry { min, max, keys, sites: sol.sites.clone() });

        let total: f64 = values.iter().sum();
        let groups: Vec<Vec<MapCell>> = snaps
            .par_iter()
            .zip(sol.cells.par_iter())
            .enumerate()
            .map(|(i, (s, poly))| {
                if poly.is_empty() {
                    return Vec::new();
                }
                let br = branch.unwrap_or(i as u16);
                let mut v = vec![MapCell {
                    node: s.node,
                    kind: s.kind,
                    depth,
                    branch: br,
                    rank: i as u16,
                    size: s.size,
                    share: s.size as f64 / total,
                    polygon: poly.clone(),
                    parent: None,
                    has_children: false,
                }];
                if !s.children.is_empty() {
                    let inner = poly.inset(self.params.padding(depth));
                    if inner.area() >= self.params.min_cell_area * 2.0 {
                        let sub = self.level(&s.children, &inner, s.node, depth + 1, Some(br));
                        v[0].has_children = !sub.is_empty();
                        v.extend(sub.into_iter().map(|mut c| {
                            c.parent = Some(c.parent.map_or(0, |p| p + 1));
                            c
                        }));
                    }
                }
                v
            })
            .collect();

        let mut out = Vec::with_capacity(groups.iter().map(Vec::len).sum());
        for g in groups {
            let off = out.len() as u32;
            out.extend(g.into_iter().map(|mut c| {
                c.parent = c.parent.map(|p| p + off);
                c
            }));
        }
        out
    }
}

#[derive(Clone)]
pub struct MapRequest {
    pub inventory: Arc<RwLock<Inventory>>,
    pub focus: NodeId,
    pub width: f64,
    pub height: f64,
}

/// Background service that keeps the most recent [`MapLayout`]. Requests
/// coalesce: only the newest pending request is ever solved.
pub struct Cartographer {
    tx: Sender<MapRequest>,
    latest: Arc<Mutex<Option<Arc<MapLayout>>>>,
    busy: Arc<AtomicBool>,
}

impl Cartographer {
    pub fn new(params: MapParams, waker: Waker) -> Cartographer {
        let (tx, rx) = crossbeam_channel::unbounded::<MapRequest>();
        let latest: Arc<Mutex<Option<Arc<MapLayout>>>> = Arc::default();
        let busy = Arc::new(AtomicBool::new(false));
        let (out, flag) = (latest.clone(), busy.clone());
        std::thread::Builder::new()
            .name("tessera-cartographer".into())
            .spawn(move || {
                let mut warm = WarmStart::default();
                let mut inv_ptr: *const RwLock<Inventory> = std::ptr::null();
                while let Ok(mut req) = rx.recv() {
                    while let Ok(r) = rx.try_recv() {
                        req = r;
                    }
                    flag.store(true, Ordering::Relaxed);
                    if Arc::as_ptr(&req.inventory) != inv_ptr {
                        warm = WarmStart::default();
                        inv_ptr = Arc::as_ptr(&req.inventory);
                    }
                    let layout = compute(&req, &params, &mut warm);
                    *out.lock() = Some(Arc::new(layout));
                    flag.store(false, Ordering::Relaxed);
                    waker();
                }
            })
            .expect("cartographer thread");
        Cartographer { tx, latest, busy }
    }

    pub fn request(&self, req: MapRequest) {
        self.busy.store(true, Ordering::Relaxed);
        let _ = self.tx.send(req);
    }

    pub fn latest(&self) -> Option<Arc<MapLayout>> {
        self.latest.lock().clone()
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Relaxed)
    }
}

fn compute(req: &MapRequest, params: &MapParams, warm: &mut WarmStart) -> MapLayout {
    let t = Instant::now();
    let container = Polygon::rect(Vec2::new(0.0, 0.0), Vec2::new(req.width, req.height));
    let (snaps, generation) = {
        let inv = req.inventory.read();
        let focus = if inv.get(req.focus).is_some_and(|n| !n.is_removed()) { req.focus } else { NodeId::ROOT };
        (snapshot(&inv, focus, container.area(), 1, params), inv.generation())
    };
    let solver = Solver { params, warm, next_warm: Mutex::default() };
    let cells = solver.level(&snaps, &container, req.focus, 1, None);
    let next = solver.next_warm.into_inner();
    // Keep warm entries of levels that were not re-solved this time.
    warm.0.extend(next.0);
    MapLayout { source: Arc::as_ptr(&req.inventory) as usize, focus: req.focus, generation, width: req.width, height: req.height, cells, elapsed: t.elapsed() }
}

/// Solves a layout synchronously (tests, headless snapshots).
pub fn compute_now(req: &MapRequest, params: &MapParams) -> MapLayout {
    compute(req, params, &mut WarmStart::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tessera_domain::{EntryKind, Listing, ScanPolicy};

    fn inventory() -> Arc<RwLock<Inventory>> {
        let mut inv = Inventory::new("/r");
        let p = ScanPolicy::default();
        let mut l = Listing::default();
        for i in 0..40u64 {
            l.push(&format!("d{i}"), EntryKind::Dir, 0, i, 1, false);
        }
        for i in 0..400u64 {
            l.push(&format!("f{i}"), EntryKind::File, 1 + i, 1000 + i, 1, false);
        }
        let mut next = vec![];
        inv.ingest(NodeId::ROOT, &l, &p, &mut next);
        for (k, d) in next.clone().into_iter().enumerate() {
            let mut l = Listing::default();
            for j in 0..20u64 {
                l.push(&format!("g{j}"), EntryKind::File, (k as u64 + 1) * 10_000 + j * 500, 5000 + j + k as u64 * 100, 1, false);
            }
            inv.ingest(d, &l, &p, &mut vec![]);
        }
        Arc::new(RwLock::new(inv))
    }

    #[test]
    fn layout_is_nested_and_folds_small_files() {
        let inv = inventory();
        let req = MapRequest { inventory: inv, focus: NodeId::ROOT, width: 1200.0, height: 800.0 };
        let l = compute_now(&req, &MapParams::default());
        let top: Vec<&MapCell> = l.cells.iter().filter(|c| c.depth == 1).collect();
        assert!(top.len() <= 64);
        assert!(top.iter().any(|c| matches!(c.kind, CellKind::Others { .. })), "small files folded");
        assert!(l.cells.iter().any(|c| c.depth == 2), "big dirs show contents");
        for (i, c) in l.cells.iter().enumerate() {
            if let Some(p) = c.parent {
                assert!((p as usize) < i, "parents precede children");
                assert_eq!(l.cells[p as usize].depth + 1, c.depth);
            }
        }
        let area: f64 = top.iter().map(|c| c.polygon.area()).sum();
        assert!((area - 1200.0 * 800.0).abs() < 1.0);
        let probe = l.cells.iter().find(|c| c.depth == 2).unwrap().polygon.centroid();
        assert_eq!(l.cells[l.hit(probe).unwrap()].depth, 2);
    }
}
