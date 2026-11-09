//! Application layer: use cases orchestrating the domain through ports.
//!
//! * [`scan`] – parallel scan feeding the Inventory aggregate
//! * [`reclaim`] – committing a ReclaimPlan through the Trash port
//! * [`cartography`] – Voronoi map layout as a background service
//! * [`workspace`] – façade the presentation layer talks to

pub mod cartography;
pub mod ports;
pub mod reclaim;
pub mod scan;
pub mod workspace;

pub use workspace::{CommitState, Services, Workspace};
