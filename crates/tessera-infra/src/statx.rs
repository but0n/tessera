//! Linux reader: `getdents64(2)` + one `statx(2)` per entry, relative to the
//! open directory (ADR-0014).
//!
//! `statx` is here for what it knows more than for speed. It reports whether
//! an entry is the root of a mount, which is what a boundary means: on btrfs
//! every subvolume has its own device number, so comparing `st_dev` would
//! stop at each one. It also never triggers an automount and does not wait
//! on a network filesystem to sync.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use tessera_app::ports::DirectoryReader;
use tessera_domain::{EntryKind, Listing};

pub struct StatxReader;

const MASK: libc::c_uint = libc::STATX_TYPE | libc::STATX_NLINK | libc::STATX_INO | libc::STATX_BLOCKS;
const FLAGS: libc::c_int = libc::AT_SYMLINK_NOFOLLOW | libc::AT_NO_AUTOMOUNT | libc::AT_STATX_DONT_SYNC;
const MOUNT_ROOT: u64 = libc::STATX_ATTR_MOUNT_ROOT as u64;

const BUF_SIZE: usize = 64 * 1024;
/// Offset of `d_name` in `struct linux_dirent64`: ino (8), off (8),
/// reclen (2), type (1).
const NAME_OFFSET: usize = 19;

thread_local! {
    /// `getdents64` output; `u64` words keep the records aligned.
    static BUF: RefCell<Vec<u64>> = RefCell::new(vec![0u64; BUF_SIZE / 8]);
}

fn statx(dirfd: libc::c_int, name: &CStr, flags: libc::c_int) -> Option<libc::statx> {
    let mut st: libc::statx = unsafe { std::mem::zeroed() };
    // SAFETY: `name` is NUL-terminated and `st` is a valid out-pointer.
    (unsafe { libc::statx(dirfd, name.as_ptr(), flags, MASK, &mut st) } == 0).then_some(st)
}

fn device(st: &libc::statx) -> u64 {
    (st.stx_dev_major as u64) << 32 | st.stx_dev_minor as u64
}

