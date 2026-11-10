//! Sample data: a fictional, deterministic developer folder served through
//! the real ports. Used for demos, screenshots and UI work, so they never
//! show anyone's actual files (`tessera --sample`).
//!
//! Every directory's listing is a pure function of its path: the reader
//! replays the generator from the root to find what kind of folder a path
//! is, then expands it with an RNG seeded by the path.

use std::io;
use std::path::{Path, PathBuf};

use tessera_app::ports::{DirectoryReader, Trash, VolumeProbe, VolumeStats};
use tessera_domain::{EntryKind, Listing};

const KB: u64 = 1 << 10;
const MB: u64 = 1 << 20;
const GB: u64 = 1 << 30;

/// Path the sample tree pretends to live at.
pub fn sample_root() -> PathBuf {
    PathBuf::from("~/Developer")
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum Kind {
    Root,
    Rust(f32),
    Node(f32),
    Python,
    Source(u8),
    Target(f32),
    Profile(f32),
    Incremental,
    NodeModules,
    Package(f32),
    Venv,
    SitePackages,
    Footage,
    Shoot,
    Photos,
    Month,
    Datasets,
    Assets,
    Archives,
    Blobs(&'static [&'static str], u64, u64, u32),
}

struct Entry {
    name: String,
    size: u64,
    dir: Option<Kind>,
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next() % (hi - lo + 1) as u64) as u32
    }
    /// Log-uniform size in `[lo, hi]`: many small, few large.
    fn size(&mut self, lo: u64, hi: u64) -> u64 {
        let (a, b) = ((lo as f64).ln(), (hi as f64).ln());
        (a + (b - a) * self.unit().powf(1.6)).exp() as u64
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

fn file(name: impl Into<String>, size: u64) -> Entry {
    Entry { name: name.into(), size, dir: None }
}

fn dir(name: impl Into<String>, kind: Kind) -> Entry {
    Entry { name: name.into(), size: 0, dir: Some(kind) }
}

const WORDS: &[&str] = &[
    "core", "render", "net", "io", "util", "graph", "shader", "parse", "cache", "store", "auth", "ui", "layout", "query", "codec", "audio", "physics", "scene", "sync", "index",
    "plugin", "cli", "math", "event", "worker", "theme", "fonts", "api", "model", "geom",
];
const PACKAGES: &[&str] = &[
    "react",
    "react-dom",
    "typescript",
    "esbuild",
    "vite",
    "rollup",
    "postcss",
    "tailwindcss",
    "zod",
    "lodash",
    "date-fns",
    "three",
    "d3",
    "eslint",
    "prettier",
    "@babel/core",
    "@swc/core",
    "sharp",
    "playwright-core",
    "@next/swc-darwin-arm64",
    "next",
    "framer-motion",
    "rxjs",
    "axios",
    "undici",
    "@types/node",
    "webpack",
    "terser",
    "caniuse-lite",
    "monaco-editor",
    "pdfjs-dist",
    "@img/sharp-darwin-arm64",
    "prisma",
    "@prisma/engines",
];
const PY: &[&str] = &["numpy", "torch", "pandas", "scipy", "matplotlib", "jax", "sklearn", "transformers", "polars", "pyarrow", "tokenizers", "jupyterlab"];

fn listing(kind: Kind, rng: &mut Rng) -> Vec<Entry> {
    let mut v = Vec::new();
    match kind {
        Kind::Root => {
            v.push(dir("aurora-engine", Kind::Rust(3.2)));
            v.push(dir("atlas-web", Kind::Node(1.6)));
            v.push(dir("footage", Kind::Footage));
            v.push(dir("datasets", Kind::Datasets));
            v.push(dir("photos-raw", Kind::Photos));
            v.push(dir("tidepool", Kind::Rust(1.4)));
            v.push(dir("ml-notebooks", Kind::Python));
            v.push(dir("harbor-dashboard", Kind::Node(0.8)));
            v.push(dir("game-assets", Kind::Assets));
            v.push(dir("archives", Kind::Archives));
            v.push(dir("lumen-cli", Kind::Rust(0.4)));
            v.push(dir("sketches", Kind::Source(2)));
            v.push(file("backup-2025-06.tar.gz", 3 * GB + 410 * MB));
            v.push(file("Screen Recording 2026-03-14 at 09.41.27.mov", 2 * GB + 640 * MB));
            v.push(file("notes.md", 18 * KB));
        }
        Kind::Rust(s) => {
            v.push(file("Cargo.toml", 2 * KB));
            v.push(file("Cargo.lock", 96 * KB));
            v.push(file("README.md", 6 * KB));
            v.push(dir("src", Kind::Source(0)));
            v.push(dir("crates", Kind::Source(1)));
            v.push(dir("target", Kind::Target(s)));
            if s > 1.0 {
                v.push(dir("assets", Kind::Assets));
                v.push(dir("benches", Kind::Source(2)));
            }
        }
        Kind::Target(s) => {
            v.push(dir("debug", Kind::Profile(s)));
            v.push(dir("release", Kind::Profile(s * 0.45)));
            v.push(dir("doc", Kind::Source(3)));
        }
        Kind::Profile(s) => {
            let n = (60.0 * s) as u32;
            v.push(dir("deps", Kind::Blobs(&["rlib", "rmeta", "dylib", "o"], 200 * KB, (220.0 * s as f64) as u64 * MB, n.max(20))));
            v.push(dir("incremental", Kind::Incremental));
            v.push(dir("build", Kind::Blobs(&["o", "rs", "a"], 4 * KB, 40 * MB, 30)));
            v.push(dir("examples", Kind::Blobs(&["bin", "d"], MB, (90.0 * s as f64) as u64 * MB, 8)));
        }
        Kind::Incremental => {
            for _ in 0..rng.range(14, 30) {
                let name = format!("{}-{:x}", rng.pick(WORDS), rng.next() & 0xffff_ffff);
                v.push(dir(name, Kind::Blobs(&["bin", "o"], 8 * KB, 90 * MB, 18)));
            }
        }
        Kind::Node(s) => {
            v.push(file("package.json", 3 * KB));
            v.push(file("pnpm-lock.yaml", 410 * KB));
            v.push(dir("src", Kind::Source(0)));
            v.push(dir("public", Kind::Blobs(&["png", "svg", "woff2", "webp"], 4 * KB, 6 * MB, 30)));
            v.push(dir("node_modules", Kind::NodeModules));
            v.push(dir(".next", Kind::Blobs(&["js", "map", "json", "pack"], 8 * KB, (180.0 * s as f64) as u64 * MB, 40)));
            v.push(dir("dist", Kind::Blobs(&["js", "css", "map"], 4 * KB, 12 * MB, 24)));
        }
        Kind::NodeModules => {
            for p in PACKAGES {
                if rng.unit() < 0.8 {
                    let heavy = ["@next/swc-darwin-arm64", "@swc/core", "esbuild", "playwright-core", "@prisma/engines", "@img/sharp-darwin-arm64", "monaco-editor"].contains(p);
                    v.push(dir(*p, Kind::Package(if heavy { 6.0 } else { 1.0 })));
                }
            }
            v.push(dir(".pnpm", Kind::Blobs(&["js", "json", "node"], 4 * KB, 60 * MB, 50)));
        }
        Kind::Package(s) => {
            v.push(file("package.json", 2 * KB));
            v.push(file("README.md", 12 * KB));
            v.push(dir("dist", Kind::Blobs(&["js", "cjs", "map", "d.ts"], 2 * KB, (4.0 * s as f64) as u64 * MB, 24)));
            if s > 1.0 {
                v.push(file(format!("{}.node", "binding"), rng.size(40 * MB, 140 * MB)));
            }
        }
        Kind::Python => {
            v.push(file("pyproject.toml", 2 * KB));
            v.push(dir(".venv", Kind::Venv));
            v.push(dir("notebooks", Kind::Blobs(&["ipynb"], 40 * KB, 60 * MB, 24)));
            v.push(dir("data", Kind::Blobs(&["parquet", "npy", "csv"], 4 * MB, 2 * GB, 14)));
            v.push(dir("checkpoints", Kind::Blobs(&["pt", "safetensors"], 200 * MB, 5 * GB, 6)));
            v.push(dir("__pycache__", Kind::Blobs(&["pyc"], 2 * KB, 300 * KB, 30)));
        }
        Kind::Venv => {
            v.push(dir("lib", Kind::SitePackages));
            v.push(dir("bin", Kind::Blobs(&["py", "sh"], KB, 9 * MB, 30)));
        }
        Kind::SitePackages => {
            for p in PY {
                let hi = if *p == "torch" || *p == "jax" { 700 * MB } else { 60 * MB };
                v.push(dir(*p, Kind::Blobs(&["py", "so", "dylib", "pyi"], 2 * KB, hi, 30)));
            }
        }
        Kind::Footage => {
            for y in ["2024", "2025", "2026"] {
                v.push(dir(y, Kind::Shoot));
            }
            v.push(dir("proxies", Kind::Blobs(&["mp4"], 40 * MB, 900 * MB, 30)));
        }
        Kind::Shoot => {
            for _ in 0..rng.range(3, 6) {
                let name = rng.pick(&["iceland-trip", "studio-session", "drone-coast", "launch-event", "timelapse", "interview", "city-night", "b-roll"]);
                v.push(dir(format!("{name}-{:02}", rng.range(1, 28)), Kind::Blobs(&["mov", "mp4", "braw"], 300 * MB, 9 * GB, 10)));
            }
        }
        Kind::Photos => {
            for y in ["2024", "2025"] {
                v.push(dir(y, Kind::Month));
            }
            v.push(dir("exports", Kind::Blobs(&["jpg", "heic"], 2 * MB, 14 * MB, 120)));
        }
        Kind::Month => {
            for m in ["01", "03", "04", "06", "07", "08", "10", "12"] {
                v.push(dir(m, Kind::Blobs(&["dng", "jpg", "cr3"], 6 * MB, 70 * MB, 90)));
            }
        }
        Kind::Datasets => {
            v.push(dir("imagenet-subset", Kind::Blobs(&["tar", "idx"], 200 * MB, 9 * GB, 12)));
            v.push(dir("telemetry", Kind::Blobs(&["parquet"], 30 * MB, 3 * GB, 40)));
            v.push(dir("osm-extracts", Kind::Blobs(&["pbf", "gpkg"], 100 * MB, 6 * GB, 10)));
            v.push(dir("audio-corpus", Kind::Blobs(&["flac", "wav"], 10 * MB, 400 * MB, 60)));
            v.push(file("embeddings.npy", 11 * GB + 300 * MB));
        }
        Kind::Assets => {
            v.push(dir("textures", Kind::Blobs(&["ktx2", "png", "exr"], MB, 400 * MB, 50)));
            v.push(dir("models", Kind::Blobs(&["glb", "fbx", "obj"], 2 * MB, 600 * MB, 30)));
            v.push(dir("audio", Kind::Blobs(&["wav", "ogg"], 500 * KB, 120 * MB, 40)));
        }
        Kind::Archives => {
            v.push(file("ubuntu-24.04-desktop-arm64.iso", 2 * GB + 900 * MB));
            v.push(file("Xcode_16.xip", 3 * GB + 200 * MB));
            v.push(file("old-laptop-home.zip", 7 * GB + 600 * MB));
            v.push(dir("installers", Kind::Blobs(&["dmg", "pkg"], 50 * MB, 2 * GB, 14)));
        }
        Kind::Source(depth) => {
            for _ in 0..rng.range(6, 18) {
                let ext = rng.pick(&["rs", "ts", "tsx", "py", "wgsl", "md", "json"]);
                v.push(file(format!("{}.{ext}", rng.pick(WORDS)), rng.size(2 * KB, 160 * KB)));
            }
            if depth < 4 {
                for _ in 0..rng.range(1, 5 - depth as u32) {
                    v.push(dir(rng.pick(WORDS), Kind::Source(depth + 1)));
                }
            }
        }
        Kind::Blobs(exts, lo, hi, n) => {
            for i in 0..rng.range(n / 2, n) {
                let ext = rng.pick(exts);
                let name = match ext {
                    "mov" | "mp4" | "braw" => format!("A{:03}_C{:03}.{ext}", rng.range(1, 40), i),
                    "dng" | "jpg" | "cr3" | "heic" => format!("IMG_{:04}.{ext}", 1000 + i * 7 + rng.range(0, 6)),
                    _ => format!("{}-{i:03}.{ext}", rng.pick(WORDS)),
                };
                v.push(file(name, rng.size(lo, hi)));
            }
        }
    }
    // Names must be unique within a directory.
    let mut seen = std::collections::HashSet::new();
    v.retain(|e| seen.insert(e.name.clone()));
    v
}

/// Kind of the directory at `rel` (components below the root), or `None`.
fn kind_at(rel: &[String]) -> Option<Kind> {
    let mut kind = Kind::Root;
    let mut path = String::new();
    for name in rel {
        let entries = listing(kind, &mut Rng::new(fnv(&path)));
        kind = entries.into_iter().find(|e| &e.name == name)?.dir?;
        path.push('/');
        path.push_str(name);
    }
    Some(kind)
}

/// Directory reader over the sample tree.
pub struct SampleReader;

impl DirectoryReader for SampleReader {
    fn name(&self) -> &'static str {
        "sample data"
    }

    fn read(&self, path: &Path, out: &mut Listing) -> io::Result<()> {
        let rel: Vec<String> = path.strip_prefix(sample_root()).map_err(|_| io::Error::from(io::ErrorKind::NotFound))?.iter().map(|c| c.to_string_lossy().into_owned()).collect();
        let kind = kind_at(&rel).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        let key: String = rel.iter().map(|c| format!("/{c}")).collect();
        for e in listing(kind, &mut Rng::new(fnv(&key))) {
            let inode = fnv(&format!("{key}/{}", e.name));
            match e.dir {
                Some(_) => out.push(&e.name, EntryKind::Dir, 0, inode, 1, false),
                None => out.push(&e.name, EntryKind::File, e.size.div_ceil(4096) * 4096, inode, 1, false),
            }
        }
        Ok(())
    }
}

/// Trash for sample data: there is nothing on disk, so "moving" always
/// succeeds and the commit flow can be tried without consequences.
pub struct SampleTrash;

impl Trash for SampleTrash {
    fn trash(&self, _: &Path) -> Result<(), String> {
        Ok(())
    }
}

pub struct SampleVolume;

impl VolumeProbe for SampleVolume {
    fn probe(&self, _: &Path) -> Option<VolumeStats> {
        Some(VolumeStats { total: 994_662_584_320, available: 161_061_273_600 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_resolvable() {
        let mut a = Listing::default();
        let mut b = Listing::default();
        let p = sample_root().join("aurora-engine/target/debug");
        SampleReader.read(&p, &mut a).unwrap();
        SampleReader.read(&p, &mut b).unwrap();
        assert_eq!(a.len(), b.len());
        assert!(a.len() >= 4);
        assert!(SampleReader.read(&sample_root().join("no-such-dir"), &mut a).is_err());
    }
}
