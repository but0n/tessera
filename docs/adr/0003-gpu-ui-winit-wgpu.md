# ADR-0003: Own GPU UI on winit + wgpu + glyphon

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Presentation

## Context

The centrepiece is thousands of anti-aliased, rounded, convex polygons with
hatching, hover and selection states, animated zoom and live updates while a
scan streams in — plus conventional chrome (tree, lists, buttons). Candidates:
gpui (Zed), egui, iced, a webview, or our own renderer on wgpu.

## Decision

- **winit 0.30** for windowing/input, **wgpu 30** for rendering, **glyphon
  0.12** (cosmic-text + swash) for shaped text with font fallback (CJK file
  names work out of the box).
- Two instanced SDF pipelines (`gfx/shapes.wgsl`):
  - *quads* — rounded rects with border, soft shadow, gradients, stripes;
  - *cells* — exact signed distance to a convex polygon whose vertices live in
    a storage buffer. Corner rounding is exact: the CPU insets the polygon by
    `gap + r`, the shader draws `sdf - r`.
- No MSAA: SDF coverage gives ~1 physical-pixel AA at any zoom.
- A tiny immediate-mode layer (`ui.rs`): views are functions of state that
  emit primitives and `Action`s. Three draw layers (base, labels, overlays),
  each `quads → cells → text`.
- Redraw on demand (`ControlFlow::Wait`); background services wake the loop
  through an `EventLoopProxy`. Idle CPU is 0%.
- A headless `--snapshot` mode renders the identical frame to PNG for visual
  review and docs.

## Consequences

- Full control over the look; cells cost one instance + ~8 vertices each.
- We own widgets (buttons, lists, scrolling). Accessibility (VoiceOver) and
  IME text input are not provided; acceptable for a visual analysis tool, to be
  revisited if text entry (search) is added.

## Alternatives considered

- **gpui** — excellent, but its API tracks Zed's internal needs, it is not a
  stable crates.io dependency, and custom polygon rendering would still need
  its own shader path.
- **egui** — tessellates shapes on the CPU; thousands of rounded polygons per
  frame and the bespoke look would fight the framework.
- **iced** — viable, but custom primitives require the same wgpu work plus an
  extra abstraction layer.
- **Webview (Tauri)** — two runtimes, and SVG/Canvas with thousands of cells
  is markedly slower than one instanced draw call.
