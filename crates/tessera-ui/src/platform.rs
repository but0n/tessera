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
