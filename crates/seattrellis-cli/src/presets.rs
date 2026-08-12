//! Preset catalog mirror (oracle `presets.py`) for the `validate` command's
//! "preset context warnings": the v1 CLI warns when a preset's preferred data
//! (history / score / height / vision) is missing from the current inputs.
//!
//! The catalog below mirrors `presets.py::_PRESETS` requirements and
//! `_DEGRADATION_NOTES` so the Rust CLI can reproduce those warnings for a
//! problem JSON without re-reading the Python source. `preset_requirements`
//! returns `None` for unknown names so the caller can report an error, and
//! the mirror is locked by `catalog_mirrors_oracle` in `main.rs` tests.

use seattrellis_core::models::{SoftRules, Student};

/// The four preferred-data requirement kinds (`presets.py` requirements).
/// The catalog mirror test locks every preset requirement against this
/// universe; kept `pub` for that contract.
#[cfg_attr(not(test), allow(dead_code))]
pub const REQUIREMENTS: [&str; 4] = ["history", "score", "height", "vision"];

/// `_DEGRADATION_NOTES` from `presets.py`.
pub const DEGRADATION_NOTES: [(&str, &str); 4] = [
    (
        "history",
        "History-based preferences stay enabled but contribute no cost or score until snapshots are supplied.",
    ),
    (
        "score",
        "Score-based goals stay enabled but are ignored when fewer than two students have distinct scores.",
    ),
    (
        "height",
        "height_back stays enabled but is ignored when usable height or row variation is unavailable.",
    ),
    (
        "vision",
        "vision_front stays enabled but is ignored when no student has a vision/front-seat marker.",
    ),
];

/// The `_has_vision_marker` keyword set from `presets.py`.
pub const VISION_KEYWORDS: [&str; 11] = [
    "vision",
    "vision_front",
    "front",
    "poor",
    "low",
    "nearsighted",
    "short_sighted",
    "myopia",
    "视力",
    "近视",
    "靠前",
];

/// `_PRESETS` requirements mirror: preset name -> preferred-data requirements.
/// `random` and `exam` activate no data-dependent preferences (empty list).
/// Unknown names return `None` (the Python CLI reports "Unknown preset").
pub fn preset_requirements(name: &str) -> Option<&'static [&'static str]> {
    let normalized = name.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "daily" => Some(&["history", "score", "height", "vision"]),
        "fair-rotation" => Some(&["history"]),
        "neighbor-aware" => Some(&["history"]),
        "balanced" | "peer-mixing" => Some(&["score"]),
        "score-high-front"
        | "score-high-back"
        | "row-score-balanced"
        | "group-score-balanced"
        | "mentor-pairing" => Some(&["score"]),
        "height-aware" => Some(&["height"]),
        "vision-friendly" => Some(&["vision"]),
        "random" | "exam" => Some(&[]),
        _ => None,
    }
}

/// `_requirement_enabled` from `presets.py`: the requirement only counts when
/// the soft rules that consume it are actually active.
pub fn requirement_enabled(requirement: &str, rules: &SoftRules) -> bool {
    match requirement {
        "history" => {
            (rules.fair_rotation.enabled && rules.fair_rotation.weight > 0)
                || (rules.avoid_recent_neighbors.enabled && rules.avoid_recent_neighbors.weight > 0)
        }
        "score" => {
            (rules.score_balance.enabled && rules.score_balance.weight > 0)
                || (rules.score_position.enabled && rules.score_position.weight > 0)
                || (rules.score_distribution.enabled && rules.score_distribution.weight > 0)
                || (rules.mentor_pairing.enabled && rules.mentor_pairing.weight > 0)
        }
        "height" => rules.height_back.enabled && rules.height_back.weight > 0,
        "vision" => rules.vision_front.enabled && rules.vision_front.weight > 0,
        _ => false,
    }
}

/// `_requirement_available` from `presets.py`: does the current input carry
/// the data the requirement needs?
pub fn requirement_available(
    requirement: &str,
    students: &[Student],
    history_count: usize,
) -> bool {
    match requirement {
        "history" => history_count > 0,
        "score" => {
            let values: std::collections::HashSet<u64> = students
                .iter()
                .filter_map(|student| student.score)
                .map(|score| score.to_bits())
                .collect();
            values.len() >= 2
        }
        "height" => {
            let values: std::collections::HashSet<u64> = students
                .iter()
                .filter_map(|student| student.height_cm)
                .map(|height| height.to_bits())
                .collect();
            values.len() >= 2
        }
        "vision" => students.iter().any(has_vision_marker),
        _ => false,
    }
}

/// `_has_vision_marker` from `presets.py`: a vision value or a tag/need
/// keyword marks the student for the front-seat preference.
fn has_vision_marker(student: &Student) -> bool {
    if student.vision.is_some() {
        return true;
    }
    let keywords: std::collections::HashSet<&str> = VISION_KEYWORDS.iter().copied().collect();
    student.tags.iter().chain(student.needs.iter()).any(|item| {
        let lowered = item.to_lowercase();
        keywords.contains(lowered.as_str())
    })
}

/// `preset_context_warnings` from `presets.py`. The message text matches the
/// oracle byte-for-byte so the CLI warning output is directly comparable.
pub fn preset_context_warnings(
    preset_name: &str,
    students: &[Student],
    rules: &SoftRules,
    history_count: usize,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let Some(requirements) = preset_requirements(preset_name) else {
        return warnings;
    };
    for requirement in requirements {
        if !requirement_enabled(requirement, rules) {
            continue;
        }
        if !requirement_available(requirement, students, history_count) {
            let note = DEGRADATION_NOTES
                .iter()
                .find(|(name, _)| *name == *requirement)
                .map(|(_, note)| *note)
                .unwrap_or("");
            warnings.push(format!(
                "Preset \"{preset_name}\" is missing preferred {requirement} data. {note}"
            ));
        }
    }
    warnings
}
