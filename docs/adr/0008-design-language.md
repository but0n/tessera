# ADR-0008: Design language — tokens, and the layout is the legend

- **Status:** Accepted — palette amended by [ADR-0011](0011-perceptual-colour-system.md)
- **Date:** 2026-11-09
- **Context map:** Presentation

## Context

A disk tool is read in a glance and used under mild stress ("my disk is
full"). It must explain itself without a manual, and look coherent everywhere.

## Decision

**The layout explains itself.** Fixed regions, each answering one question:

| region | question |
|--------|----------|
| top bar | Where am I (breadcrumb), what view, is the scan done? |
| Explorer (left) | What exists, largest first? |
| Overview / Map (centre) | How big are things relative to each other? |
| Inspector (right, top) | What exactly is the selection? Is it safe to remove? |
| Reclaim (right, bottom) | What have I decided to free, and how much? |
| footer | What can I do right now (contextual key hints)? Where do numbers come from? |

The map header carries its own legend (folder / file / small items / marked)
and states the encoding: *area ∝ size on disk*.

**One vocabulary across views.** Branch colours in the map are reused for the
tree icons, share bars and the inspector; category colours are shared between
the composition chart and file dots. Vermilion + 45° hatching = "marked for
removal", everywhere (map cells, capacity bar, queue, buttons), and nothing
else uses it.

**Tokens only** (`tessera-ui/src/theme.rs`): surfaces (`BG → PANEL → RAISED →
HOVER`), three ink levels, semantic colours, a 10-hue muted branch palette
tuned for equal visual weight on dark, a type scale on the system font (SF Pro,
variable weight) with **tabular figures** for every number, and metrics
(gutters, radii, row height). Views must not hard-code colours or sizes.

**Rendering rules**: non-sRGB surface so alpha blending matches design tools;
units are set smaller and dimmer than their numbers; cells get a soft
"membrane" shading (darker rim, lighter core) rather than flat fills; motion is
short (≤ 320 ms), eased, and only used to preserve context (zoom into a cell,
fade in a new level).

## Consequences

Changing the look is a `theme.rs` edit. New views must reuse widgets in
`views/widgets.rs` and these tokens; reviewers reject ad-hoc colours.
