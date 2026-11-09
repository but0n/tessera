//! Commit use case: carries out a [`ReclaimPlan`] through the [`Trash`] port.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use tessera_domain::{Inventory, NodeId, ReclaimPlan};

use crate::ports::Trash;
use crate::scan::Waker;

#[derive(Clone, Debug, Default)]
pub struct CommitReport {
    pub trashed: Vec<NodeId>,
    pub freed: u64,
    pub failed: Vec<(PathBuf, String)>,
}

/// Snapshot of what a commit is about to do, shown in the confirmation.
#[derive(Clone, Debug)]
pub struct CommitPreview {
    pub items: Vec<(NodeId, PathBuf, u64)>,
    pub total: u64,
}

pub fn preview(plan: &ReclaimPlan, inv: &Inventory) -> CommitPreview {
    let items: Vec<_> = plan.items().iter().map(|&id| (id, inv.path(id), inv.node(id).size())).collect();
    let total = items.iter().map(|i| i.2).sum();
    CommitPreview { items, total }
}

/// Trashes every item of `preview` on a background thread. Each success is
/// applied to the inventory immediately, so the UI shrinks as items go.
pub fn commit(preview: CommitPreview, inventory: Arc<RwLock<Inventory>>, trash: Arc<dyn Trash>, waker: Waker, done: impl FnOnce(CommitReport) + Send + 'static) {
    std::thread::Builder::new()
        .name("tessera-commit".into())
        .spawn(move || {
            let mut report = CommitReport::default();
            for (id, path, _) in preview.items {
                match trash.trash(&path) {
                    Ok(()) => {
                        report.freed += inventory.write().remove(id);
                        report.trashed.push(id);
                    }
                    Err(e) => report.failed.push((path, e)),
                }
                waker();
            }
            done(report);
            waker();
        })
        .expect("commit thread");
}
