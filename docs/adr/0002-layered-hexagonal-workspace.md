# ADR-0002: DDD layers as one crate each; dependencies point inward

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** all

## Context

We want the domain rules (sizes roll up correctly, marks never overlap, the
root is never deleted) to be testable without a filesystem or a GPU, and the
expensive pieces (scanner, renderer) to be swappable per platform.

## Decision

A Cargo workspace with one crate per layer. Cargo then *enforces* the
dependency rule — an inner crate literally cannot name an outer one.

```
tessera-ui (presentation + composition root, bin "tessera")
   │  └──────────────► tessera-infra (adapters: getattrlistbulk, Trash, statvfs, TCC)
   ▼                        │
tessera-app (use cases, ports, background services) ◄┘ implements ports
   │
   ├──► tessera-domain   (aggregates, value objects, domain services; zero deps)
   └──► tessera-voronoi  (pure geometry; zero deps)
```

Rules:

1. `tessera-domain` and `tessera-voronoi` have **no dependencies** and do no IO.
2. Ports (`DirectoryReader`, `Trash`, `VolumeProbe`) are traits in
   `tessera-app::ports`; `tessera-infra` implements them.
3. Only `tessera-ui/src/main.rs` (the composition root) knows every crate and
   chooses concrete adapters.
4. Views never mutate domain state. They emit `Action`s; the app shell applies
   them through `Workspace` use cases after the frame is built.
5. Aggregates are mutated through their own methods only (`Inventory::ingest_at`,
   `ReclaimPlan::toggle`, …), which own their invariants and tests.

## Consequences

- Domain tests run in milliseconds with fake listings; the scan pipeline is
  tested with an in-memory `FakeFs`; the macOS fast path is tested *against*
  the portable reader.
- Porting to Linux/Windows means adding readers/trash adapters in
  `tessera-infra`; nothing else changes.
- A little ceremony: adding a capability that touches IO means adding a port.

## Alternatives considered

- **Single crate with modules** — the dependency rule would be convention only.
- **Domain events / message bus** — unnecessary for a single-process app with
  one writer per aggregate; `generation` counters give cheap change detection.
