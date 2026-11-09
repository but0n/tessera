/// Why a directory is probably safe to reclaim.
///
/// Hints are domain knowledge about *regenerable* content: things a tool can
/// rebuild or re-download. They are advisory only; nothing is ever marked
/// automatically (see ADR-0007).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ReclaimHint {
    #[default]
    None = 0,
    /// Package-manager installs: `node_modules`, `.venv`, `Pods`, …
    Dependencies,
    /// Compiler / bundler output: `target`, `DerivedData`, `.next`, …
    BuildArtifacts,
    /// Tool and application caches.
    Cache,
    /// Already-deleted content waiting in a trash folder.
    Trash,
}

impl ReclaimHint {
    pub fn is_some(self) -> bool {
        self != ReclaimHint::None
    }

    pub fn label(self) -> &'static str {
        match self {
            ReclaimHint::None => "",
            ReclaimHint::Dependencies => "Dependencies",
            ReclaimHint::BuildArtifacts => "Build output",
            ReclaimHint::Cache => "Cache",
            ReclaimHint::Trash => "Trash",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            ReclaimHint::None => "",
            ReclaimHint::Dependencies => "Reinstallable by the package manager",
            ReclaimHint::BuildArtifacts => "Rebuilt by the next compile",
            ReclaimHint::Cache => "Recreated on demand by its owner",
            ReclaimHint::Trash => "Already deleted, waiting to be emptied",
        }
    }

    /// Classifies a directory by its name and the names of its siblings.
    /// `has_sibling` lets ambiguous names (`target`, `build`) be confirmed by
    /// the manifest that produced them.
    pub fn detect(name: &str, has_sibling: impl Fn(&str) -> bool) -> ReclaimHint {
        use ReclaimHint::*;
        match name {
            "node_modules" | ".venv" | "venv" | "Pods" | "bower_components" | ".pnpm-store" | ".yarn" => Dependencies,
            "DerivedData" | ".next" | ".nuxt" | ".turbo" | ".parcel-cache" | ".svelte-kit" | ".gradle" | "__pycache__" | ".pytest_cache" | ".mypy_cache" | ".ruff_cache"
            | ".tox" | "CMakeFiles" | ".zig-cache" | "zig-cache" => BuildArtifacts,
            "target" if has_sibling("Cargo.toml") => BuildArtifacts,
            "build" if has_sibling("build.gradle") || has_sibling("build.gradle.kts") || has_sibling("CMakeLists.txt") || has_sibling("package.json") => BuildArtifacts,
            "dist" if has_sibling("package.json") => BuildArtifacts,
            ".build" if has_sibling("Package.swift") => BuildArtifacts,
            "Caches" | ".cache" | ".npm" | ".cargo-cache" | "CachedData" | "Code Cache" | "GPUCache" | "ShaderCache" => Cache,
            ".Trash" | ".Trashes" => Trash,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_names_need_a_manifest() {
        assert_eq!(ReclaimHint::detect("target", |_| false), ReclaimHint::None);
        assert_eq!(ReclaimHint::detect("target", |s| s == "Cargo.toml"), ReclaimHint::BuildArtifacts);
        assert_eq!(ReclaimHint::detect("node_modules", |_| false), ReclaimHint::Dependencies);
    }
}
