# Context map

```
            ┌───────────────────────────── Presentation (tessera-ui) ────────────────────────────┐
            │  views emit Actions ─► Workspace use cases          ViewState (focus, selection…) │
            └───────────────┬───────────────────────────┬─────────────────────────┬──────────────┘
                            │ reads                     │ requests                │ commands
                            ▼                           ▼                         ▼
┌──────────────── Inventory ───────────┐  ┌──────── Cartography ────────┐  ┌──── Reclamation ─────┐
│ Inventory aggregate (arena tree)     │  │ Cartographer service         │  │ ReclaimPlan aggregate│
│ ScanPolicy, Descend, Listing         │──►  snapshot → solve (Voronoi)  │  │ commit via Trash port│
│ fed by ScanSession (app)             │  │ MapLayout read model         │  │                      │
│ ◄── DirectoryReader port (infra)     │  └──────────────────────────────┘  └──────────┬───────────┘
└──────────────┬───────────────────────┘        uses tessera-voronoi                   │ removes
               │ derives                                                               ▼
               ▼                                                               Inventory.remove()
┌──────────────── Insight ─────────────┐
│ Insights read model, Category,       │
│ ReclaimHint (advises Reclamation)    │
└──────────────────────────────────────┘
```

| Context | Owns | Upstream of | Relationship |
|---------|------|-------------|--------------|
| **Inventory** | `Inventory`, `Node`, `Listing`, `ScanPolicy`, protected locations | everything | Core. Single writer (ingest thread). |
| **Insight** | `Insights`, `Category`, `ReclaimHint` | Presentation, Reclamation | Conformist read models over Inventory; recomputed, never stored. |
| **Reclamation** | `ReclaimPlan`, `CommitReport` | Inventory (removal) | Customer of Inventory; only path that mutates it besides scanning. |
| **Cartography** | `MapLayout`, `MapCell`, `MapParams` | Presentation | Read model over Inventory; built on the generic `tessera-voronoi` kernel. |
| **Presentation** | `ViewState`, `Action`, theme | — | Anti-corruption layer: domain ids in, actions out. |

Shared kernel: `NodeId` and `ByteSize` are used by all contexts.
