use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::category::Category;
use crate::hint::ReclaimHint;

/// Identity of an entry inside one [`Inventory`]. Stable for the lifetime of
/// the inventory (removal tombstones the node; ids are never reused).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const ROOT: NodeId = NodeId(0);

    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

const NO_PARENT: u32 = u32::MAX;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

mod flag {
    pub const SCANNED: u8 = 1 << 0;
    pub const FAILED: u8 = 1 << 1;
    pub const REMOVED: u8 = 1 << 2;
    /// A mount point the scan policy told us not to cross.
    pub const BOUNDARY: u8 = 1 << 3;
    /// A hard link whose bytes were already counted elsewhere.
    pub const SHARED: u8 = 1 << 4;
    /// Privacy-protected; deliberately not read.
    pub const PROTECTED: u8 = 1 << 5;
}

/// One entry of the inventory tree. 40 bytes; names live in a shared arena.
#[derive(Clone, Debug)]
pub struct Node {
    parent: u32,
    name_off: u32,
    child_start: u32,
    child_count: u32,
    size: u64,
    files: u32,
    dirs: u32,
    name_len: u16,
    depth: u16,
    kind: EntryKind,
    category: Category,
    hint: ReclaimHint,
    flags: u8,
}

impl Node {
    /// Filler for id ranges reserved by a scanner but not yet ingested.
    /// Unreachable from the tree and skipped by iteration (flagged removed).
    const PLACEHOLDER: Node = Node {
        parent: NO_PARENT,
        name_off: 0,
        child_start: 0,
        child_count: 0,
        size: 0,
        files: 0,
        dirs: 0,
        name_len: 0,
        depth: 0,
        kind: EntryKind::Other,
        category: Category::Other,
        hint: ReclaimHint::None,
        flags: flag::REMOVED | flag::SCANNED,
    };

    /// Allocated bytes of the whole subtree (what removing it would free).
    pub fn size(&self) -> u64 {
        self.size
    }
    /// Regular files in the subtree, counting itself.
    pub fn files(&self) -> u32 {
        self.files
    }
    /// Directories strictly below this node.
    pub fn dirs(&self) -> u32 {
        self.dirs
    }
    pub fn kind(&self) -> EntryKind {
        self.kind
    }
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir
    }
    pub fn category(&self) -> Category {
        self.category
    }
    pub fn hint(&self) -> ReclaimHint {
        self.hint
    }
    pub fn depth(&self) -> u16 {
        self.depth
    }
    pub fn parent(&self) -> Option<NodeId> {
        (self.parent != NO_PARENT).then_some(NodeId(self.parent))
    }
    pub fn child_count(&self) -> u32 {
        self.child_count
    }
    pub fn is_scanned(&self) -> bool {
        self.flags & flag::SCANNED != 0
    }
    pub fn is_failed(&self) -> bool {
        self.flags & flag::FAILED != 0
    }
    pub fn is_removed(&self) -> bool {
        self.flags & flag::REMOVED != 0
    }
    pub fn is_boundary(&self) -> bool {
        self.flags & flag::BOUNDARY != 0
    }
    pub fn is_shared(&self) -> bool {
        self.flags & flag::SHARED != 0
    }
    pub fn is_protected(&self) -> bool {
        self.flags & flag::PROTECTED != 0
    }
}

/// Raw directory contents as reported by a directory reader (infrastructure).
/// Names are packed into one string to keep per-entry allocations at zero.
#[derive(Default, Debug)]
pub struct Listing {
    names: String,
    entries: Vec<RawEntry>,
}

#[derive(Copy, Clone, Debug)]
pub struct RawEntry {
    name_off: u32,
    name_len: u16,
    pub kind: EntryKind,
    /// Bytes allocated on disk.
    pub alloc: u64,
    pub inode: u64,
    pub nlink: u32,
    pub mount_point: bool,
}

impl Listing {
    pub fn clear(&mut self) {
        self.names.clear();
        self.entries.clear();
    }

