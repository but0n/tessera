//! Insight context: read models computed from an [`Inventory`] snapshot.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::category::Category;
use crate::inventory::{EntryKind, Inventory, NodeId};

#[derive(Clone, Debug, Default)]
pub struct CategoryShare {
    pub category: Option<Category>,
    pub bytes: u64,
    pub files: u32,
}

/// Dashboard read model. Recomputed wholesale (O(n)) at a throttled rate;
/// cheap enough for millions of nodes and simpler than incremental heaps.
#[derive(Clone, Debug, Default)]
pub struct Insights {
    pub generation: u64,
    /// Largest individual files, descending.
    pub largest_files: Vec<NodeId>,
    /// Outermost directories carrying a [`crate::ReclaimHint`], descending.
    pub reclaimable: Vec<NodeId>,
    pub reclaimable_bytes: u64,
    /// Byte totals per file category, descending, empty ones omitted.
    pub categories: Vec<CategoryShare>,
}

impl Insights {
    /// `k` bounds the largest-files list; every reclaimable place is kept.
    pub fn compute(inv: &Inventory, k: usize) -> Insights {
        let mut heap: BinaryHeap<Reverse<(u64, NodeId)>> = BinaryHeap::with_capacity(k + 1);
        let mut hinted: Vec<NodeId> = Vec::new();
        for (id, n) in inv.nodes() {
            if n.is_removed() {
                continue;
            }
            match n.kind() {
                EntryKind::File => {
                    if heap.len() < k {
                        heap.push(Reverse((n.size(), id)));
                    } else if heap.peek().is_some_and(|Reverse((s, _))| n.size() > *s) {
                        heap.pop();
                        heap.push(Reverse((n.size(), id)));
                    }
                }
                EntryKind::Dir if n.hint().is_some() => hinted.push(id),
                _ => {}
            }
        }
        let mut largest_files: Vec<(u64, NodeId)> = heap.into_iter().map(|Reverse(x)| x).collect();
        largest_files.sort_unstable_by(|a, b| b.cmp(a));

        // Keep only outermost hinted dirs so bytes are never double counted.
        let mut reclaimable: Vec<NodeId> = hinted.iter().copied().filter(|&id| !inv.ancestors(id).skip(1).any(|a| inv.node(a).hint().is_some())).collect();
        let reclaimable_bytes = reclaimable.iter().map(|&id| inv.node(id).size()).sum();
        // All of them: only outermost folders count, so the list stays short,
        // and its length is the "N places" the total is spread over.
        reclaimable.sort_unstable_by_key(|&id| Reverse(inv.node(id).size()));

        let mut categories: Vec<CategoryShare> =
            Category::FILE_KINDS.iter().map(|&c| CategoryShare { category: Some(c), bytes: inv.category_bytes(c), files: inv.category_files(c) }).filter(|s| s.bytes > 0).collect();
        categories.sort_unstable_by_key(|s| Reverse(s.bytes));

        Insights { generation: inv.generation(), largest_files: largest_files.into_iter().map(|(_, id)| id).collect(), reclaimable, reclaimable_bytes, categories }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::tests::sample;

    #[test]
    fn finds_largest_and_reclaimable() {
        let inv = sample();
        let ins = Insights::compute(&inv, 2);
        let names: Vec<&str> = ins.largest_files.iter().map(|i| inv.name(*i)).collect();
        assert_eq!(names, ["a.mp4", "main.rs"]);
        assert_eq!(ins.reclaimable.len(), 1);
        assert_eq!(inv.name(ins.reclaimable[0]), "node_modules");
        assert_eq!(ins.reclaimable_bytes, 5);
        assert_eq!(ins.categories[0].category, Some(Category::Video));
    }
}
