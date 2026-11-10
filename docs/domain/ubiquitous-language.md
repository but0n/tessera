# Ubiquitous language

Use these words — in code, UI copy, issues and reviews — with exactly these
meanings. Code identifiers are given in `backticks`.

| Term | Meaning | Code |
|------|---------|------|
| **Scan** | One walk of a root directory, producing an inventory. Restarting is a *rescan*; ids do not survive it. | `ScanSession` |
| **Root** | The directory a scan starts from. Can never be marked. | `NodeId::ROOT` |
| **Inventory** | Everything a scan found: the tree of entries with totals. | `Inventory` |
| **Entry / node** | One file, directory, symlink or special file. | `Node`, `NodeId` |
| **Listing** | Raw contents of one directory as a reader saw them. | `Listing`, `RawEntry` |
| **Size** | Bytes *allocated on disk* for an entry's whole subtree — what removing it would free. Never the logical length. | `Node::size` |
| **Shared** | A hard link whose bytes were already counted at another link. | `Node::is_shared` |
| **Boundary** | A mount point the scan did not cross. | `Descend::Boundary` |
| **Protected** | A privacy-protected location deliberately not read. | `Descend::Protected` |
| **Unreadable** | A directory the OS refused to list. | `Node::is_failed` |
| **Category** | The kind of content a file holds (Video, Data, Code…), inferred from its extension. | `Category` |
| **Hint** | Advisory knowledge that a directory is *regenerable* (dependencies, build output, cache, trash). | `ReclaimHint` |
| **Insight** | A read model derived from the inventory: largest files, regenerable places, composition by category. | `Insights` |
| **Mark** | The user's intent to remove an entry. Planning only; touches nothing on disk. | `ReclaimPlan::toggle` |
| **Covered** | Not marked itself, but inside a marked directory. | `ReclaimPlan::covering` |
| **Reclaim plan** | The set of marks, with its invariants. | `ReclaimPlan` |
| **Commit** | Carrying out the plan: moving every marked entry to the Trash. | `Workspace::commit` |
| **Focus** | The directory the map is showing. | `ViewState::focus` |
| **Open (a cell)** | Make the cell's top-level folder the new focus: one level down, whatever nested cell was clicked. | `Action::Focus` |
| **Family** | The colour a subtree is drawn in, derived from the hierarchy (largest child keeps its parent's hue). | `ViewState::family` |
| **Selection** | The entry the inspector describes. | `ViewState::selected` |
| **Map** | The nested Voronoi treemap of the focus. | `MapLayout` |
| **Cell** | One convex region of the map; its area is proportional to its size. | `MapCell` |
| **Branch** | The top-level child of the focus a cell descends from; its family colours the whole subtree. | `MapCell::branch` |
| **Small items** | Siblings too small to draw individually, folded into one dotted cell. | `CellKind::Others` |
| **Step** | One level of map navigation, in or out, by click, scroll, pinch or Esc. A scroll *leans* toward the step before it fires. | `state::Gesture` |
| **Title / item** (map label) | A folder showing its contents is labelled as a *title* at the top of its region; anything else as an *item* at its cell's centre. The level is told by the style. | `views::map::place_labels` |
| **Arrival** | The entry a step was about (what was clicked, or the folder just left). It stays labelled and pulses once in the new view. | `ViewState::arrival` |
| **Sample data** | A fictional folder served through the real ports, for demos and screenshots. | `tessera_infra::SampleReader` |
