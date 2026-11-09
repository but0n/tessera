# ADR-0013: Vector icons, and a map that labels itself by size

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Presentation
- **Amends:** [ADR-0008](0008-design-language.md), [ADR-0004](0004-voronoi-treemap.md) (rendering only)

## Context

The UI explained itself with words. There were section captions on every
panel, a four-item legend over the map, badges such as "protected", text
glyphs (▸ ✓ × ←) standing in for icons, and a label pill on every top-level
cell. Together they made the map busy, and they repeated what the layout
already shows.

## Decision

**Icons.** `gfx::icons` holds our own icon set (32 icons), each a few SVG path
strings on a 24-unit grid.
- They are stroked with round caps at a weight matched to the type: 1.9 units
  at 16 pt or smaller, 1.5 units at large sizes.
- tiny-skia rasterises each icon at its exact physical size into glyphon's
  atlas, through glyphon's custom-glyph API. Icons are therefore masks that
  take any token colour, are cached like glyphs, and add no font file or
  licence.
- `Ui::icon` draws one icon. Widgets (`button_icon`, `icon_button` with a
  hover `tip`, `kind_icon`) apply them consistently:
  - folders and files show a kind or content icon;
  - chevrons show disclosure;
  - lock, alert and drive icons replace the "protected", "unreadable" and
    "other volume" badges;
  - buttons in the top bar and inspector are icon-first.

**Less chrome.**
- The map legend is gone, because the cells show their own roles.
- The map header is a single breadcrumb with the scope's totals; it replaces
  both the old title block and the top-bar breadcrumb.
- The Explorer and Inspector captions are gone.
- Scan state is one line next to a check mark.

**The map draws more quietly.**
- At most three levels are shown: `MapParams::max_depth` drops from 4 to 3,
  and the thresholds for minimum cell size and recursion go up. The next
  level is one scroll away (ADR-0012).
- Cells are soft "pebbles": corner radius scales with depth and zoom, the
  grout narrows with depth, and the light comes from above with a top-edge
  bevel (`fs_cell`).
- *Small items* cells carry a fine dot texture and a muted folder tone, and
  inherit the mark state of their folder.
- **Labels are sized by room.** A cell's inradius picks the style:
  - 15 pt with its size;
  - 12.5 pt with its size;
  - 11 pt, name only;
  - nothing.
  Deeper cells drop one step. Stubs, and file names shortened to less than
  70% of their length, are not drawn. Folders that show their contents
  float their name on a soft halo instead of a pill. The tooltip carries the
  detail: icon, name, size, path, share bar, and what a click opens.

**Continuity through a step.** A name must not vanish or jump when the map
zooms.
- Folders are labelled at *every* depth, outermost first. A cell that covers
  at least half of its nearest labelled ancestor reads as the same region
  and is not named again. The name on screen is therefore the folder a click
  opens next, and it keeps its label as the zoom brings it closer.
- A click opens one level *and* selects the entry that was clicked. A scroll
  remembers it as the *arrival*. That entry and its ancestors are labelled
  before anything else, the selection ring marks it, and it pulses once
  (1.1 s) when the new scope lands.
- Going up pulses the folder just left. Both cases use
  `ViewState::arrival`.

## Consequences

Each word on screen now carries information the layout does not already show.
The icon pipeline costs one dependency (tiny-skia, already in the lockfile
through other crates) and one small rasteriser. New icons are path strings,
checked by the `every_icon_parses_and_inks` test. They can be reviewed as a
contact sheet:
`TESSERA_ICON_SHEET=/tmp/icons.png cargo test -p tessera-ui icon_sheet -- --ignored`.

## Alternatives considered

- *An icon font* (Lucide, Phosphor, SF Symbols): adds a binary asset, plus a
  licence to track (SF Symbols cannot be redistributed). Stroke weight would
  also be fixed rather than matched to the size.
- *SDF stroke primitives in the shape shader*: crisp, but overlapping
  translucent segments double-blend at joints, and curves are tedious to
  author.
