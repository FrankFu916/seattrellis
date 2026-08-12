//! SeatTrellis I/O layer (M1-02).
//!
//! File- and artifact-oriented domain modules split out of the app crate:
//! roster uploads/CSV, project bundles, v1→v2 migration, rotation plans and
//! the journaled multi-file transaction helper. No HTTP types.

pub mod export_defaults;
pub mod migration;
pub mod projects;
pub mod roster;
pub mod rotation;
pub mod sample_roster;
pub mod transaction;

#[cfg(test)]
mod rollback_faults;
