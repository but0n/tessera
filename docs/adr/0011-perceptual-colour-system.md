# ADR-0011: Perceptual (OKLCH) colours and hierarchy-derived hues

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Presentation
- **Amends:** the palette paragraph of ADR-0008

## Context

The first palette mixed ten hand-picked sRGB hues into the background by a
fixed fraction. In sRGB equal mixing does not give equal lightness: amber and
olive cells looked muddy, blue and violet sank into the background, and the
map read as uneven. Colours were also assigned per layout: entering a folder
repainted everything, and the tree lost its tints whenever the map focus
changed.

## Decision

- **Every colour is specified in OKLCH**, which is perceptually uniform:
  - Neutrals share one cool hue (265°) at chroma ≤ 0.014, and surfaces step
    evenly in lightness (`BG 0.155 → HOVER 0.245`).
  - Each semantic colour has exactly one meaning: vermilion = marked, accent
    blue = interaction / scope / progress, green = free / done.
- **Eight branch hues at equal L 0.74, C 0.085**, avoiding the vermilion band
  (10°–60°), so no family can be mistaken for "marked". Constants are baked to
  hex, and a unit test keeps them equal to the OKLCH definition.
- **Map cells use tones, not mixes:** `theme::tone(hue, Tone, rank)` sets
  lightness and chroma per role (frame by depth, folder, file, small items).
  A role therefore reads the same in every family.
- **Hues come from the hierarchy:** `hue(root) = 0`,
  `hue(child) = hue(parent) + rank`. The largest child keeps its parent's hue,
  so zooming never repaints the dominant region. The tree, map, inspector and
  tooltips all call `ViewState::family`, so they always agree.
- The vermilion fill is kept for the one irreversible call to action ("Move to
  Trash…"). Adding to the plan uses a vermilion-tinted button
  (`ButtonKind::Mark`).

## Consequences

The map has an even visual weight, and colour carries hierarchy across scope
changes. New colours must be defined in OKLCH and baked like the others.

## Alternatives considered

- *Per-layout branch indices* (the previous scheme): every level gets fresh
  colours, which breaks continuity when zooming.
- *Colour by category (file type) in the map*: this answers "what kind of
  data", which the Overview's composition chart already covers. The map's job
  is "where".
