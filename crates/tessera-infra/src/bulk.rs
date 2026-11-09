//! macOS fast path: `getattrlistbulk(2)` returns names, types and sizes of
//! many entries per syscall, avoiding one `lstat` per file (ADR-0005).

use std::cell::RefCell;
use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

use tessera_app::ports::DirectoryReader;
use tessera_domain::{EntryKind, Listing};

#[repr(C)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

const ATTR_BIT_MAP_COUNT: u16 = 5;
const ATTR_CMN_NAME: u32 = 0x0000_0001;
const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
const ATTR_CMN_FILEID: u32 = 0x0200_0000;
const ATTR_CMN_ERROR: u32 = 0x2000_0000;
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
const ATTR_DIR_MOUNTSTATUS: u32 = 0x0000_0004;
const ATTR_FILE_LINKCOUNT: u32 = 0x0000_0001;
const ATTR_FILE_ALLOCSIZE: u32 = 0x0000_0004;
const DIR_MNTSTATUS_MNTPOINT: u32 = 0x0000_0001;

// fsobj_type_t values from <sys/vnode.h>
const VREG: u32 = 1;
const VDIR: u32 = 2;
const VLNK: u32 = 5;

unsafe extern "C" {
    fn getattrlistbulk(dirfd: libc::c_int, attr_list: *mut AttrList, attr_buf: *mut c_void, attr_buf_size: usize, options: u64) -> libc::c_int;
}

const BUF_SIZE: usize = 128 * 1024;

thread_local! {
    static BUF: RefCell<Vec<u64>> = RefCell::new(vec![0u64; BUF_SIZE / 8]);
}

pub struct BulkReader;

/// Cursor over one packed entry. Fields are 4-byte aligned at best, so every
/// read is unaligned.
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn u32(&mut self) -> u32 {
        let v = u32::from_ne_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        v
    }
    fn i32(&mut self) -> i32 {
        self.u32() as i32
    }
    fn u64(&mut self) -> u64 {
        let v = u64::from_ne_bytes(self.buf[self.pos..self.pos + 8].try_into().unwrap());
        self.pos += 8;
        v
    }
}

impl DirectoryReader for BulkReader {
    fn name(&self) -> &'static str {
        "getattrlistbulk"
    }

    fn read(&self, path: &Path, out: &mut Listing) -> io::Result<()> {
        out.clear();
        let dir = File::open(path)?;
        let fd = dir.as_raw_fd();
        let mut al = AttrList {
            bitmapcount: ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_ERROR | ATTR_CMN_OBJTYPE | ATTR_CMN_FILEID,
            volattr: 0,
            dirattr: ATTR_DIR_MOUNTSTATUS,
            fileattr: ATTR_FILE_LINKCOUNT | ATTR_FILE_ALLOCSIZE,
            forkattr: 0,
        };
        BUF.with_borrow_mut(|words| {
            loop {
                let n = unsafe { getattrlistbulk(fd, &mut al, words.as_mut_ptr().cast(), BUF_SIZE, 0) };
                if n < 0 {
                    return Err(io::Error::last_os_error());
                }
                if n == 0 {
                    return Ok(());
                }
                let bytes: &[u8] = bytemuck_bytes(words);
                let mut off = 0usize;
                for _ in 0..n {
                    let len = u32::from_ne_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
                    if len == 0 || off + len > bytes.len() {
                        break;
                    }
                    parse_entry(&bytes[off..off + len], out);
                    off += len;
                }
            }
        })
    }
}

fn bytemuck_bytes(words: &[u64]) -> &[u8] {
    // SAFETY: u64 slice reinterpreted as bytes; alignment of u8 is 1.
    unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), words.len() * 8) }
}

fn parse_entry(e: &[u8], out: &mut Listing) {
    let mut c = Cursor { buf: e, pos: 4 };
    let common = c.u32();
    let _vol = c.u32();
    let dir = c.u32();
    let file = c.u32();
    let _fork = c.u32();

    if common & ATTR_CMN_ERROR != 0 && c.u32() != 0 {
        return;
    }
    let mut name = "";
    if common & ATTR_CMN_NAME != 0 {
        let at = c.pos;
        let rel = c.i32();
        let nlen = c.u32() as usize;
        let start = (at as isize + rel as isize) as usize;
        if nlen > 0 && start + nlen <= e.len() {
            let raw = &e[start..start + nlen - 1]; // strip NUL
            name = std::str::from_utf8(raw).unwrap_or("");
            if name.is_empty() {
                let lossy = String::from_utf8_lossy(raw).into_owned();
                return push(out, &lossy, &mut c, common, dir, file);
            }
        }
    }
    push(out, name, &mut c, common, dir, file);
}

fn push(out: &mut Listing, name: &str, c: &mut Cursor, common: u32, dir: u32, file: u32) {
    if name.is_empty() {
        return;
    }
    let objtype = if common & ATTR_CMN_OBJTYPE != 0 { c.u32() } else { 0 };
    let inode = if common & ATTR_CMN_FILEID != 0 { c.u64() } else { 0 };
    let mut mount_point = false;
    if dir & ATTR_DIR_MOUNTSTATUS != 0 {
        mount_point = c.u32() & DIR_MNTSTATUS_MNTPOINT != 0;
    }
    let mut nlink = 1;
    let mut alloc = 0;
    if file & ATTR_FILE_LINKCOUNT != 0 {
        nlink = c.u32();
    }
    if file & ATTR_FILE_ALLOCSIZE != 0 {
        alloc = c.u64();
    }
    let kind = match objtype {
        VREG => EntryKind::File,
        VDIR => EntryKind::Dir,
        VLNK => EntryKind::Symlink,
        _ => EntryKind::Other,
    };
    out.push(name, kind, alloc, inode, nlink, mount_point);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PortableReader;

    fn snapshot(r: &dyn DirectoryReader, p: &Path) -> Vec<(String, EntryKind, u64, u64)> {
        let mut l = Listing::default();
        r.read(p, &mut l).unwrap();
        let mut v: Vec<_> = l.iter().map(|(n, e)| (n.to_owned(), e.kind, if e.kind == EntryKind::Dir { 0 } else { e.alloc }, e.inode)).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// The fast path must agree with `lstat` on names, kinds, sizes, inodes.
    #[test]
    fn agrees_with_portable_reader() {
        let tmp = std::env::temp_dir().join(format!("tessera-bulk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("sub dir/ünïcødé")).unwrap();
        std::fs::write(tmp.join("a.bin"), vec![7u8; 123_457]).unwrap();
        std::fs::write(tmp.join("文件.txt"), b"hello").unwrap();
        std::fs::write(tmp.join("empty"), b"").unwrap();
        std::fs::hard_link(tmp.join("a.bin"), tmp.join("a-link.bin")).unwrap();
        std::os::unix::fs::symlink("a.bin", tmp.join("link")).unwrap();

        let fast = snapshot(&BulkReader, &tmp);
        let slow = snapshot(&PortableReader, &tmp);
        assert_eq!(fast, slow);
        assert_eq!(fast.len(), 6);

        let mut l = Listing::default();
        BulkReader.read(&tmp, &mut l).unwrap();
        let (_, a) = l.iter().find(|(n, _)| *n == "a.bin").unwrap();
        assert_eq!(a.nlink, 2);
        assert!(a.alloc >= 123_457);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn missing_dir_is_an_error() {
        let mut l = Listing::default();
        assert!(BulkReader.read(Path::new("/definitely/not/here"), &mut l).is_err());
    }
}
