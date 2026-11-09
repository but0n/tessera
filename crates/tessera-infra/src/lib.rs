//! Infrastructure adapters for the application ports.

#[cfg(target_os = "macos")]
mod bulk;
mod portable;
mod privacy;
mod sample;
mod trash_bin;
mod volume;

use std::sync::Arc;

use tessera_app::ports::DirectoryReader;

#[cfg(target_os = "macos")]
pub use bulk::BulkReader;
pub use portable::PortableReader;
pub use privacy::{default_policy, has_full_disk_access, home_dir};
pub use sample::{SampleReader, SampleTrash, SampleVolume, sample_root};
pub use trash_bin::SystemTrash;
pub use volume::StatVfs;

/// Fastest reader available on this platform (ADR-0005).
pub fn best_reader() -> Arc<dyn DirectoryReader> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(BulkReader)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(PortableReader)
    }
}
