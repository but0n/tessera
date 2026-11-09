# ADR-0009: Seal privacy-protected locations instead of blocking on TCC

- **Status:** Accepted
- **Date:** 2026-11-09
- **Context map:** Inventory

## Context

On macOS, opening certain folders (`~/Library/Mail`, `Messages`, other apps'
`Containers`, …) without Full Disk Access makes `open(2)` **block until the
user answers a consent prompt**, or fail. During development a scan of
`~/Library` parked every reader thread in `open()` waiting on such prompts —
the scan never finished (`du` hangs the same way).

## Decision

- At startup the infrastructure probes Full Disk Access by opening the TCC
  database (readable only with FDA; fails fast with `EPERM`, never prompts).
- Without FDA, `ScanPolicy.protected` lists the known protected locations
  (`tessera-domain::protected`). The walker *seals* them (`Descend::Protected`)
  instead of reading: they appear in the tree as "protected", unmeasured.
- The overview shows a notice with a button that opens *System Settings →
  Privacy & Security → Full Disk Access*. With FDA granted, nothing is sealed.

## Consequences

Scans always terminate and never trigger surprise prompts. Without FDA, totals
exclude the sealed folders (some can be large, e.g. Docker's VM image lives in
`~/Library/Containers`); the UI says so explicitly rather than silently
under-reporting.
