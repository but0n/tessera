# Architecture Decision Records

Every decision that shapes the code beyond a single module is recorded here
(see [ADR-0001](0001-record-architecture-decisions.md)). Copy
[`0000-template.md`](0000-template.md), take the next number, and link it below.
ADRs are immutable once *Accepted*; change course with a new ADR that
*supersedes* the old one.

| #    | Decision | Status |
|------|----------|--------|
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-layered-hexagonal-workspace.md) | DDD layers as one crate each; dependencies point inward | Accepted |
| [0003](0003-gpu-ui-winit-wgpu.md) | Own GPU UI on winit + wgpu + glyphon (not gpui / egui / iced) | Accepted |
| [0004](0004-voronoi-treemap.md) | Visualise sizes as a nested power-diagram Voronoi treemap | Accepted (rendering amended by 0013) |
| [0005](0005-parallel-scanner.md) | Parallel scanner: `getattrlistbulk`, reserved id ranges, single writer | Accepted (Linux reader added by 0014) |
| [0006](0006-inventory-arena.md) | Inventory as an arena tree with incremental roll-ups | Accepted |
| [0007](0007-safe-reclamation.md) | Reclamation is a plan; it only ever moves to the Trash | Accepted |
| [0008](0008-design-language.md) | Design language: tokens, the layout is the legend | Accepted (palette amended by 0011, icons by 0013) |
| [0009](0009-privacy-protected-locations.md) | Seal privacy-protected locations instead of blocking on TCC | Accepted |
| [0010](0010-hierarchical-map-navigation.md) | Map navigation: one level per click, spatial zoom transitions | Accepted (amended by 0012) |
| [0011](0011-perceptual-colour-system.md) | OKLCH colours; hues derived from the hierarchy | Accepted |
| [0012](0012-stepped-scroll-navigation.md) | Scroll and pinch step through the hierarchy, with a lean preview and latch | Accepted |
| [0013](0013-icons-and-quiet-map.md) | Own vector icons; a three-level map labelled by available room | Accepted |
| [0014](0014-linux-support.md) | Linux support: `statx` reader, mount roots as boundaries, platform seams | Accepted |
