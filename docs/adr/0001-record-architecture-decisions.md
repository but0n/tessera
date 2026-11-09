# ADR-0001: Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-11-09

## Context

Tessera mixes several specialised concerns (filesystem syscalls, computational
geometry, GPU rendering, destructive file operations). The reasons behind
choices in each are not visible from the code and are easy to undo by accident.

## Decision

Use lightweight ADRs (Nygard format, `docs/adr/NNNN-title.md`, template in
`0000-template.md`). A change needs an ADR when it alters a crate boundary,
a dependency direction, a domain invariant, a performance strategy, anything
touching deletion, or the design language. Pull requests that do so link
their ADR.

## Consequences

Reviewers can check code against written decisions. ADRs are append-only;
reversing one means writing a superseding ADR, so history stays legible.
