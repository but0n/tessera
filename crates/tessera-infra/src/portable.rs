use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use tessera_app::ports::DirectoryReader;
use tessera_domain::{EntryKind, Listing};

/// `readdir` + `lstat` per entry. Works on any Unix; used as the reference
/// implementation the platform readers are tested against.
pub struct PortableReader;

impl DirectoryReader for PortableReader {
    fn name(&self) -> &'static str {
        "readdir+lstat"
    }

    fn read(&self, path: &Path, out: &mut Listing) -> io::Result<()> {
        out.clear();
        let dev = fs::symlink_metadata(path)?.dev();
        for entry in fs::read_dir(path)? {
            let Ok(entry) = entry else { continue };
            let Ok(md) = entry.metadata() else { continue };
            let ft = md.file_type();
            let kind = if ft.is_dir() {
                EntryKind::Dir
            } else if ft.is_file() {
                EntryKind::File
            } else if ft.is_symlink() {
                EntryKind::Symlink
            } else {
                EntryKind::Other
            };
            let name = entry.file_name();
            out.push(&name.to_string_lossy(), kind, md.blocks() * 512, md.ino(), md.nlink() as u32, kind == EntryKind::Dir && md.dev() != dev);
        }
        Ok(())
    }
}
