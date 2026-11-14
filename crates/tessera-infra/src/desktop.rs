//! The system file manager and settings, reached by spawning the platform's
//! own launchers. Nothing here waits on the UI thread.

use std::path::Path;

use tessera_app::ports::Desktop;

pub struct SystemDesktop;

#[cfg(target_os = "macos")]
impl Desktop for SystemDesktop {
    fn reveal(&self, path: &Path) {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }

    fn open_privacy_settings(&self) {
        let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles").spawn();
    }
}

#[cfg(not(target_os = "macos"))]
impl Desktop for SystemDesktop {
    /// Asks the file manager to select the entry (the freedesktop
    /// `FileManager1` D-Bus interface: Nautilus, Dolphin, Nemo, Thunar…).
    /// Where nobody answers, opens the containing folder instead.
    fn reveal(&self, path: &Path) {
        use std::process::{Command, Stdio};
        let path = path.to_path_buf();
        let _ = std::thread::Builder::new().name("tessera-reveal".into()).spawn(move || {
            let quiet = |c: &mut Command| c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
            let shown = quiet(Command::new("dbus-send").args([
                "--session",
                "--print-reply",
                "--reply-timeout=3000",
                "--dest=org.freedesktop.FileManager1",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("array:string:{}", file_uri(&path)),
                "string:",
            ]));
            if !shown {
                quiet(Command::new("xdg-open").arg(path.parent().unwrap_or(&path)));
            }
        });
    }

    /// Nothing is sealed outside macOS (ADR-0009), so there is nothing to grant.
    fn open_privacy_settings(&self) {}
}

/// `file://` URI of an absolute path, percent-encoding everything outside
/// the unreserved set (which also keeps `,` from splitting the D-Bus array).
#[cfg(not(target_os = "macos"))]
fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'_' | b'~') {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn uris_escape_everything_special() {
        assert_eq!(file_uri(Path::new("/home/u/a b,c/文.txt")), "file:///home/u/a%20b%2Cc/%E6%96%87.txt");
    }
}
