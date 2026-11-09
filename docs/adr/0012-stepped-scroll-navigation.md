# ADR-0012: Scroll and pinch step through the hierarchy

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Presentation
- **Amends:** [ADR-0010](0010-hierarchical-map-navigation.md)

## Context

On a map, people scroll or pinch to zoom without thinking. ADR-0010 rejected a
*free* scroll camera: nested cells would need re-solving at every zoom level,
and "where am I" would blur. Ignoring the wheel entirely leaves the most
natural gesture dead, though.

## Decision

Scroll and pinch drive the same one-level steps as click and Esc
(`state::Gesture`, `views::map::gesture`).

- **Direction:** scroll up or pinch out opens the scope under the pointer.
  Scroll down or pinch in goes up a level.
- **Progress:** input builds a signed value from -1 to 1, and a step fires at
  ±1. That takes 120 pt of scroll, a magnification of 0.35, or one mouse-wheel
  notch, which is treated as a whole step.
- **Feedback before the step:** the camera leans up to 14% toward the target
  scope, or the map shrinks by up to 5% when going up. If the gesture stops
  short, it springs back. Where there is nowhere to go (the root, or a file or
  small-items cell), it gives like a rubber band and never fires.
- **Hand-off:** a step starts the ADR-0010 transition already partway through
  (`Transition::pre`), so the lean flows into the zoom without a jump.
- **Latch:** once a step fires, input is ignored until 220 ms pass with no
  input, or until the direction reverses. One flick, momentum tail included,
  moves exactly one level.
- **Cursor:** the cursor is *zoom-in* over any cell that a click or scroll
  would open, so the affordance shows before the gesture.

## Consequences

Every input device can drill down and back up without a click, and every step
is still discrete, animated and reversible. Headless snapshots replay the path
with `--scroll X,Y,DY [--after MS]`.

## Alternatives considered

- *Free continuous zoom* (as on geographic maps): rejected again, for the
  reasons in ADR-0010.
- *Scroll pans the map*: there is nothing to pan, because the map always fits
  its scope.
