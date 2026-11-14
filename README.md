# Tessera

[![CI](https://github.com/but0n/tessera/actions/workflows/ci.yml/badge.svg)](https://github.com/but0n/tessera/actions/workflows/ci.yml)

**See where your disk went, and take it back.**

Tessera draws a folder as a map. Every file and folder is a cell, sized
by how much space it takes, nested inside the folder that holds it. The
biggest space users stand out at a glance. Open them to look closer, mark
what you no longer need, and move it all to the Trash in one confirmed
step.

![The map](docs/img/map.png)

## Features

- **The whole picture at once.** Areas match sizes, nested folders sit
  inside their parents, and each top-level folder keeps its own colour all
  the way down. The labels show the structure too: a folder's name heads
  its region, and its contents are named inside it.
- **Explore by zooming.** Click, scroll or pinch to go one level deeper.
  The map flies into the folder you picked. Scroll back, press Esc or use
  the breadcrumb to come back out.
- **Fast.** Hundreds of thousands of files are counted in seconds, and the
  map fills in while the scan is still running.
- **Finds what is safe to clear.** Build outputs, dependency folders and
  caches (`target`, `node_modules`, `.venv`, …) are flagged as regenerable
  and collected in one list.
- **Nothing is deleted by accident.** Marking only builds a plan. You
  review it, and confirmed items go to the Trash, where you can still put
  them back. Tessera never empties the Trash and never marks anything for
  you.
- **Private.** Everything runs on your computer, with no network access.
  On macOS, privacy-protected folders such as Mail and Messages are listed
  but not opened, so there are no permission pop-ups. Grant Full Disk
  Access if you want them measured too.
- **Light.** It is a native, GPU-drawn app that uses no CPU while idle.

![The overview](docs/img/overview.png)

## Getting started

Tessera runs on macOS and Linux (Wayland or X11, with Vulkan or OpenGL).
Until prebuilt releases are available, build it with
[Rust](https://rustup.rs):

```sh
git clone https://github.com/but0n/tessera
cd tessera
cargo install --path crates/tessera-ui
```

Then:

```sh
tessera              # your home folder
tessera ~/Projects   # any folder
tessera --sample     # a made-up folder, to look around first
```

On Linux it uses the first of Inter, Adwaita Sans or Noto Sans that is
installed. *Reveal* and *Open folder* go through your desktop's file
manager and file chooser portal.

## Controls

On Linux, read ⌘ as Ctrl and ⌥ as Alt.

| | |
|---|---|
| Click, scroll up or pinch out on the map | Open that folder |
| Scroll down, pinch in, Esc | Up one level |
| ⌥-click | Select an item without opening it |
| Right-click, Space or M | Mark or unmark |
| ↑ ↓ ← →, Page Up/Down, Home/End | Move through the file tree |
| Enter, or double-click in the tree | Show a folder on the map |
| ⌘1 / ⌘2 | Overview / Map |
| ⌘⌫ | Review marked items and move them to the Trash |
| ⌘R / ⌘O | Rescan / open another folder |

## Good to know

- Sizes are space actually used on disk. A file with several hard links is
  counted once.
- A scan stays on one volume. Other disks mounted inside the folder are
  shown but not entered. On btrfs, a subvolume counts as part of its disk
  unless it is mounted separately. Snapshots are measured like `du` does,
  at full size, even though they share data with their origin.

## Development

How the code is organised, the conventions it follows and the checks to
run are in [AGENTS.md](AGENTS.md).

## License

MIT, see [LICENSE](LICENSE).
