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
- Cells are soft "pebbles". The corner radius is about 13 / 8.5 / 6 pt by
  depth, scaled with zoom and capped at 55% of the inradius. The radii are
  concentric: a child's radius is its parent's minus the padding, so no
  child corner pokes out of a rounded parent. Cells too thin to round are
  drawn square rather than overlapping. The grout narrows with depth, and
  the light comes from above with a top-edge bevel (`fs_cell`).
- *Small items* cells carry a fine dot texture and a muted folder tone, and
  inherit the mark state of their folder.
- **Labels show the tree: placement says *what*, style says *how deep*.**
  This follows the cartographic split between region names and place
  names.
  - A folder showing its contents is a **title**, set as high inside its
    region as it fits, on one line with its glyph and size
    (`▣ footage  175 GB · 52%`). A title at the top of an area reads as
    "this area is that folder".
  - Everything else is an **item**, centred in its cell, with its size
    underneath when there is room. It reads as "one thing in here".
  - Each nesting level has its own style (`views::map::Level`, tokens
    `MAP_L1`–`MAP_L3`), so the level of any label reads at a glance,
    whatever its cell's size. Room only decides whether it shows and how
    much of it:

    | Level | Name | Glyph | Size line | Shown from (inradius) |
    |---|---|---|---|---|
    | 1, the scope's children | 14.5 pt bold, white | solid folder in the family colour | size · share of the scope, family colour | 16 pt (size 26, share 44) |
    | 2 | 12 pt semibold, 86% | outline folder | size, dimmed | 16 pt (size 34) |
    | 3 | 10.5 pt regular, 62% | none (files: type icon) | none | 20 pt |

  - Hovering a cell lights the titles of every folder containing it and
    dims the other labels to 72%, which traces the cell's place in the
    tree.
  - Titles keep a soft dark halo (level 1 strongest, level 3 none); there
    are no pills. Folder names that would be cut to a stub are not drawn.
  - Because a parent's title sits at the top of its region and its child's
    label sits at the child's centre, a folder filled by one child shows
    both names without either covering the other (`bevy` over `target`).
    The earlier merged "chain" labels are gone.
- **Files wear their type.** A file's label starts with its category icon
  (video, image, archive, data…), sized per level, while folders carry the
  folder glyph of their level. A file name that does not fit is shortened
  in steps:
  1. full name;
  2. without the extension, once the icon shows the type
     (`A034_C006.mp4` → `A034_C006`);
  3. for names of 14 or more characters, cut in the middle so the end
     survives (`ubuntu-24.0…p-arm64`), keeping at least 10 characters;
  4. the icon alone.

  Short codes skip step 3. Files too small for any name still show their
  icon, faint at level 3, so a folder of videos reads as videos.

**Continuity through a step.** A name must not vanish or jump when the map
zooms.
- Folders are labelled at *every* depth, outermost first, so the name on
  screen is the folder a click opens next, and it keeps its label as the
  zoom brings it closer.
- **Every cell that can hold a label gets one.** A title searches
  downward from the top of its region; an item tries its centroid, then up
  to 24 nearby positions, nearest first. Each takes the first spot where
  the whole label lies inside the cell polygon and clear of other labels.
  If the full label does not fit, it retries with just the name. The
  scope's children are placed first. A level-1 name that
  still has no place may overhang a narrow cell rather than vanish. Nothing
  inside an unnamed level-1 folder is labelled, so a name never appears
  without its context.
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
