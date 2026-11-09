# ADR-0006: Inventory as an arena tree with incremental roll-ups

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Inventory

## Context

Home directories routinely hold millions of entries. The UI must show correct
directory totals *while* scanning, and after items are trashed.

## Decision

- `Inventory` stores nodes in one `Vec<Node>` (40 bytes each) and all names in
  one `String` arena (offset + length per node). No per-entry heap allocation.
- A directory's children occupy a **contiguous id range**; a node stores only
  `parent`, `child_start`, `child_count`. Parents always have smaller ids.
- Totals (`size`, `files`, `dirs`, per-category bytes) are maintained
  **incrementally**: each ingest adds its sum along the ancestor chain
  (O(depth)); removal subtracts likewise. There is never an O(n) recompute.
- Removal **tombstones** the subtree (`REMOVED` flag); ids are never reused,
  so references held by the UI or the reclaim plan stay safe.
- `generation` increments on every mutation for cheap change detection.

## Consequences

- 809k entries ≈ 32 MB of nodes plus names.
- Paths are reconstructed on demand by walking parents (only for display or
  for the Trash call) — cheap at the rates we need.
- Invariants are unit-tested in `inventory.rs` (roll-ups, removal, hard links,
  out-of-order reserved ranges, listings arriving for removed directories).
