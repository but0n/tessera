use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use tessera_app::ports::{VolumeProbe, VolumeStats};

/// Capacity of the volume containing a path, via `statvfs(3)`.
pub struct StatVfs;

impl VolumeProbe for StatVfs {
    fn probe(&self, path: &Path) -> Option<VolumeStats> {
        let c = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
            return None;
        }
        let frsize = s.f_frsize as u64;
        Some(VolumeStats { total: s.f_blocks as u64 * frsize, available: s.f_bavail as u64 * frsize })
    }
}
