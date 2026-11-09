/// What kind of content a file holds, inferred from its extension.
///
/// Categories are an *Insight* concept: they answer "what is eating my disk"
/// in human terms. Directories are always [`Category::Folder`] and never
/// contribute to category totals.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Category {
    Folder = 0,
    Video,
    Audio,
    Image,
    Archive,
    Document,
    Code,
    Binary,
    Data,
    Other,
}

impl Category {
    pub const COUNT: usize = 10;

    /// Every category a file can belong to, in display order.
    pub const FILE_KINDS: [Category; 9] =
        [Category::Video, Category::Image, Category::Audio, Category::Archive, Category::Data, Category::Binary, Category::Code, Category::Document, Category::Other];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Folder => "Folders",
            Category::Video => "Video",
            Category::Audio => "Audio",
            Category::Image => "Images",
            Category::Archive => "Archives & Disk Images",
            Category::Document => "Documents",
            Category::Code => "Source Code",
            Category::Binary => "Apps & Binaries",
            Category::Data => "Data & Models",
            Category::Other => "Other",
        }
    }

    /// Compact label for legends and narrow columns.
    pub fn short_label(self) -> &'static str {
        match self {
            Category::Archive => "Archives",
            Category::Document => "Docs",
            Category::Code => "Code",
            Category::Binary => "Apps & Bins",
            Category::Data => "Data",
            c => c.label(),
        }
    }

    pub fn classify(name: &str) -> Category {
        let Some(dot) = name.rfind('.') else {
            return Category::Other;
        };
        if dot == 0 || dot + 1 == name.len() {
            return Category::Other;
        }
        let ext = &name[dot + 1..];
        if ext.len() > 12 {
            return Category::Other;
        }
        let mut buf = [0u8; 12];
        for (i, b) in ext.bytes().enumerate() {
            buf[i] = b.to_ascii_lowercase();
        }
        let ext = std::str::from_utf8(&buf[..ext.len()]).unwrap_or("");
        match ext {
            "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "wmv" | "flv" | "mpg" | "mpeg" | "3gp" | "mts" | "m2ts" | "vob" | "braw" | "r3d" | "prores" => Category::Video,
            "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "aiff" | "aif" | "wma" | "caf" | "mid" | "midi" | "alac" | "logicx" => Category::Audio,
            "jpg" | "jpeg" | "png" | "gif" | "heic" | "heif" | "webp" | "tif" | "tiff" | "bmp" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "raf" | "psd" | "svg" | "ico"
            | "icns" | "exr" | "hdr" | "avif" | "ktx" | "ktx2" | "dds" | "tga" => Category::Image,
            "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "dmg" | "iso" | "pkg" | "xip" | "lz4" | "lzma" | "cab" | "sparseimage" | "sparsebundle"
            | "img" | "vdi" | "vmdk" | "qcow2" | "ipsw" => Category::Archive,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "key" | "pages" | "numbers" | "txt" | "md" | "rtf" | "epub" | "odt" | "ods" | "odp" | "tex" => {
                Category::Document
            }
            "rs" | "c" | "h" | "cc" | "cpp" | "hpp" | "cxx" | "py" | "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "go" | "java" | "kt" | "swift" | "m" | "mm" | "rb" | "php"
            | "cs" | "sh" | "zsh" | "fish" | "toml" | "yaml" | "yml" | "html" | "css" | "scss" | "vue" | "svelte" | "lua" | "zig" | "wgsl" | "glsl" | "hlsl" | "metal" | "dart"
            | "scala" | "hs" | "ml" | "ex" | "exs" | "erl" | "clj" | "nim" | "map" => Category::Code,
            "app" | "dylib" | "so" | "a" | "o" | "rlib" | "rmeta" | "exe" | "dll" | "wasm" | "class" | "pyc" | "node" | "ipa" | "apk" | "aab" | "jar" | "dsym" | "pch" | "pcm"
            | "lib" | "obj" | "bin" | "framework" => Category::Binary,
            "json" | "xml" | "sqlite" | "sqlite3" | "db" | "log" | "csv" | "tsv" | "parquet" | "arrow" | "npy" | "npz" | "pt" | "pth" | "ckpt" | "safetensors" | "gguf"
            | "onnx" | "h5" | "hdf5" | "pb" | "tflite" | "mlmodel" | "realm" | "wal" | "ldb" | "dat" | "idx" | "pack" => Category::Data,
            _ => Category::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_extension_case_insensitively() {
        assert_eq!(Category::classify("Movie.MOV"), Category::Video);
        assert_eq!(Category::classify("model.safetensors"), Category::Data);
        assert_eq!(Category::classify("main.rs"), Category::Code);
        assert_eq!(Category::classify(".zshrc"), Category::Other);
        assert_eq!(Category::classify("Makefile"), Category::Other);
        assert_eq!(Category::classify("trailing."), Category::Other);
        assert_eq!(Category::classify("x.averyveryverylongextension"), Category::Other);
    }
}
