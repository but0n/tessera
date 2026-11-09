# ADR-0010: Navigate the map one level per click, with spatial transitions

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Presentation, Cartography

## Context

The first map opened a folder on *double-click* of the cell under the pointer.
The deepest cell under the pointer is almost always a file or a nested leaf
folder, so double-clicks mostly did nothing, and single clicks selected a tiny
cell with no visible effect. Users reported that "clicking does nothing".
Nested treemaps (FoamTree, DaisyDisk, Disk Inventory X) teach that drilling
down must be one obvious gesture, and that the zoom must keep the viewer
oriented.

## Decision

- **A click opens the cell's scope:** its depth-1 ancestor relative to the map
  focus. The map always descends exactly one level, whatever nested cell was
  hit. Hover lifts the whole scope (cell flag `SCOPE`) and the tooltip says
  "Click to open *name*", so the target is visible before the click.
- ⌥-click selects the exact entry; right-click marks it. A click on a
  top-level *file* selects it. Press and release must land in the same scope.
- **Going up**: Esc, Backspace, the mouse back button, the ← button in the map
  header, or any breadcrumb segment (`Action::FocusUp` / `Action::Focus`).
- **Transitions** (`state::Transition`, 460 ms, ease-in-out):
  - *In*: the camera dives into the pivot cell of the old layout while the
    other cells fade. The new layout grows out of the pivot's bounding box to
    fill the map.
  - *Out*: the mirror image. The parent layout's camera pulls back from the
    cell of the folder just left, and the old layout shrinks into it. The
    clock starts when the parent layout is ready (it waits at most 450 ms,
    then falls back to a cross-fade).
  - *Fade*: unrelated scopes, e.g. a far jump from the tree, cross-fade.
  Labels appear only on cells that are almost fully opaque, so the two layers
  never print over each other.
- The Explorer mirrors the map: the scope's subtree is a tinted band, the scope
  row carries a marker, rows outside it recede, and the row under the map
  pointer is highlighted.

## Consequences

Drilling down is a single, predictable click everywhere on the map. Warm
starts (ADR-0004) keep the new layout's arrangement close to the nested one
the user just saw, which makes the zoom read as continuous. Headless
snapshots can replay the real input path: `--click X,Y --after MS` captures a
frame in the middle of a transition.

## Alternatives considered

- *Double-click the deepest folder*: jumps several levels at once and loses the
  user. A single click on a file did nothing.
- *Scroll-to-zoom with a free camera*: it suits geographic maps, but nested
  cells then need re-solving at every zoom level, and the meaning of "where am
  I" gets blurry.
