//! Scan use case: walks a directory tree in parallel and feeds the
//! [`Inventory`] aggregate (ADR-0005, ADR-0006).
//!
//! Topology: N reader workers (rayon pool, IO bound) → one channel → one
//! *ingest* thread that owns all writes to the inventory.
//!
//! Workers never wait for the ingest thread: after reading a directory they
//! reserve an id range for its children from an atomic counter, send the
//! listing, then immediately spawn reads of the child directories. Because
//! the channel is FIFO and a child's read starts after its parent's send
//! completed, a parent's listing is always ingested before its children's.
//! The single writer keeps the aggregate's invariants trivial; the UI reads
//! through the shared `RwLock` between batches.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use tessera_domain::{Descend, EntryKind, Insights, Inventory, Listing, NodeId, ScanPolicy};

use crate::ports::DirectoryReader;

/// Called (from a background thread) whenever observable state changed.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

enum Job {
    Listed { dir: NodeId, start: NodeId, listing: Listing, fates: Vec<Descend> },
    Failed { dir: NodeId },
}

/// State shared by all reader workers of one scan.
struct Walker {
    reader: Arc<dyn DirectoryReader>,
    policy: ScanPolicy,
    next_id: AtomicU32,
    cancel: Arc<AtomicBool>,
    tx: Sender<Job>,
}

impl Walker {
    fn visit(self: &Arc<Self>, scope: &rayon::Scope<'_>, dir: NodeId, path: PathBuf) {
        if self.cancel.load(Ordering::Relaxed) {
            let _ = self.tx.send(Job::Failed { dir });
            return;
        }
        let mut listing = Listing::default();
        if self.reader.read(&path, &mut listing).is_err() {
            let _ = self.tx.send(Job::Failed { dir });
            return;
        }
        let start = NodeId(self.next_id.fetch_add(listing.len() as u32, Ordering::Relaxed));
        let mut fates = Vec::new();
        let mut children = Vec::new();
        for (i, (name, e)) in listing.iter().enumerate() {
            if e.kind != EntryKind::Dir {
                fates.push(Descend::Read);
                continue;
            }
            let child = path.join(name);
            let fate = self.policy.descend(e, &child);
            if fate == Descend::Read {
                children.push((NodeId(start.0 + i as u32), child));
            }
            fates.push(fate);
        }
        // Send before spawning: guarantees parent-before-child ingestion.
        let _ = self.tx.send(Job::Listed { dir, start, listing, fates });
        for (id, child) in children {
            let w = self.clone();
            scope.spawn(move |s| w.visit(s, id, child));
        }
    }
}

#[derive(Default)]
pub struct ScanStats {
    pub dirs: AtomicU64,
    pub entries: AtomicU64,
    started: Mutex<Option<Instant>>,
    finished: Mutex<Option<Instant>>,
}

impl ScanStats {
    pub fn elapsed(&self) -> Duration {
        let s = self.started.lock().unwrap_or_else(Instant::now);
        self.finished.lock().unwrap_or_else(Instant::now) - s
    }
    pub fn is_finished(&self) -> bool {
        self.finished.lock().is_some()
    }
}

/// A running or finished scan of one root.
pub struct ScanSession {
    inventory: Arc<RwLock<Inventory>>,
    insights: Arc<RwLock<Arc<Insights>>>,
    stats: Arc<ScanStats>,
    cancel: Arc<AtomicBool>,
    reader_name: &'static str,
}

const INSIGHT_K: usize = 200;

impl ScanSession {
    pub fn start(root: PathBuf, reader: Arc<dyn DirectoryReader>, policy: ScanPolicy, waker: Waker) -> ScanSession {
        let inventory = Arc::new(RwLock::new(Inventory::new(root.clone())));
        let insights = Arc::new(RwLock::new(Arc::new(Insights::default())));
        let stats = Arc::new(ScanStats::default());
        let cancel = Arc::new(AtomicBool::new(false));
        *stats.started.lock() = Some(Instant::now());
        let reader_name = reader.name();

        let threads = std::thread::available_parallelism().map_or(8, |n| n.get()).clamp(4, 16) * 2;
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|i| format!("tessera-read-{i}")).build().expect("reader pool");
        let (tx, rx) = crossbeam_channel::unbounded::<Job>();
        let walker = Arc::new(Walker { reader, policy, next_id: AtomicU32::new(1), cancel: cancel.clone(), tx });
        std::thread::Builder::new().name("tessera-walk".into()).spawn(move || pool.scope(|s| walker.visit(s, NodeId::ROOT, root))).expect("walk thread");

