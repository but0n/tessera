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
- **Labels are styled by level.** Room only decides *whether* a label shows
  and how many lines it gets, never its style. A region is labelled at the
  level of the folder it belongs to (`views::map::Level`, tokens
  `MAP_L1`–`MAP_L3`):

  | Level | Role | Name | Second line | Shown from (inradius) |
  |---|---|---|---|---|
  | 1 | the groups a click opens | 15 pt bold, white | size · share of scope, in the family colour | 18 pt (size from 30, share from 52) |
  | 2 | sub-groups | 12 pt semibold, 88% | size, dimmed | 18 pt (size from 40) |
  | 3 | detail | 10.5 pt regular, 66% | none | 22 pt |

  Folders showing their contents float the name on a soft halo (level 1
  strongest, level 3 none), never a pill. Folder names that would be cut to a
  stub are not drawn.
- **Files wear their type.** A file's label starts with its category icon
  (video, image, archive, data…), sized per level, while folders are plain
  names. The icon tells the two apart at a glance. A file name that does not
  fit is shortened in steps:
  1. full name;
  2. without the extension, once the icon shows the type
     (`A034_C006.mp4` → `A034_C006`);
  3. for names of 14 or more characters, cut in the middle so the end
     survives (`ubuntu-24.0…p-arm64`), keeping at least 10 characters;
  4. the icon alone.

  Cutting short codes leaves noise, so they skip step 3. Files too small for
  any name still show their icon, faint at level 3, so a folder of videos
  reads as videos. The tooltip carries the
  rest: icon, name, size, path, share bar, and what a click opens.

**Continuity through a step.** A name must not vanish or jump when the map
zooms.
- Folders are labelled at *every* depth, outermost first, so the name on
  screen is the folder a click opens next, and it keeps its label as the
  zoom brings it closer.
- **Chains.** When a folder's largest child fills at least 70% of it, the
  folder shows only as a thin rim, and to the eye the region *is* the child.
  The whole chain gets one label at the child: the main line names the cell
  actually seen (`target 45.1 GB`), and a small path above it names the
  folders around it (`bevy ›`). A top-level folder in that path keeps its
  family colour. This avoids both failure modes: a parent's name printed
  over its child, and a child's name hidden behind its parent's. If the
  region has no room for the path line, it is named by the outer folder
  alone, so an inner name never appears out of context.
- **Level 1 is never demoted.** A direct child of the scope is what a click
  opens, so its own name always stays the main label. When such a folder
  forms a chain, its label is the level-1 name, size and share, and a quiet
  line underneath names what fills it (`tessera` / `5.27 GB · 31%` /
  `target › release`). Level-1 labels are also placed first, before the
  followed entry's path, so no other label can crowd them out.
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
