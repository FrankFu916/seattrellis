//! SeatTrellis domain layer (M1-02).
//!
//! Editor state machine ([`editing`]), layout drafts ([`layouts`]), room
//! template grids ([`room_templates`]) and goal-rule definitions
//! ([`goal_rules`]). Split out of the app crate so transport/application stay
//! thin and the domain has no HTTP types.

pub mod editing;
pub mod goal_rules;
pub mod layouts;
pub mod room_templates;