        let ctx = IngestCtx { inventory: inventory.clone(), insights: insights.clone(), stats: stats.clone(), cancel: cancel.clone(), waker, rx };
        std::thread::Builder::new().name("tessera-ingest".into()).spawn(move || ctx.run()).expect("ingest thread");

        ScanSession { inventory, insights, stats, cancel, reader_name }
    }

    pub fn inventory(&self) -> &Arc<RwLock<Inventory>> {
        &self.inventory
    }

    pub fn insights(&self) -> Arc<Insights> {
        self.insights.read().clone()
    }

    pub fn stats(&self) -> &ScanStats {
        &self.stats
    }

    pub fn reader_name(&self) -> &'static str {
        self.reader_name
    }

    pub fn is_finished(&self) -> bool {
        self.stats.is_finished()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Recomputes insights now (e.g. after entries were reclaimed).
    pub fn refresh_insights(&self) {
        let fresh = Insights::compute(&self.inventory.read(), INSIGHT_K);
        *self.insights.write() = Arc::new(fresh);
    }
}

impl Drop for ScanSession {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct IngestCtx {
    inventory: Arc<RwLock<Inventory>>,
    insights: Arc<RwLock<Arc<Insights>>>,
    stats: Arc<ScanStats>,
    cancel: Arc<AtomicBool>,
    waker: Waker,
    rx: Receiver<Job>,
}

impl IngestCtx {
    fn run(self) {
        const INSIGHT_EVERY: Duration = Duration::from_millis(600);
        const WAKE_EVERY: Duration = Duration::from_millis(33);
        let mut batch: Vec<Job> = Vec::with_capacity(4096);
        let mut last_insight = Instant::now();
        let mut last_wake = Instant::now();

        loop {
            match self.rx.recv_timeout(Duration::from_millis(100)) {
                Ok(j) => batch.push(j),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                // All workers finished; anything still pending was cancelled.
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
            batch.extend(self.rx.try_iter().take(4095));

            let complete = {
                let mut inv = self.inventory.write();
                for job in batch.drain(..) {
                    match job {
                        Job::Listed { dir, start, listing, fates } => {
                            inv.ingest_at(dir, start, &listing, |i, _| fates[i]);
                            self.stats.entries.fetch_add(listing.len() as u64, Ordering::Relaxed);
                        }
                        Job::Failed { dir } => inv.fail(dir),
                    }
                    self.stats.dirs.fetch_add(1, Ordering::Relaxed);
                }
                inv.is_complete()
            };

            if complete || last_insight.elapsed() >= INSIGHT_EVERY {
                let fresh = Insights::compute(&self.inventory.read(), INSIGHT_K);
                *self.insights.write() = Arc::new(fresh);
                last_insight = Instant::now();
            }
            if last_wake.elapsed() >= WAKE_EVERY {
                (self.waker)();
                last_wake = Instant::now();
            }
            if complete || self.cancel.load(Ordering::Relaxed) {
                break;
            }
        }
        *self.stats.finished.lock() = Some(Instant::now());
        (self.waker)();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;
    use tessera_domain::EntryKind;

    /// In-memory filesystem: path -> entries.
    struct FakeFs(HashMap<PathBuf, Vec<(&'static str, EntryKind, u64)>>);

    impl DirectoryReader for FakeFs {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn read(&self, path: &Path, out: &mut Listing) -> std::io::Result<()> {
            let entries = self.0.get(path).ok_or_else(|| std::io::Error::other("nope"))?;
            for (i, (n, k, s)) in entries.iter().enumerate() {
                out.push(n, *k, *s, i as u64, 1, false);
            }
            Ok(())
        }
    }

    #[test]
    fn scans_whole_tree_in_parallel() {
        use EntryKind::*;
        let mut fs = HashMap::new();
        fs.insert(PathBuf::from("/r"), vec![("a", Dir, 0), ("b", Dir, 0), ("f", File, 10)]);
        fs.insert(PathBuf::from("/r/a"), vec![("x", File, 5), ("deep", Dir, 0)]);
        fs.insert(PathBuf::from("/r/a/deep"), vec![("y", File, 7)]);
        // "/r/b" is unreadable.
        let s = ScanSession::start("/r".into(), Arc::new(FakeFs(fs)), ScanPolicy::default(), Arc::new(|| {}));
        let t = Instant::now();
        while !s.is_finished() {
            assert!(t.elapsed() < Duration::from_secs(5), "scan hung");
            std::thread::sleep(Duration::from_millis(2));
        }
        let inv = s.inventory().read();
        assert_eq!(inv.total_size(), 22);
        assert_eq!(inv.failures(), 1);
        assert!(inv.is_complete());
        assert_eq!(s.insights().largest_files.len(), 3);
    }
}