    pub fn push(&mut self, name: &str, kind: EntryKind, alloc: u64, inode: u64, nlink: u32, mount_point: bool) {
        let name_off = self.names.len() as u32;
        let name_len = name.len().min(u16::MAX as usize) as u16;
        self.names.push_str(&name[..name_len as usize]);
        self.entries.push(RawEntry { name_off, name_len, kind, alloc, inode, nlink, mount_point });
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn name(&self, e: &RawEntry) -> &str {
        &self.names[e.name_off as usize..e.name_off as usize + e.name_len as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &RawEntry)> {
        self.entries.iter().map(|e| (self.name(e), e))
    }

    pub fn contains_name(&self, name: &str) -> bool {
        self.iter().any(|(n, _)| n == name)
    }
}

/// How a scan treats the filesystem it walks.
#[derive(Clone, Debug)]
pub struct ScanPolicy {
    /// Do not descend into other mounted volumes.
    pub one_filesystem: bool,
    /// Directories to seal instead of reading (see [`crate::protected`]).
    pub protected: Arc<HashSet<PathBuf>>,
}

impl Default for ScanPolicy {
    fn default() -> Self {
        ScanPolicy { one_filesystem: true, protected: Arc::default() }
    }
}

impl ScanPolicy {
    pub fn is_protected(&self, path: &Path) -> bool {
        !self.protected.is_empty() && self.protected.contains(path)
    }

    /// Decides what happens to a child directory found at `path`.
    pub fn descend(&self, entry: &RawEntry, path: &Path) -> Descend {
        if entry.mount_point && self.one_filesystem {
            Descend::Boundary
        } else if self.is_protected(path) {
            Descend::Protected
        } else {
            Descend::Read
        }
    }
}

/// What a scan does with a child directory.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Descend {
    Read,
    /// A mount point the policy does not cross.
    Boundary,
    /// Privacy-protected; sealed rather than read.
    Protected,
}

/// Aggregate root of the Inventory context.
///
/// Invariants:
/// * a directory's `size`/`files`/`dirs` always equal the sums over its live
///   children (maintained incrementally on every ingest and removal);
/// * children of a directory occupy a contiguous id range, assigned when the
///   directory's listing is ingested; a parent's id is always smaller than
///   its children's;
/// * removed nodes (and their whole subtree) are tombstoned, never reused.
#[derive(Debug)]
pub struct Inventory {
    root_path: PathBuf,
    nodes: Vec<Node>,
    names: String,
    hardlinks: HashSet<u64>,
    category_bytes: [u64; Category::COUNT],
    category_files: [u32; Category::COUNT],
    pending: u32,
    failures: u32,
    generation: u64,
}

impl Inventory {
    pub fn new(root_path: impl Into<PathBuf>) -> Self {
        let root_path: PathBuf = root_path.into();
        let display = root_path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| root_path.to_string_lossy().into_owned());
        let mut names = String::new();
        names.push_str(&display);
        let root = Node {
            parent: NO_PARENT,
            name_off: 0,
            child_start: 0,
            child_count: 0,
            size: 0,
            files: 0,
            dirs: 0,
            name_len: display.len() as u16,
            depth: 0,
            kind: EntryKind::Dir,
            category: Category::Folder,
            hint: ReclaimHint::None,
            flags: 0,
        };
        Inventory {
            root_path,
            nodes: vec![root],
            names,
            hardlinks: HashSet::new(),
            category_bytes: [0; Category::COUNT],
            category_files: [0; Category::COUNT],
            pending: 1,
            failures: 0,
            generation: 0,
        }
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    /// Monotonic counter bumped on every mutation; cheap change detection for
    /// read models and caches.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Directories queued but not yet ingested.
    pub fn pending(&self) -> u32 {
        self.pending
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }

    pub fn is_complete(&self) -> bool {
        self.pending == 0
    }

    #[inline]
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().enumerate().map(|(i, n)| (NodeId(i as u32), n))
    }

    pub fn name(&self, id: NodeId) -> &str {
        let n = self.node(id);
        &self.names[n.name_off as usize..n.name_off as usize + n.name_len as usize]
    }

    pub fn total_size(&self) -> u64 {
        self.nodes[0].size
    }

    pub fn category_bytes(&self, c: Category) -> u64 {
        self.category_bytes[c.index()]
    }

    pub fn category_files(&self, c: Category) -> u32 {
        self.category_files[c.index()]
    }

