# ADR-0004: Visualise sizes as a nested power-diagram Voronoi treemap

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Cartography, Insight

## Context

Rectangular (squarified) treemaps produce slivers and a grid in which
hierarchy is hard to read. Voronoi treemaps give organic convex cells of any
aspect, make nesting obvious, and match the "cells" visual language we want.

## Decision

**Algorithm** (`tessera-voronoi`): Nocaj & Brandes, *Computing Voronoi
Treemaps* (EuroVis 2012), with the stabilisers popularised by
`d3-voronoi-map`:

1. Initial sites on a Vogel spiral, largest value at the centre.
2. Iterate: Lloyd step (sites → cell centroids, damped), multiplicative weight
   step `w ← w · target/area` (clamped), and *overweight handling* that keeps
   `|wᵢ - wⱼ| ≤ |sᵢ - sⱼ|²` so no cell can vanish.
3. Stop at `Σ|area − target| / total < ε` (1% top level, 2% nested) or an
   iteration cap; keep the best iterate.

**Power cells by half-plane clipping**, not a full power-diagram
construction: each cell is the container clipped by bisectors of its
neighbours, visited nearest first with an exact early exit (a bisector farther
than the cell's radius cannot cut it). For the ≤ 64 sites per level we draw,
this is simpler and faster, and every cell is convex by construction.

**Level of detail** (`tessera-app::cartography`): because area ∝ size, every
cell's *pixel area is known before solving*. Under a short read lock we
snapshot only cells ≥ 48 px²; smaller siblings fold into one "small items"
cell; directories ≥ 2200 px² recurse (max depth 4). Solving happens lock-free,
siblings in parallel (rayon), **warm-started** from the previous layout so live
scan updates morph instead of jumping.

**Service**: a `Cartographer` thread coalesces requests (only the newest is
solved) and publishes `Arc<MapLayout>`.

## Consequences

- Measured on an M1: ~300–450 cells for a 170 GB tree in 10–20 ms.
- Layout is not instantaneous for huge fan-out levels; the UI keeps showing the
  previous layout and animates the zoom while the new one solves.
- Area fidelity is approximate (≤ 1–2% per level); the inspector and tooltips
  show exact numbers.

## Alternatives considered

- **Squarified treemap** — rejected for the reasons above.
- **Sunburst** — angular area is hard to compare; deep levels become slivers.
- **CGAL-style power diagram via weighted Delaunay** — O(n log n) but far more
  code; no measurable gain at n ≤ 64.
