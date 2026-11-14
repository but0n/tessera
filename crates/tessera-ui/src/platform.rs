//! What differs between desktops in the presentation layer: which modifier
//! is "the command key" and how shortcuts are written (ADR-0014).

use winit::keyboard::ModifiersState;

const MAC: bool = cfg!(target_os = "macos");

/// The modifier of app shortcuts: ⌘ on macOS, Ctrl elsewhere (where the
/// Super key belongs to the window manager).
pub fn command(mods: ModifiersState) -> bool {
    if MAC { mods.super_key() } else { mods.control_key() }
}

/// True where the top bar doubles as the window's titlebar: its empty space
/// drags the window and its left end is kept clear for the window controls.
pub const INTEGRATED_TITLEBAR: bool = MAC;

// Shortcuts as written in tooltips and keycaps.
pub const OPEN_FOLDER_TIP: &str = if MAC { "Open folder…  ⌘O" } else { "Open folder…  Ctrl+O" };
pub const RESCAN_TIP: &str = if MAC { "Rescan  ⌘R" } else { "Rescan  Ctrl+R" };
pub const KEY_MAP_TAB: &str = if MAC { "⌘2" } else { "Ctrl+2" };
pub const KEY_SELECT_CLICK: &str = if MAC { "⌥ Click" } else { "Alt Click" };

/// Finds the user's graphical session when the environment does not name
/// one. A shell that was not started from the desktop (ssh, tmux, a systemd
/// unit, an editor's terminal) has neither `WAYLAND_DISPLAY` nor `DISPLAY`,
/// although the compositor's socket sits in the runtime directory.
///
/// # Safety
/// Sets environment variables: call before any other thread exists.
#[cfg(target_os = "linux")]
pub unsafe fn adopt_display() {
    use std::os::unix::fs::FileTypeExt;
    use std::path::Path;
    let set = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    if set("WAYLAND_DISPLAY") || set("WAYLAND_SOCKET") || set("DISPLAY") {
        return;
    }
    // Lowest-numbered socket in `dir` whose name starts with `prefix`.
    let socket = |dir: &Path, prefix: &str| {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_socket()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.strip_prefix(prefix).is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())))
            .collect();
        names.sort_by_key(|n| (n.len(), n.clone()));
        names.into_iter().next()
    };
    let wayland = std::env::var_os("XDG_RUNTIME_DIR").and_then(|dir| socket(Path::new(&dir), "wayland-"));
    if let Some(name) = wayland {
        // SAFETY: single-threaded, per this function's contract.
        unsafe { std::env::set_var("WAYLAND_DISPLAY", name) };
    } else if let Some(name) = socket(Path::new("/tmp/.X11-unix"), "X") {
        // SAFETY: as above.
        unsafe { std::env::set_var("DISPLAY", format!(":{}", &name[1..])) };
    }
}

/// # Safety
/// None needed here; the signature matches the Linux one.
#[cfg(not(target_os = "linux"))]
pub unsafe fn adopt_display() {}