    /// Live children, in arena (filesystem) order.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let n = self.node(id);
        (n.child_start..n.child_start + n.child_count).map(NodeId).filter(|c| !self.nodes[c.index()].is_removed())
    }

    /// Live children ordered largest first (ties broken by name).
    pub fn children_by_size(&self, id: NodeId) -> Vec<NodeId> {
        let mut v: Vec<NodeId> = self.children(id).collect();
        v.sort_unstable_by(|a, b| self.node(*b).size.cmp(&self.node(*a).size).then_with(|| self.name(*a).cmp(self.name(*b))));
        v
    }

    /// `id` itself first, then each parent up to the root.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let mut cur = Some(id);
        std::iter::from_fn(move || {
            let c = cur?;
            cur = self.node(c).parent();
            Some(c)
        })
    }

    /// True if `a` is a strict ancestor of `b`.
    pub fn is_ancestor(&self, a: NodeId, b: NodeId) -> bool {
        a.0 < b.0 && self.ancestors(b).skip(1).any(|x| x == a)
    }

    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut chain: Vec<NodeId> = self.ancestors(id).collect();
        chain.pop(); // root is represented by root_path itself
        let mut p = self.root_path.clone();
        for c in chain.iter().rev() {
            p.push(self.name(*c));
        }
        p
    }

    /// Path relative to the scan root, for compact display.
    pub fn relative_path(&self, id: NodeId) -> String {
        let mut chain: Vec<NodeId> = self.ancestors(id).collect();
        chain.reverse();
        let mut s = String::new();
        for (i, c) in chain.iter().enumerate() {
            if i > 0 {
                s.push('/');
            }
            s.push_str(self.name(*c));
        }
        s
    }

    /// Records the contents of directory `dir`, assigning ids itself.
    /// Directories that should be read next are appended to `next`.
    /// Convenience for sequential callers and tests.
    pub fn ingest(&mut self, dir: NodeId, listing: &Listing, policy: &ScanPolicy, next: &mut Vec<NodeId>) {
        let start = NodeId(self.nodes.len() as u32);
        let fate = |_: usize, e: &RawEntry| if e.mount_point && policy.one_filesystem { Descend::Boundary } else { Descend::Read };
        if self.ingest_at(dir, start, listing, fate) {
            next.extend((0..listing.len() as u32).map(|i| NodeId(start.0 + i)).filter(|c| self.node(*c).is_dir() && !self.node(*c).is_scanned()));
        }
    }

    /// Records the contents of directory `dir` into the pre-reserved id range
    /// `start..start + listing.len()` (see ADR-0006: scanner workers reserve
    /// ranges so they can descend without waiting for this aggregate).
    /// `fate` says what the scanner does with each child directory.
    ///
    /// Callers must ingest a directory before any of its descendants.
    /// Returns false if `dir` was already ingested.
    pub fn ingest_at(&mut self, dir: NodeId, start: NodeId, listing: &Listing, fate: impl Fn(usize, &RawEntry) -> Descend) -> bool {
        let d = dir.index();
        debug_assert!(self.nodes[d].is_dir());
        if self.nodes[d].is_scanned() {
            return false;
        }
        self.pending -= 1;
        self.generation += 1;
        let detached = self.nodes[d].is_removed();
        let s = start.index();
        if self.nodes.len() < s + listing.len() {
            self.nodes.resize(s + listing.len(), Node::PLACEHOLDER);
        }
        let depth = self.nodes[d].depth.saturating_add(1);
        let (mut add_size, mut add_files, mut add_dirs) = (0u64, 0u32, 0u32);
        self.names.reserve(listing.names.len());

        for (i, (name, e)) in listing.iter().enumerate() {
            let name_off = self.names.len() as u32;
            self.names.push_str(name);
            let mut node = Node {
                parent: dir.0,
                name_off,
                child_start: 0,
                child_count: 0,
                size: e.alloc,
                files: 0,
                dirs: 0,
                name_len: name.len() as u16,
                depth,
                kind: e.kind,
                category: Category::Folder,
                hint: ReclaimHint::None,
                flags: 0,
            };
            if e.kind == EntryKind::Dir {
                node.hint = ReclaimHint::detect(name, |s| listing.contains_name(s));
                match fate(i, e) {
                    Descend::Read => self.pending += 1,
                    Descend::Boundary => {
                        node.flags |= flag::BOUNDARY | flag::SCANNED;
                        node.size = 0;
                    }
                    Descend::Protected => node.flags |= flag::PROTECTED | flag::SCANNED,
                }
                add_dirs += 1;
            } else {
                node.category = Category::classify(name);
                node.files = 1;
                node.flags |= flag::SCANNED;
                if !detached {
                    if e.nlink > 1 && !self.hardlinks.insert(e.inode) {
                        node.size = 0;
                        node.flags |= flag::SHARED;
                    }
                    self.category_bytes[node.category.index()] += node.size;
                    self.category_files[node.category.index()] += 1;
                }
                add_files += 1;
            }
            if detached {
                node.flags |= flag::REMOVED;
            }
            add_size += node.size;
            self.nodes[s + i] = node;
        }

        let n = &mut self.nodes[d];
        n.child_start = start.0;
        n.child_count = listing.len() as u32;
        n.flags |= flag::SCANNED;
        if !detached {
            self.propagate(dir, add_size as i64, add_files as i64, add_dirs as i64);
        }
        true
    }

    /// Records that `dir` could not be read; it stays in the tree, empty.
    pub fn fail(&mut self, dir: NodeId) {
        let n = &mut self.nodes[dir.index()];
        if n.is_scanned() {
            return;
        }
        n.flags |= flag::SCANNED | flag::FAILED;
        self.pending -= 1;
        self.failures += 1;
        self.generation += 1;
    }

    /// Tombstones `id` and its subtree, returning the bytes that were
    /// accounted to it. The root cannot be removed.
    pub fn remove(&mut self, id: NodeId) -> u64 {
        if id == NodeId::ROOT || self.node(id).is_removed() {
            return 0;
        }
        let (size, files, dirs, is_dir) = {
            let n = self.node(id);
            (n.size, n.files, n.dirs, n.is_dir())
        };
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let n = &mut self.nodes[cur.index()];
            if n.is_removed() {
                continue;
            }
            n.flags |= flag::REMOVED;
            if n.kind == EntryKind::Dir {
                if !n.is_scanned() {
                    // Still queued; its eventual listing will be dropped.
                    continue;
                }
                let (s, c) = (n.child_start, n.child_count);
                stack.extend((s..s + c).map(NodeId));
            } else {
                let (cat, sz) = (n.category.index(), n.size);
                self.category_bytes[cat] -= sz;
                self.category_files[cat] -= 1;
            }
        }
        let parent = self.node(id).parent().expect("non-root has parent");
        self.propagate(parent, -(size as i64), -(files as i64), -(dirs as i64 + is_dir as i64));
        self.generation += 1;
        size
    }

    fn propagate(&mut self, from: NodeId, size: i64, files: i64, dirs: i64) {
        let mut cur = from.0;
        while cur != NO_PARENT {
            let n = &mut self.nodes[cur as usize];
            n.size = n.size.wrapping_add_signed(size);
            n.files = (n.files as i64 + files) as u32;
            n.dirs = (n.dirs as i64 + dirs) as u32;
            cur = n.parent;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn listing(entries: &[(&str, EntryKind, u64)]) -> Listing {
        let mut l = Listing::default();
        for (i, (n, k, s)) in entries.iter().enumerate() {
            l.push(n, *k, *s, 1000 + i as u64, 1, false);
        }
        l
    }

    /// root/{a.mp4 100, src/{main.rs 10, node_modules/{x.js 5}}, Cargo.toml 1}
    pub fn sample() -> Inventory {
        use EntryKind::*;
        let mut inv = Inventory::new("/tmp/root");
        let p = ScanPolicy::default();
        let mut next = vec![];
        inv.ingest(NodeId::ROOT, &listing(&[("a.mp4", File, 100), ("src", Dir, 0), ("Cargo.toml", File, 1)]), &p, &mut next);
        assert_eq!(next, vec![NodeId(2)]);
        next.clear();
        inv.ingest(NodeId(2), &listing(&[("main.rs", File, 10), ("node_modules", Dir, 0)]), &p, &mut next);
        next.clear();
        inv.ingest(NodeId(5), &listing(&[("x.js", File, 5)]), &p, &mut next);
        inv
    }

    #[test]
    fn totals_roll_up_incrementally() {
        let inv = sample();
        assert!(inv.is_complete());
        assert_eq!(inv.total_size(), 116);
        assert_eq!(inv.node(NodeId::ROOT).files(), 4);
        assert_eq!(inv.node(NodeId::ROOT).dirs(), 2);
        assert_eq!(inv.node(NodeId(2)).size(), 15);
        assert_eq!(inv.category_bytes(Category::Video), 100);
        assert_eq!(inv.node(NodeId(5)).hint(), ReclaimHint::Dependencies);
        assert_eq!(inv.path(NodeId(6)), PathBuf::from("/tmp/root/src/node_modules/x.js"));
        assert_eq!(inv.relative_path(NodeId(6)), "root/src/node_modules/x.js");
    }

    #[test]
    fn children_sorted_by_size() {
        let inv = sample();
        let names: Vec<&str> = inv.children_by_size(NodeId::ROOT).iter().map(|c| inv.name(*c)).collect();
        assert_eq!(names, ["a.mp4", "src", "Cargo.toml"]);
    }

    #[test]
    fn removal_subtracts_subtree() {
        let mut inv = sample();
        assert_eq!(inv.remove(NodeId(2)), 15);
        assert_eq!(inv.total_size(), 101);
        assert_eq!(inv.node(NodeId::ROOT).files(), 2);
        assert_eq!(inv.node(NodeId::ROOT).dirs(), 0);
        assert!(inv.node(NodeId(6)).is_removed());
        assert_eq!(inv.category_bytes(Category::Code), 1, "only Cargo.toml remains");
        assert_eq!(inv.children(NodeId::ROOT).count(), 2);
        assert_eq!(inv.remove(NodeId(2)), 0, "idempotent");
        assert_eq!(inv.remove(NodeId::ROOT), 0, "root is permanent");
    }

    #[test]
    fn hardlinks_are_counted_once() {
        let mut inv = Inventory::new("/r");
        let mut l = Listing::default();
        l.push("a", EntryKind::File, 50, 7, 2, false);
        l.push("b", EntryKind::File, 50, 7, 2, false);
        inv.ingest(NodeId::ROOT, &l, &ScanPolicy::default(), &mut vec![]);
        assert_eq!(inv.total_size(), 50);
        assert!(inv.node(NodeId(2)).is_shared());
    }

    #[test]
    fn mount_points_are_boundaries() {
        let mut inv = Inventory::new("/r");
        let mut l = Listing::default();
        l.push("Volumes", EntryKind::Dir, 0, 1, 1, true);
        let mut next = vec![];
        inv.ingest(NodeId::ROOT, &l, &ScanPolicy::default(), &mut next);
        assert!(next.is_empty());
        assert!(inv.is_complete());
        assert!(inv.node(NodeId(1)).is_boundary());
    }

    #[test]
    fn out_of_order_reserved_ranges() {
        use EntryKind::*;
        let mut inv = Inventory::new("/r");
        // Root's children reserved at 1..3; "a"'s children reserved at 3..4
        // but a worker that reserved 4..6 for "b" reports first.
        inv.ingest_at(NodeId::ROOT, NodeId(1), &listing(&[("a", Dir, 0), ("b", Dir, 0)]), |_, _| Descend::Read);
        inv.ingest_at(NodeId(2), NodeId(4), &listing(&[("x", File, 4), ("p", Dir, 0)]), |_, _| Descend::Protected);
        assert_eq!(inv.len(), 6);
        assert!(inv.node(NodeId(3)).is_removed(), "placeholder until ingested");
        assert_eq!(inv.nodes().filter(|(_, n)| !n.is_removed()).count(), 5);
        inv.ingest_at(NodeId(1), NodeId(3), &listing(&[("y", File, 6)]), |_, _| Descend::Read);
        assert!(inv.is_complete());
        assert!(inv.node(NodeId(5)).is_protected());
        assert_eq!(inv.total_size(), 10);
        assert_eq!(inv.path(NodeId(3)), PathBuf::from("/r/a/y"));
    }

    #[test]
    fn listing_into_removed_dir_is_dropped() {
        use EntryKind::*;
        let mut inv = Inventory::new("/r");
        let p = ScanPolicy::default();
        let mut next = vec![];
        inv.ingest(NodeId::ROOT, &listing(&[("d", Dir, 0), ("f", File, 3)]), &p, &mut next);
        inv.remove(NodeId(1));
        inv.ingest(NodeId(1), &listing(&[("big", File, 999)]), &p, &mut next);
        assert_eq!(inv.total_size(), 3);
        assert!(inv.is_complete());
    }
}
