use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use tessera_domain::ScanPolicy;
use tessera_domain::protected::protected_paths;

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Full Disk Access probe: the TCC database is readable only with FDA, and
/// a failed open returns `EPERM` immediately without prompting.
pub fn has_full_disk_access() -> bool {
    let Some(home) = home_dir() else { return false };
    std::fs::File::open(home.join("Library/Application Support/com.apple.TCC/TCC.db")).is_ok()
}

/// Scan policy for this machine: seal privacy-protected locations unless the
/// process already has Full Disk Access (ADR-0009).
pub fn default_policy() -> ScanPolicy {
    let protected: HashSet<PathBuf> = match home_dir() {
        Some(home) if cfg!(target_os = "macos") && !has_full_disk_access() => protected_paths(&home).into_iter().collect(),
        _ => HashSet::new(),
    };
    ScanPolicy { one_filesystem: true, protected: Arc::new(protected) }
}
