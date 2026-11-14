# Working on Tessera

Read `docs/adr/README.md` and `docs/domain/` before changing structure.

## Rules

- **Dependency direction** (ADR-0002): `domain`, `voronoi` ← `app` ← `infra`;
  `ui` is the composition root. Never add a dependency to `tessera-domain` or
  `tessera-voronoi`. IO goes behind a port in `tessera-app::ports`.
- **Aggregates own invariants.** Mutate `Inventory` / `ReclaimPlan` only via
  their methods; add a unit test next to any new invariant.
- **Views don't mutate.** They read `Ctx`, draw, and push `Action`s.
- **Design tokens only** (ADR-0008): colours, type and metrics come from
  `theme.rs`; reuse `views/widgets.rs`. Vermilion means "marked", nothing else.
- **Nothing is ever deleted permanently** (ADR-0007).
- **Never block on TCC** (ADR-0009): don't add code that opens protected paths.
- Use the terms in `docs/domain/ubiquitous-language.md`.
- Colours are defined in OKLCH (ADR-0011); map hues come from `ViewState::family`.
- Icons come from `gfx::icons` (ADR-0013). Add new ones as path strings on the
  24-unit grid, and prefer an icon or the layout over a new text label.
- **Two platforms** (ADR-0014): macOS and Linux. OS-specific code lives in
  `tessera-infra` behind a port, or in `tessera-ui::platform`; views carry
  no `cfg`. Shortcut text comes from `platform`, never a literal ⌘.
- **Phases are enums.** State that moves through stages (`state::Lean`,
  `Clock`, `Arrival`, `app::Shell`) is one enum with a variant per stage,
  never a set of flags and `Option`s that must agree. Put the transitions
  in a method and test them.
- Significant decisions get a new ADR (template `docs/adr/0000-template.md`).

## Checks

```sh
cargo fmt --all --check                            # rustfmt.toml: wide lines
cargo test --workspace --release
cargo clippy --workspace --release --all-targets   # must be warning-free
# Visual review without a window. Always use --sample for anything you
# share (screenshots, issues): it never shows real files.
cargo run --release -- --sample --snapshot /tmp/map.png --view map --demo
# Interaction: replay a click and capture mid-transition
cargo run --release -- --sample --snapshot /tmp/t.png --view map --click 900,500 --after 200
cargo run --release -- --sample --snapshot /tmp/s.png --view map --scroll 900,500,160 --after 200
# Icon contact sheet
TESSERA_ICON_SHEET=/tmp/icons.png cargo test --release -p tessera-ui icon_sheet -- --ignored
```
