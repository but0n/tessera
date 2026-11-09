//! Application façade: the use cases the UI can invoke, wired to ports.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tessera_domain::{MarkOutcome, NodeId, ReclaimPlan, ScanPolicy};

use crate::ports::{DirectoryReader, Trash, VolumeProbe, VolumeStats};
use crate::reclaim::{self, CommitPreview, CommitReport};
use crate::scan::{ScanSession, Waker};

#[derive(Clone)]
pub struct Services {
    pub reader: Arc<dyn DirectoryReader>,
    pub trash: Arc<dyn Trash>,
    pub volume: Arc<dyn VolumeProbe>,
}

#[derive(Clone, Debug)]
pub enum CommitState {
    Idle,
    Running { items: usize },
    Finished(CommitReport),
}

pub struct Workspace {
    services: Services,
    waker: Waker,
    policy: ScanPolicy,
    pub session: ScanSession,
    pub plan: ReclaimPlan,
    pub volume: Option<VolumeStats>,
    commit: Arc<Mutex<CommitState>>,
}

impl Workspace {
    pub fn open(root: PathBuf, services: Services, policy: ScanPolicy, waker: Waker) -> Workspace {
        let volume = services.volume.probe(&root);
        let session = ScanSession::start(root, services.reader.clone(), policy.clone(), waker.clone());
        Workspace { services, waker, policy, session, plan: ReclaimPlan::default(), volume, commit: Arc::new(Mutex::new(CommitState::Idle)) }
    }

    /// True when privacy-protected locations are being skipped.
    pub fn skips_protected(&self) -> bool {
        !self.policy.protected.is_empty()
    }

    pub fn root(&self) -> PathBuf {
        self.session.inventory().read().root_path().to_path_buf()
    }

    /// Starts a fresh scan (of `root`, or the current root). Marks are
    /// cleared because node identities do not survive a rescan.
    pub fn rescan(&mut self, root: Option<PathBuf>) {
        let root = root.unwrap_or_else(|| self.root());
        self.volume = self.services.volume.probe(&root);
        self.session = ScanSession::start(root, self.services.reader.clone(), self.policy.clone(), self.waker.clone());
        self.plan.clear();
    }

    pub fn toggle_mark(&mut self, id: NodeId) -> MarkOutcome {
        let inv = self.session.inventory().read();
        self.plan.toggle(&inv, id)
    }

    pub fn preview(&self) -> CommitPreview {
        reclaim::preview(&self.plan, &self.session.inventory().read())
    }

    pub fn commit_state(&self) -> CommitState {
        self.commit.lock().clone()
    }

    /// Moves every marked entry to the Trash in the background.
    pub fn commit(&mut self) {
        if self.plan.is_empty() || matches!(*self.commit.lock(), CommitState::Running { .. }) {
            return;
        }
        let preview = self.preview();
        *self.commit.lock() = CommitState::Running { items: preview.items.len() };
        let slot = self.commit.clone();
        reclaim::commit(preview, self.session.inventory().clone(), self.services.trash.clone(), self.waker.clone(), move |report| {
            *slot.lock() = CommitState::Finished(report);
        });
    }

    /// Call once per frame. Returns the report of a commit that just ended.
    pub fn poll(&mut self) -> Option<CommitReport> {
        let mut slot = self.commit.lock();
        if let CommitState::Finished(_) = &*slot {
            let CommitState::Finished(report) = std::mem::replace(&mut *slot, CommitState::Idle) else { unreachable!() };
            drop(slot);
            self.plan.prune(&self.session.inventory().read());
            self.session.refresh_insights();
            self.volume = self.services.volume.probe(&self.root());
            return Some(report);
        }
        None
    }
}
