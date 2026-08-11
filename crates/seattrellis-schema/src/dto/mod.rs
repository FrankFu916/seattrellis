//! Typed v2 DTOs, one module per artifact kind (M2-01).
//!
//! Field shapes mirror the Python v1 models (schemas/*.schema.json) so a
//! v1→v2 migration step can be a field-preserving transform (M2-03). All
//! DTOs parse strictly: unknown fields are rejected, never ignored.

pub mod bundle_manifest;
pub mod candidate_set;
pub mod classroom_layout;
pub mod plan_comparison;
pub mod project;
pub mod rule_set;
pub mod snapshot;
pub mod student_roster;
