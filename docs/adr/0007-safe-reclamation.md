# ADR-0007: Reclamation is a plan; it only ever moves to the Trash

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Reclamation

## Context

The tool exists to delete things; a mistake is unrecoverable data loss.

## Decision

1. **Marking is planning.** `ReclaimPlan` is its own aggregate. Marking never
   touches the disk. Invariants: the scan root cannot be marked; no marked
   entry is an ancestor of another (marking a folder absorbs marks inside it,
   marking inside a marked folder is refused with "already included"), so the
   reclaimable total never double counts.
2. **Hints, never auto-marks.** `ReclaimHint` recognises regenerable content
   (`node_modules`, `target` *next to a* `Cargo.toml`, `DerivedData`, caches,
   trash) and explains why it is safe. The user still decides.
3. **Explicit confirmation** listing what will move and how much it frees.
4. **Trash only.** The `Trash` port has no "delete permanently". The macOS
   adapter uses `NSFileManager trashItemAtURL` (no Finder automation prompt;
   per-item failures). Each success is applied to the inventory immediately;
   failures are reported and leave the item in place.
5. Visual language: vermilion and hatching mean "will be removed" and are used
   for nothing else (ADR-0008).

## Consequences

Disk space is only actually freed once the user empties the Trash; the
confirmation dialog says so. Permanent deletion would need a new ADR.
