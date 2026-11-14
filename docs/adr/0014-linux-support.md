# ADR-0014: Linux support — `statx` reader, mount roots as boundaries, platform seams

- **Status:** Accepted
- **Date:** 2026-11-14
- **Context map:** Inventory, Presentation

## Context

Tessera was written on macOS. The stack (winit, wgpu, glyphon, `trash`,
`rfd`) is portable and the workspace already compiled on Linux, but the
result was not usable there:

- Text was set in "Helvetica Neue", which does not exist; every weight above
  regular fell back to an unrelated serif face and figures lost their spacing.
- Shortcuts were bound to Super, which on Linux belongs to the window
  manager (on Hyprland every Super chord is taken).
- *Reveal* ran `open -R`; the top bar reserved room for traffic lights.
- The portable reader decides "mount point" by comparing `st_dev`. On btrfs
  every subvolume has its own device number, so a scan stopped at each
  nested subvolume as if it were another disk. Inode numbers are likewise
  only unique per subvolume, while hard links are de-duplicated by inode
  across the whole scan (ADR-0005).

Measured on the development machine (24 threads, btrfs on NVMe, warm cache),
home folder of 1.29 M entries: `readdir + lstat` 238 ms, `readdir + statx`
238 ms. Both are at the floor; there is no speed to win from the syscall.

## Decision

1. **Linux is a supported platform**, checked by CI next to macOS.
2. **Reader**: `StatxReader` (`readdir` + `statx` relative to the directory
   fd) is `best_reader()` on Linux. It is chosen for correctness, not speed:
   - a **boundary is a mount root** (`STATX_ATTR_MOUNT_ROOT`), falling back
     to the device comparison where the kernel does not report it. Nested
     btrfs subvolumes are entered; `/proc`, other disks and separately
     mounted subvolumes are not.
   - the hard-link identity folds the device into the inode number.
   - `AT_NO_AUTOMOUNT | AT_STATX_DONT_SYNC`: a scan never triggers an
     automount nor waits on a network filesystem, in the spirit of ADR-0009.
   `PortableReader` stays the reference and the reader for other Unixes.
3. **Nothing is sealed on Linux.** There is no consent prompt to block on,
   so `default_policy()` protects nothing; unreadable directories are simply
   *unreadable*.
4. **Desktop port.** Revealing an entry and opening privacy settings go
   through `ports::Desktop`. On Linux, reveal calls the freedesktop
   `FileManager1.ShowItems` D-Bus method through `dbus-send` and falls back
   to `xdg-open` on the parent folder. Sample data gets a no-op.
5. **Presentation seams live in `tessera-ui::platform`**: the command
   modifier (⌘ on macOS, Ctrl elsewhere), how shortcuts are written, and
   whether the top bar is the titlebar. Views contain no `cfg`.
6. **Fonts.** Linux has no single system UI font, and the user's fontconfig
   default is often a two-weight metric clone. The type scale needs weights
   from 400 to 650 and tabular figures, so the UI face is the first installed
   of a preference list headed by Inter, Adwaita Sans and Noto Sans.
7. The window carries the app id `tessera` and normal server- or client-side
   decorations; it does not draw its own titlebar.

## Consequences

- One more reader to keep honest: `statx::tests` assert agreement with the
  portable reader, one identity per hard-link set, and that `/proc` is a
  boundary while `/etc` is not.
- Snapshots inside the scanned tree are counted at full size although they
  share extents, as `du` does. Shared-extent accounting is out of scope.
- No new crates. `dbus-send` and `xdg-open` are expected on a desktop
  system; without them *Reveal* does nothing.
- The font list is a design choice that overrides the desktop default; a
  machine with none of the listed faces gets glyph-by-glyph fallback.
- Pinch is not delivered by winit on Linux; scroll and click cover the same
  steps (ADR-0012).

## Alternatives considered

- **Keep `PortableReader` on Linux and compare `st_dev`** — wrong on btrfs,
  the default filesystem of several distributions.
- **Parse `/proc/self/mountinfo` for boundaries** — a second source of truth
  to keep in sync with a walk that already stats every entry.
- **`getdents64` + `io_uring` batched `statx`** — the scan is already at the
  page-cache floor; complexity without a measured gain.
- **Ask fontconfig for `sans-serif`** — respects the user's setting but
  yields faces without the weights and features the type scale relies on.
- **A D-Bus client crate for reveal** — a dependency tree for one method
  call; `dbus-send` ships with D-Bus itself.
