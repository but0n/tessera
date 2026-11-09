//! Privacy-protected locations (macOS TCC).
//!
//! Opening these without Full Disk Access blocks the calling thread on a
//! consent prompt, or fails. A scan therefore *seals* them instead of
//! reading them unless access was granted up front (ADR-0009).

use std::path::{Path, PathBuf};

/// Paths relative to the user's home directory.
pub const PROTECTED_IN_HOME: &[&str] = &[
    "Library/Mail",
    "Library/Messages",
    "Library/Safari",
    "Library/Cookies",
    "Library/HomeKit",
    "Library/IdentityServices",
    "Library/Metadata/CoreSpotlight",
    "Library/Suggestions",
    "Library/PersonalizationPortrait",
    "Library/Calendars",
    "Library/Reminders",
    "Library/Sharing",
    "Library/Accounts",
    "Library/Biome",
    "Library/DuetExpertCenter",
    "Library/Trial",
    "Library/StatusKit",
    "Library/Autosave Information",
    "Library/Containers",
    "Library/Group Containers",
    "Library/Daemon Containers",
    "Library/Application Support/AddressBook",
    "Library/Application Support/CallHistoryDB",
    "Library/Application Support/CallHistoryTransactions",
    "Library/Application Support/com.apple.TCC",
    "Library/Application Support/FaceTime",
    "Library/Application Support/Knowledge",
    "Library/Application Support/com.apple.sharedfilelist",
    "Library/Application Support/MobileSync",
    "Pictures/Photos Library.photoslibrary",
];

pub fn protected_paths(home: &Path) -> Vec<PathBuf> {
    PROTECTED_IN_HOME.iter().map(|p| home.join(p)).collect()
}
