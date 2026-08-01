//! Self-contained Rust port of the SeatTrellis cost functions.
//!
//! Ported from `src/seattrellis/solver/backend_common.py` and
//! `src/seattrellis/history.py`:
//!
//! * [`individual_cost`] — per student-seat cost contribution (vision-front,
//!   height-back, randomize, fair-rotation).
//! * [`avoid_recent_neighbors_cost`] — pair-repetition penalty from history.
//!
//! Values are integers, exactly like the Python functions. The only behavior
//! that differs is the RNG: Python's stateful `random.Random` is replaced by a
//! fixed-seed [`SplitMix64`](crate::rng::SplitMix64), so outputs are
//! deterministic across runs.

use crate::models::*;
use crate::rng::RandInt;
use std::collections::{HashMap, HashSet};

/// `_VISION_NEED_KEYWORDS` from `models/student.py`.
pub const VISION_NEED_KEYWORDS: [&str; 11] = [
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

/// `student_needs_front` from `models/student.py`.
///
/// A student needs the front if their (parseable) vision value is < 1.0, or —
/// when the vision value is not numeric — if any vision/tag/need keyword is a
/// known vision marker.
pub fn student_needs_front(student: &Student) -> bool {
    let mut values: Vec<String> = student
        .tags
        .iter()
        .chain(student.needs.iter())
        .map(|item| item.to_lowercase())
        .collect();
    if let Some(vision) = &student.vision {
        let lowered = vision.to_lowercase();
        values.push(lowered.clone());
        if let Ok(number) = lowered.parse::<f64>() {
            return number < 1.0;
        }
    }
    values
        .iter()
        .any(|v| VISION_NEED_KEYWORDS.contains(&v.as_str()))
}

/// Python `round()` (banker's rounding, ties-to-even) to the nearest integer.
fn round_half_even(x: f64) -> i64 {
    let floor = x.floor();
    let diff = x - floor;
    if diff < 0.5 {
        floor as i64
    } else if diff > 0.5 {
        (floor + 1.0) as i64
    } else {
        let f = floor as i64;
        if f % 2 == 0 {
            f
        } else {
            f + 1
        }
    }
}

/// `individual_cost` from `solver/backend_common.py`.
///
/// Cost contribution for one student-seat assignment:
/// 1. `vision_front`: students who need the front pay
///    `weight * (row - min_row) * 100`.
/// 2. `height_back`: tall students pay `weight * round(height) * (max_row - row)`
///    (banker's rounding, as Python).
/// 3. `randomize`: `weight * rng.randint(0, 100)`.
/// 4. `fair_rotation_cost` (may be negative — the compensation bonus can exceed
///    the penalties).
#[allow(clippy::too_many_arguments)]
pub fn individual_cost(
    student: &Student,
    seat: &Seat,
    layout: &Layout,
    rules: &RuleSet,
    history: Option<&SeatHistory>,
    rng: &mut dyn RandInt,
    min_row: i32,
    max_row: i32,
) -> i64 {
    let mut cost: i64 = 0;
    if rules.soft.vision_front.enabled && student_needs_front(student) {
        cost += i64::from(rules.soft.vision_front.weight)
            * i64::from(seat.row - min_row)
            * 100;
    }
    if rules.soft.height_back.enabled {
        if let Some(height) = student.height_cm {
            let front_penalty = max_row - seat.row;
            cost += i64::from(rules.soft.height_back.weight)
                * round_half_even(height)
                * i64::from(front_penalty);
        }
    }
    if rules.soft.randomize.enabled {
        cost += i64::from(rules.soft.randomize.weight) * i64::from(rng.randint(0, 100));
    }
    cost += fair_rotation_cost(student, seat, layout, &rules.soft.fair_rotation, history);
    cost
}

/// `fair_rotation_cost` from `solver/backend_common.py` / `history.py`.
///
/// Penalizes repeating seat-position categories (front/back/side/…): a
/// per-category sum of a recent-repeat penalty, a long-term imbalance penalty,
/// minus a compensation bonus for being the least-frequently assigned student in
/// that category (which may make the total negative).
pub fn fair_rotation_cost(
    student: &Student,
    seat: &Seat,
    layout: &Layout,
    rule: &FairRotationRule,
    history: Option<&SeatHistory>,
) -> i64 {
    if !rule.enabled || rule.weight == 0 {
        return 0;
    }
    let Some(hist) = history else {
        return 0;
    };
    if hist.history_count == 0 {
        return 0;
    }
    let Some(student_history) = hist.students.get(&student.key) else {
        return 0;
    };

    let categories = classify_seat_position(seat, layout);
    let avoid_categories: HashSet<&str> = rule
        .avoid_repeating_categories
        .iter()
        .map(|s| s.as_str())
        .collect();
    let candidate_categories: Vec<String> = categories
        .iter()
        .filter(|c| avoid_categories.contains(c.as_str()))
        .cloned()
        .collect();
    if candidate_categories.is_empty() {
        return 0;
    }

    let recent_counts = student_history.recent_category_counts(rule.lookback);
    let mut total_cost: i64 = 0;
    for category in &candidate_categories {
        let total_count = student_history
            .category_counts
            .get(category)
            .copied()
            .unwrap_or(0);
        let min_count = hist
            .students
            .values()
            .map(|sh| sh.category_counts.get(category).copied().unwrap_or(0))
            .min()
            .unwrap_or(0);
        let repeated_recent_penalty = i64::from(
            recent_counts.get(category).copied().unwrap_or(0) * 100,
        );
        let long_term_penalty = i64::from(0.max(total_count - min_count) * 25);
        let compensation_bonus = if total_count == min_count { 10 } else { 0 };
        total_cost += repeated_recent_penalty + long_term_penalty - compensation_bonus;
    }
    i64::from(rule.weight) * total_cost
}

/// `classify_seat_position` from `history.py`: the set of rotation categories a
/// seat belongs to.
pub fn classify_seat_position(seat: &Seat, layout: &Layout) -> HashSet<String> {
    let mut categories: HashSet<String> = HashSet::new();
    if !seat.enabled {
        return categories;
    }
    let enabled_seats = layout.enabled_seats();
    if enabled_seats.is_empty() {
        return categories;
    }
    let mut rows: Vec<i32> = enabled_seats.iter().map(|s| s.row).collect();
    rows.sort_unstable();
    rows.dedup();
    let mut cols: Vec<i32> = enabled_seats.iter().map(|s| s.col).collect();
    cols.sort_unstable();
    cols.dedup();
    let min_row = rows[0];
    let max_row = rows[rows.len() - 1];
    let min_col = cols[0];
    let max_col = cols[cols.len() - 1];

    let zone_category = zone_category(seat.zone.as_deref());
    if let Some(zc) = &zone_category {
        categories.insert(zc.clone());
    }
    if !matches!(
        zone_category.as_deref(),
        Some("front") | Some("back") | Some("middle")
    ) {
        categories.insert(inferred_row_category(seat, &rows, min_row, max_row));
    }
    if seat.col == min_col || seat.col == max_col {
        categories.insert("side".to_string());
    }
    if (seat.row == min_row || seat.row == max_row)
        && (seat.col == min_col || seat.col == max_col)
    {
        categories.insert("corner".to_string());
    }
    if seat.near_window {
        categories.insert("near_window".to_string());
    }
    if seat.near_door {
        categories.insert("near_door".to_string());
    }
    if seat.near_platform {
        categories.insert("near_platform".to_string());
    }
    if seat.near_ac {
        categories.insert("near_ac".to_string());
    }
    categories
}

/// `_zone_category` from `history.py`: maps a zone string to a rotation category,
/// normalizing dashes/spaces to underscores and matching case-insensitively.
fn zone_category(zone: Option<&str>) -> Option<String> {
    let zone = zone?;
    let normalized = zone.trim().to_lowercase().replace(['-', ' '], "_");
    const CATEGORIES: [&str; 9] = [
        "front",
        "back",
        "middle",
        "side",
        "corner",
        "near_window",
        "near_door",
        "near_platform",
        "near_ac",
    ];
    CATEGORIES
        .iter()
        .find(|c| **c == normalized)
        .map(|c| c.to_string())
}

/// `_inferred_row_category` from `history.py`.
fn inferred_row_category(seat: &Seat, rows: &[i32], min_row: i32, max_row: i32) -> String {
    if rows.len() == 1 {
        return "middle".to_string();
    }
    if seat.row == min_row {
        return "front".to_string();
    }
    if seat.row == max_row {
        return "back".to_string();
    }
    "middle".to_string()
}

/// `avoid_recent_neighbors_cost` from `history.py`.
///
/// Penalizes seating two students as neighbors when they have already been
/// neighbors too often in recent history. Returns 0 unless the current seats
/// exhibit a selected relation, the pair has history, and the recent occurrence
/// count exceeds `max_recent_count`.
#[allow(clippy::too_many_arguments)]
pub fn avoid_recent_neighbors_cost(
    first_student_key: &str,
    second_student_key: &str,
    first_seat: &Seat,
    second_seat: &Seat,
    layout: &Layout,
    rule: &AvoidRecentNeighborsRule,
    pair_history: Option<&PairHistory>,
    adjacency_edges: Option<&HashSet<(String, String)>>,
) -> i64 {
    if !rule.enabled || rule.weight == 0 {
        return 0;
    }
    let Some(pair_history) = pair_history else {
        return 0;
    };
    if pair_history.history_count == 0 {
        return 0;
    }

    let selected_relations: HashSet<String> = rule.relation_types.iter().cloned().collect();
    let current_relations = detect_neighbor_relation_types(
        first_seat,
        second_seat,
        layout,
        adjacency_edges,
        rule.within_distance,
    );
    if !current_relations
        .iter()
        .any(|r| selected_relations.contains(r))
    {
        return 0;
    }

    let pair = pair_history.pairs.get(&student_pair_key(first_student_key, second_student_key));
    let Some(pair) = pair else {
        return 0;
    };
    let recent_count = pair.recent_occurrence_count(&selected_relations, rule.lookback);
    let excess = 0.max(recent_count - rule.max_recent_count);
    i64::from(rule.weight) * i64::from(excess) * 100
}

/// `detect_neighbor_relation_types` from `history.py`.
///
/// Classifies the relation between two seats using their row/col deltas, plus
/// the derived adjacency graph when the seats are not trivially adjacent.
pub fn detect_neighbor_relation_types(
    first_seat: &Seat,
    second_seat: &Seat,
    layout: &Layout,
    adjacency_edges: Option<&HashSet<(String, String)>>,
    within_distance: i32,
) -> HashSet<String> {
    let mut relations: HashSet<String> = HashSet::new();
    if first_seat.seat_id == second_seat.seat_id {
        return relations;
    }

    let row_delta = (first_seat.row - second_seat.row).abs();
    let col_delta = (first_seat.col - second_seat.col).abs();

    if row_delta == 0 && col_delta == 1 {
        relations.insert("horizontal".to_string());
        relations.insert("desk_mate".to_string());
    }
    if col_delta == 0 && row_delta == 1 {
        relations.insert("vertical".to_string());
    }
    if row_delta == 1 && col_delta == 1 {
        relations.insert("diagonal".to_string());
    }

    let has_basic = relations.contains("horizontal")
        || relations.contains("vertical")
        || relations.contains("diagonal");
    if has_basic {
        relations.insert("adjacent_any".to_string());
    } else {
        let edges = match adjacency_edges {
            Some(edges) => edges.clone(),
            None => build_adjacency_edges(layout),
        };
        if edges.contains(&normalize_edge(&first_seat.seat_id, &second_seat.seat_id)) {
            relations.insert("adjacent_any".to_string());
        }
    }

    if row_delta.max(col_delta) <= within_distance {
        relations.insert("within_distance".to_string());
    }
    relations
}

/// `normalize_edge` from `solver/adjacency.py`: canonical undirected edge.
pub fn normalize_edge(first: &str, second: &str) -> (String, String) {
    if first <= second {
        (first.to_string(), second.to_string())
    } else {
        (second.to_string(), first.to_string())
    }
}

/// `build_adjacency_edges` from `solver/adjacency.py`: derived adjacency graph
/// over enabled seats (including custom edges).
pub fn build_adjacency_edges(layout: &Layout) -> HashSet<(String, String)> {
    let config = &layout.adjacency;
    let enabled: HashMap<&str, &Seat> = layout
        .seats
        .iter()
        .filter(|s| s.enabled)
        .map(|s| (s.seat_id.as_str(), s))
        .collect();
    let seats: Vec<&Seat> = layout.seats.iter().filter(|s| s.enabled).collect();

    let mut edges: HashSet<(String, String)> = HashSet::new();
    for i in 0..seats.len() {
        for second in seats.iter().skip(i + 1) {
            if are_adjacent(seats[i], second, config) {
                edges.insert(normalize_edge(&seats[i].seat_id, &second.seat_id));
            }
        }
    }
    for (a, b) in &config.custom_edges {
        if enabled.contains_key(a.as_str())
            && enabled.contains_key(b.as_str())
            && a != b
        {
            edges.insert(normalize_edge(a, b));
        }
    }
    edges
}

/// `_are_adjacent` from `solver/adjacency.py`.
fn are_adjacent(first: &Seat, second: &Seat, config: &AdjacencyConfig) -> bool {
    if let Some(max_distance) = config.max_distance {
        let distance = if config.use_xy_distance {
            let dx = first.x_default() - second.x_default();
            let dy = first.y_default() - second.y_default();
            (dx * dx + dy * dy).sqrt()
        } else {
            let dr = (first.row - second.row) as f64;
            let dc = (first.col - second.col) as f64;
            (dr * dr + dc * dc).sqrt()
        };
        return distance <= max_distance;
    }

    let row_delta = (first.row - second.row).abs();
    let col_delta = (first.col - second.col).abs();
    if row_delta == 0 && 0 < col_delta && col_delta <= config.max_col_delta {
        return config.include_horizontal;
    }
    if col_delta == 0 && 0 < row_delta && row_delta <= config.max_row_delta {
        return config.include_vertical;
    }
    if row_delta != 0 && col_delta != 0 {
        return config.include_diagonal
            && row_delta <= config.max_row_delta
            && col_delta <= config.max_col_delta;
    }
    false
}

/// `student_pair_key` from `history.py`: sorted-key `"first|second"` pair key.
pub fn student_pair_key(first_student_key: &str, second_student_key: &str) -> String {
    let (a, b) = if first_student_key <= second_student_key {
        (first_student_key, second_student_key)
    } else {
        (second_student_key, first_student_key)
    };
    format!("{a}|{b}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;

    #[test]
    fn student_needs_front_keyword_and_numeric() {
        let mut s = Student {
            key: "S1".into(),
            display_name: None,
            height_cm: None,
            score: None,
            vision: Some("0.8".into()),
            tags: vec![],
            needs: vec![],
        };
        assert!(student_needs_front(&s)); // 0.8 < 1.0
        s.vision = Some("2.0".into());
        assert!(!student_needs_front(&s)); // 2.0 >= 1.0
        s.vision = Some("poor".into());
        assert!(student_needs_front(&s)); // keyword
        s.vision = Some("good".into());
        assert!(!student_needs_front(&s)); // unknown keyword
        s.vision = None;
        s.tags = vec!["myopia".into()];
        assert!(student_needs_front(&s)); // tag keyword
        s.tags = vec!["near_window".into()];
        assert!(!student_needs_front(&s)); // non-vision tag
    }

    #[test]
    fn round_half_even_matches_python() {
        assert_eq!(round_half_even(165.5), 166);
        assert_eq!(round_half_even(166.5), 166);
        assert_eq!(round_half_even(164.5), 164);
        assert_eq!(round_half_even(182.4), 182);
        assert_eq!(round_half_even(170.0), 170);
        assert_eq!(round_half_even(150.0), 150);
    }

    #[test]
    fn individual_cost_with_all_rules_disabled_is_zero() {
        let layout = Layout {
            layout_id: "t".into(),
            name: "T".into(),
            seats: vec![Seat {
                seat_id: "S1".into(),
                row: 1,
                col: 1,
                x: Some(1.0),
                y: Some(1.0),
                enabled: true,
                zone: None,
                group_id: None,
                near_window: false,
                near_door: false,
                near_platform: false,
                near_ac: false,
            }],
            adjacency: AdjacencyConfig::default(),
        };
        let student = Student {
            key: "A".into(),
            display_name: None,
            height_cm: Some(180.0),
            score: None,
            vision: Some("0.5".into()),
            tags: vec![],
            needs: vec![],
        };
        let mut rules = RuleSet::default();
        rules.soft.vision_front.enabled = false;
        rules.soft.height_back.enabled = false;
        rules.soft.randomize.enabled = false;
        let mut rng = SplitMix64::new(0);
        assert_eq!(
            individual_cost(&student, &layout.seats[0], &layout, &rules, None, &mut rng, 1, 1),
            0
        );
    }
}
