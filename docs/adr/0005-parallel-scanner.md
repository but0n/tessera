# ADR-0005: Parallel scanner — `getattrlistbulk`, reserved id ranges, single writer

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Inventory

## Context

Scanning is the critical path. A naïve walker does `readdir` + one `lstat`
per entry, single threaded. Our first parallel design had workers wait for the
ingest thread to assign child ids before descending, plus a batching window;
readers idled and `~/.cargo` took 270 ms (slower than `du`).

## Decision

1. **Fast reader on macOS**: `getattrlistbulk(2)` returns name, type, inode,
   link count, allocated size and mount status for many entries per syscall.
   The portable `readdir + lstat` reader is the reference; a test asserts both
   agree on names, kinds, sizes and inodes.
2. **Workers never wait.** After reading a directory, a worker reserves the id
   range for its children from an `AtomicU32`, sends the listing, then spawns
   reads for child directories (rayon scope, `2 × cores` threads, IO bound).
3. **Ordering by construction**: the channel is FIFO and a child's read starts
   after its parent's send completed, so a parent is always ingested before
   its children. `Inventory::ingest_at` fills reserved ranges; not-yet-ingested
   ids are unreachable placeholders.
4. **Single writer**: one ingest thread owns all writes, draining the channel
   in batches under one write lock. The UI and the cartographer read between
   batches.
5. Sizes are **allocated bytes** (what deletion frees); hard links are counted
   once (by inode); mount points are not crossed.

## Consequences

Measured on an M1, warm cache:

| tree | entries | Tessera | `du -sk` |
|------|--------:|--------:|---------:|
| `~/.cargo` | 34k | **36 ms** (940k entries/s) | 110 ms |
| a source tree (many git repos) | 809k | **3.6 s** (≈225k entries/s) | — |

A single directory is still read serially: `~/Library/Caches/…/Cache_Data`
(113k files) costs 5.5 s even for a bare `ls -f`, i.e. the kernel floor; the
rest of the tree completes in parallel meanwhile and the UI streams progress.

## Alternatives considered

- **jwalk / walkdir** — per-entry `lstat`, no bulk attributes.
- **fts(3)** — single threaded.
- **Spotlight / `mdfind` metadata** — not authoritative for sizes, misses
  excluded paths.
