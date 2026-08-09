//! seattrellis-rules: the RuleSpec metadata registry (M3-01).
//!
//! The single source of truth for every official rule: stable IDs, parameter
//! schemas, defaults, bilingual i18n keys and objective semantics. The React
//! UI renders rule controls from [`rule_registry_json`] (generated, drift-
//! checked) and never hard-codes rule lists (M6-02).

pub mod params;
pub mod spec;

pub use spec::{
    rule_registry_json, rule_spec, rule_specs, ExplanationCode, LocalizedKeys, ObjectiveMeta,
    RuleCategory, RuleSpec,
};
