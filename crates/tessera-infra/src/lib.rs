//! Infrastructure adapters for the application ports.

#[cfg(target_os = "macos")]
mod bulk;
mod desktop;
mod portable;
mod privacy;
mod sample;
#[cfg(target_os = "linux")]
mod statx;
mod trash_bin;
mod volume;

use std::sync::Arc;

use tessera_app::ports::DirectoryReader;

#[cfg(target_os = "macos")]
pub use bulk::BulkReader;
pub use desktop::SystemDesktop;
pub use portable::PortableReader;
pub use privacy::{default_policy, has_full_disk_access, home_dir};
pub use sample::{SampleDesktop, SampleReader, SampleTrash, SampleVolume, sample_root};
#[cfg(target_os = "linux")]
pub use statx::StatxReader;
pub use trash_bin::SystemTrash;
pub use volume::StatVfs;

/// The reader that knows this platform best (ADR-0005, ADR-0014).
pub fn best_reader() -> Arc<dyn DirectoryReader> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(BulkReader)
    }
    #[cfg(target_os = "linux")]
    {
        Arc::new(StatxReader)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Arc::new(PortableReader)
    }
}
