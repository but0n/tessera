//! Reclamation context: the user's intent to free space.

use std::collections::HashSet;

use crate::inventory::{Inventory, NodeId};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MarkOutcome {
    Marked {
        absorbed: usize,
    },
    Unmarked,
    /// An ancestor is already marked; marking again would double count.
    CoveredBy(NodeId),
    /// The scan root or a removed entry cannot be marked.
    Rejected,
}

/// Aggregate root of the Reclamation context.
///
/// Invariants:
/// * the scan root is never marked;
/// * no marked entry is an ancestor of another (marking a directory absorbs
///   previously marked descendants), so `total` never double counts.
#[derive(Clone, Debug, Default)]
pub struct ReclaimPlan {
    order: Vec<NodeId>,
    set: HashSet<NodeId>,
}

impl ReclaimPlan {
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Marked entries in the order the user marked them.
    pub fn items(&self) -> &[NodeId] {
        &self.order
    }

    pub fn is_marked(&self, id: NodeId) -> bool {
        self.set.contains(&id)
    }

    /// The marked entry (itself or an ancestor) that would remove `id`.
    pub fn covering(&self, inv: &Inventory, id: NodeId) -> Option<NodeId> {
        if self.set.is_empty() {
            return None;
        }
        inv.ancestors(id).find(|a| self.set.contains(a))
    }

    pub fn total(&self, inv: &Inventory) -> u64 {
        self.order.iter().map(|&id| inv.node(id).size()).sum()
    }

    pub fn toggle(&mut self, inv: &Inventory, id: NodeId) -> MarkOutcome {
        if self.set.contains(&id) {
            self.set.remove(&id);
            self.order.retain(|&x| x != id);
            return MarkOutcome::Unmarked;
        }
        self.mark(inv, id)
    }

    pub fn mark(&mut self, inv: &Inventory, id: NodeId) -> MarkOutcome {
        if id == NodeId::ROOT || inv.get(id).is_none_or(|n| n.is_removed()) {
            return MarkOutcome::Rejected;
        }
        if self.set.contains(&id) {
            return MarkOutcome::Marked { absorbed: 0 };
        }
        if let Some(a) = inv.ancestors(id).skip(1).find(|a| self.set.contains(a)) {
            return MarkOutcome::CoveredBy(a);
        }
        let before = self.order.len();
        self.order.retain(|&x| !inv.is_ancestor(id, x));
        let absorbed = before - self.order.len();
        if absorbed > 0 {
            self.set = self.order.iter().copied().collect();
        }
        self.order.push(id);
        self.set.insert(id);
        MarkOutcome::Marked { absorbed }
    }

    pub fn unmark(&mut self, id: NodeId) {
        if self.set.remove(&id) {
            self.order.retain(|&x| x != id);
        }
    }

    pub fn clear(&mut self) {
        self.order.clear();
        self.set.clear();
    }

    /// Drops entries that no longer exist in `inv` (after a commit or rescan).
    pub fn prune(&mut self, inv: &Inventory) {
        self.order.retain(|&id| inv.get(id).is_some_and(|n| !n.is_removed()));
        self.set = self.order.iter().copied().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::tests::sample;

    #[test]
    fn marking_parent_absorbs_children() {
        let inv = sample();
        let mut plan = ReclaimPlan::default();
        assert_eq!(plan.toggle(&inv, NodeId(6)), MarkOutcome::Marked { absorbed: 0 });
        assert_eq!(plan.toggle(&inv, NodeId(4)), MarkOutcome::Marked { absorbed: 0 });
        assert_eq!(plan.toggle(&inv, NodeId(2)), MarkOutcome::Marked { absorbed: 2 });
        assert_eq!(plan.items(), &[NodeId(2)]);
        assert_eq!(plan.total(&inv), 15);
        assert_eq!(plan.toggle(&inv, NodeId(5)), MarkOutcome::CoveredBy(NodeId(2)));
        assert_eq!(plan.covering(&inv, NodeId(6)), Some(NodeId(2)));
    }

    #[test]
    fn root_cannot_be_marked() {
        let inv = sample();
        let mut plan = ReclaimPlan::default();
        assert_eq!(plan.toggle(&inv, NodeId::ROOT), MarkOutcome::Rejected);
        assert!(plan.is_empty());
    }

    #[test]
    fn toggle_unmarks_and_prune_drops_removed() {
        let mut inv = sample();
        let mut plan = ReclaimPlan::default();
        plan.toggle(&inv, NodeId(1));
        plan.toggle(&inv, NodeId(3));
        assert_eq!(plan.toggle(&inv, NodeId(1)), MarkOutcome::Unmarked);
        inv.remove(NodeId(3));
        plan.prune(&inv);
        assert!(plan.is_empty());
    }
}
