//! Tessera domain model.
//!
//! Pure, IO-free core of the application. Three bounded contexts live here
//! (see `docs/domain/context-map.md`):
//!
//! * **Inventory** – the [`Inventory`] aggregate: an arena tree of scanned
//!   filesystem entries with incrementally maintained subtree totals.
//! * **Insight** – read models derived from the inventory ([`insight`]).
//! * **Reclamation** – the [`ReclaimPlan`] aggregate: what the user intends to
//!   remove, with its invariants (no overlapping marks, root never marked).

pub mod category;
pub mod hint;
pub mod insight;
pub mod inventory;
pub mod protected;
pub mod reclaim;
pub mod size;

pub use category::Category;
pub use hint::ReclaimHint;
pub use insight::Insights;
pub use inventory::{Descend, EntryKind, Inventory, Listing, Node, NodeId, RawEntry, ScanPolicy};
pub use reclaim::{MarkOutcome, ReclaimPlan};
pub use size::ByteSize;