/// Inode numbers are only unique per device, and one scan can span several
/// (btrfs subvolumes), so the hard-link identity folds the device in.
fn identity(st: &libc::statx) -> u64 {
    st.stx_ino ^ device(st).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

impl DirectoryReader for StatxReader {
    fn name(&self) -> &'static str {
        "getdents+statx"
    }

    /// Four syscalls per directory (`open`, two `getdents64`, `close`) plus
    /// one `statx` per entry; libc's `readdir` would add an `fstat` and an
    /// `fcntl` per directory and a heap buffer.
    fn read(&self, path: &Path, out: &mut Listing) -> io::Result<()> {
        out.clear();
        let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: NUL-terminated path; a non-negative result is a fresh
        // descriptor that `OwnedFd` closes on every exit path.
        let dir = unsafe {
            let fd = libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            OwnedFd::from_raw_fd(fd)
        };
        let fd = dir.as_raw_fd();
        // Device of the directory itself: only looked up where the kernel
        // or filesystem cannot say "mount root".
        let mut own_dev: Option<Option<u64>> = None;
        BUF.with_borrow_mut(|words| {
            loop {
                // SAFETY: the kernel writes at most `BUF_SIZE` bytes into `words`.
                let n = unsafe { libc::syscall(libc::SYS_getdents64, fd, words.as_mut_ptr(), BUF_SIZE) };
                if n < 0 {
                    return Err(io::Error::last_os_error());
                }
                if n == 0 {
                    return Ok(());
                }
                // SAFETY: `n` initialised bytes inside the buffer.
                let bytes: &[u8] = unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), n as usize) };
                let mut off = 0usize;
                while off + NAME_OFFSET < bytes.len() {
                    let reclen = u16::from_ne_bytes([bytes[off + 16], bytes[off + 17]]) as usize;
                    if reclen <= NAME_OFFSET || off + reclen > bytes.len() {
                        break;
                    }
                    let record = &bytes[off + NAME_OFFSET..off + reclen];
                    off += reclen;
                    // The kernel NUL-terminates every name inside its record.
                    let Ok(name) = CStr::from_bytes_until_nul(record) else { continue };
                    if matches!(name.to_bytes(), b"." | b"..") {
                        continue;
                    }
                    // Gone between getdents and statx: skip it, like the other readers.
                    let Some(st) = statx(fd, name, FLAGS) else { continue };
                    let kind = match st.stx_mode as libc::mode_t & libc::S_IFMT {
                        libc::S_IFDIR => EntryKind::Dir,
                        libc::S_IFREG => EntryKind::File,
                        libc::S_IFLNK => EntryKind::Symlink,
                        _ => EntryKind::Other,
                    };
                    let mount_point = kind == EntryKind::Dir
                        && if st.stx_attributes_mask & MOUNT_ROOT != 0 {
                            st.stx_attributes & MOUNT_ROOT != 0
                        } else {
                            Some(device(&st)) != *own_dev.get_or_insert_with(|| statx(fd, c"", FLAGS | libc::AT_EMPTY_PATH).map(|s| device(&s)))
                        };
                    out.push(&name.to_string_lossy(), kind, st.stx_blocks * 512, identity(&st), st.stx_nlink, mount_point);
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PortableReader;

    fn snapshot(r: &dyn DirectoryReader, p: &Path) -> Vec<(String, EntryKind, u64, u32, bool)> {
        let mut l = Listing::default();
        r.read(p, &mut l).unwrap();
        let mut v: Vec<_> = l.iter().map(|(n, e)| (n.to_owned(), e.kind, if e.kind == EntryKind::Dir { 0 } else { e.alloc }, e.nlink, e.mount_point)).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// The fast path must agree with `lstat` on names, kinds, sizes and link
    /// counts, and give hard links one identity.
    #[test]
    fn agrees_with_portable_reader() {
        let tmp = std::env::temp_dir().join(format!("tessera-statx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("sub dir/ünïcødé")).unwrap();
        std::fs::write(tmp.join("a.bin"), vec![7u8; 123_457]).unwrap();
        std::fs::write(tmp.join("文件.txt"), b"hello").unwrap();
        std::fs::write(tmp.join("empty"), b"").unwrap();
        std::fs::hard_link(tmp.join("a.bin"), tmp.join("a-link.bin")).unwrap();
        std::os::unix::fs::symlink("a.bin", tmp.join("link")).unwrap();

        let fast = snapshot(&StatxReader, &tmp);
        assert_eq!(fast, snapshot(&PortableReader, &tmp));
        assert_eq!(fast.len(), 6);

        let mut l = Listing::default();
        StatxReader.read(&tmp, &mut l).unwrap();
        let entry = |name: &str| *l.iter().find(|(n, _)| *n == name).unwrap().1;
        assert_eq!(entry("a.bin").nlink, 2);
        assert!(entry("a.bin").alloc >= 123_457);
        assert_eq!(entry("a.bin").inode, entry("a-link.bin").inode);
        assert_ne!(entry("a.bin").inode, entry("empty").inode);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// `/proc` is always its own mount: a boundary, whatever its device.
    #[test]
    fn mount_roots_are_boundaries() {
        let mut l = Listing::default();
        StatxReader.read(Path::new("/"), &mut l).unwrap();
        let entry = |name: &str| l.iter().find(|(n, _)| *n == name).map(|(_, e)| *e);
        if let Some(proc) = entry("proc") {
            assert!(proc.mount_point);
        }
        if let Some(etc) = entry("etc") {
            assert!(!etc.mount_point);
        }
    }

    /// More names than one `getdents64` buffer holds.
    #[test]
    fn reads_directories_larger_than_the_buffer() {
        let tmp = std::env::temp_dir().join(format!("tessera-statx-big-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let n = 3000;
        for i in 0..n {
            std::fs::write(tmp.join(format!("a-rather-long-file-name-to-fill-the-buffer-{i:05}")), b"").unwrap();
        }
        assert_eq!(snapshot(&StatxReader, &tmp), snapshot(&PortableReader, &tmp));
        assert_eq!(snapshot(&StatxReader, &tmp).len(), n);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn missing_dir_is_an_error() {
        let mut l = Listing::default();
        assert!(StatxReader.read(Path::new("/definitely/not/here"), &mut l).is_err());
    }
}
