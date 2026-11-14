//! Driven ports: what the application needs from the outside world.
//! Implemented by `tessera-infra`; faked in tests.

use std::io;
use std::path::Path;

use tessera_domain::Listing;

/// Reads one directory level. Must not follow symlinks and must report
/// on-disk (allocated) sizes.
pub trait DirectoryReader: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn read(&self, path: &Path, out: &mut Listing) -> io::Result<()>;
}

/// Moves an entry to the user's Trash (never unlinks; see ADR-0007).
pub trait Trash: Send + Sync + 'static {
    fn trash(&self, path: &Path) -> Result<(), String>;
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct VolumeStats {
    pub total: u64,
    pub available: u64,
}

impl VolumeStats {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
}

pub trait VolumeProbe: Send + Sync + 'static {
    fn probe(&self, path: &Path) -> Option<VolumeStats>;
}

/// Hands the user over to the rest of their desktop. Fire and forget: must
/// not block the caller, and failing is silent.
pub trait Desktop: Send + Sync + 'static {
    /// Shows `path` selected in the system file manager.
    fn reveal(&self, path: &Path);
    /// Opens the system settings where access to protected locations is
    /// granted (ADR-0009). A no-op where nothing is ever sealed.
    fn open_privacy_settings(&self);
}
