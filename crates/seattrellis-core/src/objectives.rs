//! Port of `seattrellis/solver/soft_objectives.py`:
//! `score_rank_percentiles`, `compile_soft_objectives`,
//! `evaluate_soft_objectives`, `_select_mentor_pairs`, and the Hungarian
//! `_minimum_cost_bipartite_pairs`, plus the adjacency helpers
//! (`build_adjacency_edges`, `normalize_edge`) and `student_pair_key`.

use std::collections::{HashMap, HashSet};

use crate::models::*;

// ---------------------------------------------------------------------------
// Ported data structures
// ---------------------------------------------------------------------------

/// A deterministic mentor/learner pair selected before seat optimization
/// (mirrors `MentorPair`).
#[derive(Debug, Clone, PartialEq)]
pub struct MentorPair {
    pub mentor_key: String,
    pub learner_key: String,
    pub recent_occurrences: usize,
}

/// Precomputed data shared by fallback solving and result scoring
/// (mirrors `SoftObjectiveContext`).
#[derive(Debug, Clone)]
pub struct SoftObjectiveContext {
    pub score_percentiles: HashMap<String, f64>,
    pub seat_row_percentiles: HashMap<String, f64>,
    pub distribution_buckets: HashMap<String, String>,
    pub mentor_pairs: Vec<MentorPair>,
    pub seat_by_id: HashMap<String, Seat>,
    pub adjacency_edges: HashSet<(String, String)>,
    pub warnings: Vec<String>,
}

/// Normalized losses plus comparable weighted costs for enabled goals
/// (mirrors `SoftObjectiveEvaluation`).
#[derive(Debug, Clone, Default)]
pub struct SoftObjectiveEvaluation {
    pub losses: HashMap<String, Option<f64>>,
    pub weighted_costs: HashMap<String, f64>,
    pub details: HashMap<String, serde_json::Value>,
    pub warnings: Vec<String>,
}

impl SoftObjectiveEvaluation {
    pub fn total_cost(&self) -> f64 {
        self.weighted_costs.values().sum()
    }
}

// ---------------------------------------------------------------------------
// score_rank_percentiles
// ---------------------------------------------------------------------------

/// Return average-rank percentiles, preserving ties and grading scales
/// (mirrors `score_rank_percentiles`).
///
/// The lowest distinct score approaches `0` and the highest approaches `1`.
/// Tied students receive the average of their occupied rank positions. A
/// single score value (or none) is not enough to express a preference, so an
/// empty mapping is returned in that case.
pub fn score_rank_percentiles(students: &[Student]) -> HashMap<String, f64> {
    let mut scored: Vec<(&str, f64)> = students
        .iter()
        .filter_map(|student| student.score.map(|score| (student.key.as_str(), score)))
        .collect();
    scored.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    if scored.len() < 2 || scored[0].1 == scored[scored.len() - 1].1 {
        return HashMap::new();
    }
    let denominator = (scored.len() - 1) as f64;
    let mut result: HashMap<String, f64> = HashMap::new();
    let mut start = 0usize;
    while start < scored.len() {
        let mut end = start + 1;
        while end < scored.len() && scored[end].1 == scored[start].1 {
            end += 1;
        }
        let average_rank = (start + end - 1) as f64 / 2.0;
        let percentile = average_rank / denominator;
        for (key, _score) in &scored[start..end] {
            result.insert((*key).to_string(), percentile);
        }
        start = end;
    }
    result
}

// ---------------------------------------------------------------------------
// compile_soft_objectives
// ---------------------------------------------------------------------------

/// Compile score goals once for a solve or scoring operation
/// (mirrors `compile_soft_objectives`).
pub fn compile_soft_objectives(
    students: &[Student],
    layout: &Layout,
    rules: &RuleSet,
    pair_history: Option<&PairHistory>,
) -> SoftObjectiveContext {
    let percentiles = score_rank_percentiles(students);
    let enabled_seats = layout.enabled_seats();

    // Row percentiles: seats sorted by their distinct row numbers.
    let mut rows: Vec<i32> = enabled_seats.iter().map(|seat| seat.row).collect();
    rows.sort_unstable();
    rows.dedup();
    let row_percentile: HashMap<i32, f64> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let value = if rows.len() > 1 {
                index as f64 / (rows.len() - 1) as f64
            } else {
                0.5
            };
            (*row, value)
        })
        .collect();
    let seat_rows: HashMap<String, f64> = enabled_seats
        .iter()
        .map(|seat| (seat.seat_id.clone(), row_percentile[&seat.row]))
        .collect();

    // Distribution buckets.
    let mut distribution_buckets: HashMap<String, String> = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();
    let distribution_rule = &rules.soft.score_distribution;
    match distribution_rule.scope {
        DistributionScope::Row => {
            for seat in &enabled_seats {
                distribution_buckets.insert(seat.seat_id.clone(), format!("row:{}", seat.row));
            }
        }
        DistributionScope::Group => {
            let grouped: Vec<&Seat> = enabled_seats
                .iter()
                .copied()
                .filter(|seat| seat.group_id.is_some())
                .collect();
            for seat in &grouped {
                distribution_buckets.insert(
                    seat.seat_id.clone(),
                    format!("group:{}", seat.group_id.as_ref().unwrap()),
                );
            }
            if distribution_rule.enabled && grouped.len() != enabled_seats.len() {
                let missing_count = enabled_seats.len() - grouped.len();
                warnings.push(format!(
                    "score_distribution with scope='group' requires group_id on every \
                     enabled seat; {missing_count} seat(s) are missing it, so the group \
                     distribution objective is unavailable."
                ));
                distribution_buckets.clear();
            }
        }
    }

    let mentor_pairs = select_mentor_pairs(&percentiles, rules, pair_history);
    let seat_by_id: HashMap<String, Seat> = enabled_seats
        .iter()
        .map(|seat| (seat.seat_id.clone(), (*seat).clone()))
        .collect();
    let adjacency_edges = build_adjacency_edges(layout);

    SoftObjectiveContext {
        score_percentiles: percentiles,
        seat_row_percentiles: seat_rows,
        distribution_buckets,
        mentor_pairs,
        seat_by_id,
        adjacency_edges,
        warnings,
    }
}

// ---------------------------------------------------------------------------
// evaluate_soft_objectives
// ---------------------------------------------------------------------------

/// Evaluate enabled score goals for a partial or complete assignment
/// (mirrors `evaluate_soft_objectives`).
pub fn evaluate_soft_objectives(
    assignment: &HashMap<String, String>,
    context: &SoftObjectiveContext,
    rules: &RuleSet,
) -> SoftObjectiveEvaluation {
    let mut losses: HashMap<String, Option<f64>> = HashMap::new();
    let mut weighted_costs: HashMap<String, f64> = HashMap::new();
    let mut details: HashMap<String, serde_json::Value> = HashMap::new();

    // --- score_position ----------------------------------------------------
    let position_rule = &rules.soft.score_position;
    if position_rule.enabled && position_rule.weight != 0 {
        let mut errors: Vec<f64> = Vec::new();
        for (student_key, seat_id) in assignment {
            let score_position = context.score_percentiles.get(student_key);
            let row_position = context.seat_row_percentiles.get(seat_id);
            if let (Some(score_position), Some(row_position)) = (score_position, row_position) {
                let target = match position_rule.direction {
                    ScoreDirection::HighBack => *score_position,
                    ScoreDirection::HighFront => 1.0 - *score_position,
                };
                errors.push((target - *row_position).abs());
            }
        }
        let loss = if !errors.is_empty() && !context.score_percentiles.is_empty() {
            Some(mean(&errors))
        } else {
            None
        };
        losses.insert("score_position".to_string(), loss);
        details.insert(
            "score_position".to_string(),
            serde_json::json!({
                "direction": match position_rule.direction {
                    ScoreDirection::HighFront => "high_front",
                    ScoreDirection::HighBack => "high_back",
                },
                "evaluated_students": errors.len(),
                "mean_percentile_error": loss,
                "lower_error_is_better": true,
            }),
        );
        add_weighted_cost(
            &mut weighted_costs,
            "score_position",
            loss,
            position_rule.weight,
        );
    }

    // --- score_distribution ------------------------------------------------
    let distribution_rule = &rules.soft.score_distribution;
    if distribution_rule.enabled && distribution_rule.weight != 0 {
        let mut bucket_values: HashMap<String, Vec<f64>> = HashMap::new();
        for (student_key, seat_id) in assignment {
            let percentile = context.score_percentiles.get(student_key);
            let bucket = context.distribution_buckets.get(seat_id);
            if let (Some(percentile), Some(bucket)) = (percentile, bucket) {
                bucket_values
                    .entry(bucket.clone())
                    .or_default()
                    .push(*percentile);
            }
        }
        let usable: Vec<&Vec<f64>> = bucket_values
            .values()
            .filter(|values| !values.is_empty())
            .collect();
        let (loss, overall, rms) = if usable.len() >= 2 {
            let mut all: Vec<f64> = Vec::new();
            for values in &usable {
                all.extend(values.iter().copied());
            }
            let overall = mean(&all);
            let mut squared = 0.0f64;
            for values in &usable {
                let bucket_mean = mean(values);
                squared += (bucket_mean - overall).powi(2);
            }
            let rms = (squared / usable.len() as f64).sqrt();
            // With percentile data the largest practical between-bucket RMS is
            // 0.5; scaling by two maps that range to a readable 0..1 loss.
            let loss = (rms * 2.0).min(1.0);
            (Some(loss), Some(overall), Some(rms))
        } else {
            (None, None, None)
        };
        losses.insert("score_distribution".to_string(), loss);
        let mut bucket_sizes: Vec<(String, usize)> = bucket_values
            .iter()
            .map(|(key, values)| (key.clone(), values.len()))
            .collect();
        bucket_sizes.sort();
        let bucket_sizes_map: serde_json::Map<String, serde_json::Value> = bucket_sizes
            .into_iter()
            .map(|(key, size)| (key, serde_json::Value::from(size as u64)))
            .collect();
        details.insert(
            "score_distribution".to_string(),
            serde_json::json!({
                "scope": match distribution_rule.scope {
                    DistributionScope::Row => "row",
                    DistributionScope::Group => "group",
                },
                "bucket_count": usable.len(),
                "bucket_sizes": serde_json::Value::Object(bucket_sizes_map),
                "overall_mean_percentile": overall,
                "between_bucket_rms": rms,
                "lower_error_is_better": true,
            }),
        );
        add_weighted_cost(
            &mut weighted_costs,
            "score_distribution",
            loss,
            distribution_rule.weight,
        );
    }

    // --- mentor_pairing ----------------------------------------------------
    let mentor_rule = &rules.soft.mentor_pairing;
    if mentor_rule.enabled && mentor_rule.weight != 0 {
        let mut evaluated = 0usize;
        let mut satisfied = 0usize;
        let mut pair_details: Vec<serde_json::Value> = Vec::new();
        for pair in &context.mentor_pairs {
            let mentor_seat_id = assignment.get(&pair.mentor_key);
            let learner_seat_id = assignment.get(&pair.learner_key);
            if mentor_seat_id.is_none() || learner_seat_id.is_none() {
                continue;
            }
            evaluated += 1;
            let is_satisfied = relation_satisfied(
                mentor_seat_id.unwrap(),
                learner_seat_id.unwrap(),
                mentor_rule.relation,
                context,
            );
            satisfied += usize::from(is_satisfied);
            pair_details.push(serde_json::json!({
                "mentor": pair.mentor_key,
                "learner": pair.learner_key,
                "satisfied": is_satisfied,
                "recent_occurrences": pair.recent_occurrences,
            }));
        }
        let loss = if evaluated != 0 {
            Some(1.0 - satisfied as f64 / evaluated as f64)
        } else {
            None
        };
        losses.insert("mentor_pairing".to_string(), loss);
        details.insert(
            "mentor_pairing".to_string(),
            serde_json::json!({
                "relation": match mentor_rule.relation {
                    PairRelation::DeskMate => "desk_mate",
                    PairRelation::AdjacentAny => "adjacent_any",
                },
                "selected_pair_count": context.mentor_pairs.len(),
                "evaluated_pair_count": evaluated,
                "satisfied_pair_count": satisfied,
                "pairs": pair_details,
            }),
        );
        add_weighted_cost(
            &mut weighted_costs,
            "mentor_pairing",
            loss,
            mentor_rule.weight,
        );
    }

    SoftObjectiveEvaluation {
        losses,
        weighted_costs,
        details,
        warnings: context.warnings.clone(),
    }
}

fn add_weighted_cost(costs: &mut HashMap<String, f64>, name: &str, loss: Option<f64>, weight: i32) {
    if let Some(loss) = loss {
        costs.insert(name.to_string(), loss * weight as f64 * 100.0);
    }
}

// ---------------------------------------------------------------------------
// _select_mentor_pairs / _minimum_cost_bipartite_pairs
// ---------------------------------------------------------------------------

/// Select a deterministic minimum-cost set of mentor/learner pairs
/// (mirrors `_select_mentor_pairs`).
pub fn select_mentor_pairs(
    percentiles: &HashMap<String, f64>,
    rules: &RuleSet,
    pair_history: Option<&PairHistory>,
) -> Vec<MentorPair> {
    let rule = &rules.soft.mentor_pairing;
    if !rule.enabled || rule.weight == 0 || percentiles.is_empty() {
        return Vec::new();
    }

    let mut mentors: Vec<String> = percentiles
        .iter()
        .filter(|(_key, value)| **value >= rule.mentor_percentile)
        .map(|(key, _value)| key.clone())
        .collect();
    mentors.sort_by(|a, b| {
        percentiles[b]
            .partial_cmp(&percentiles[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });

    let mut learners: Vec<String> = percentiles
        .iter()
        .filter(|(_key, value)| **value <= rule.learner_percentile)
        .map(|(key, _value)| key.clone())
        .collect();
    learners.sort_by(|a, b| {
        percentiles[a]
            .partial_cmp(&percentiles[b])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });

    if mentors.is_empty() || learners.is_empty() {
        return Vec::new();
    }

    let mut occurrence_by_pair: HashMap<(String, String), usize> = HashMap::new();
    let mut costs: HashMap<(String, String), i64> = HashMap::new();
    for (mentor_index, mentor_key) in mentors.iter().enumerate() {
        for (learner_index, learner_key) in learners.iter().enumerate() {
            let occurrences = if rule.avoid_recent_repeats {
                recent_pair_occurrences(
                    mentor_key,
                    learner_key,
                    rule.relation,
                    rule.history_lookback,
                    pair_history,
                )
            } else {
                0
            };
            occurrence_by_pair.insert((mentor_key.clone(), learner_key.clone()), occurrences);
            let complement_error = (percentiles[mentor_key] + percentiles[learner_key] - 1.0).abs();
            // Occurrence count dominates rank complement, which dominates the
            // stable key order. The global assignment avoids greedy dead ends.
            let cost = occurrences as i64 * 1_000_000
                + python_round(complement_error * 10_000.0) * 100
                + mentor_index as i64 * learners.len() as i64
                + learner_index as i64;
            costs.insert((mentor_key.clone(), learner_key.clone()), cost);
        }
    }

    let selected = minimum_cost_bipartite_pairs(&mentors, &learners, &costs);
    let mut pairs: Vec<MentorPair> = selected
        .into_iter()
        .map(|(mentor_key, learner_key)| MentorPair {
            mentor_key: mentor_key.clone(),
            learner_key: learner_key.clone(),
            recent_occurrences: occurrence_by_pair[&(mentor_key, learner_key)],
        })
        .collect();
    pairs.sort_by(|a, b| {
        a.mentor_key
            .cmp(&b.mentor_key)
            .then_with(|| a.learner_key.cmp(&b.learner_key))
    });
    pairs
}

/// Return a deterministic minimum-cost matching using the Hungarian method
/// (mirrors `_minimum_cost_bipartite_pairs`).
pub fn minimum_cost_bipartite_pairs(
    mentors: &[String],
    learners: &[String],
    costs: &HashMap<(String, String), i64>,
) -> Vec<(String, String)> {
    if mentors.is_empty() || learners.is_empty() {
        return Vec::new();
    }

    let rows_are_mentors = mentors.len() <= learners.len();
    let rows: Vec<&String> = if rows_are_mentors {
        mentors.iter().collect()
    } else {
        learners.iter().collect()
    };
    let columns: Vec<&String> = if rows_are_mentors {
        learners.iter().collect()
    } else {
        mentors.iter().collect()
    };

    let mut matrix: Vec<Vec<i64>> = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut matrix_row: Vec<i64> = Vec::with_capacity(columns.len());
        for column in &columns {
            let key = if rows_are_mentors {
                ((*row).clone(), (*column).clone())
            } else {
                ((*column).clone(), (*row).clone())
            };
            matrix_row.push(costs[&key]);
        }
        matrix.push(matrix_row);
    }

    // This O(n^2 m) rectangular implementation is small enough for classroom
    // cohorts and avoids introducing a numerical dependency for one objective.
    let row_count = rows.len();
    let column_count = columns.len();
    let mut u = vec![0i64; row_count + 1];
    let mut v = vec![0i64; column_count + 1];
    let mut matched_row = vec![0usize; column_count + 1];
    let mut previous_column = vec![0usize; column_count + 1];
    let infinity = 1_000_000_000_000_000i64;

    for row_index in 1..=row_count {
        matched_row[0] = row_index;
        let mut column0 = 0usize;
        let mut minimum = vec![infinity; column_count + 1];
        let mut used = vec![false; column_count + 1];
        loop {
            used[column0] = true;
            let current_row = matched_row[column0];
            let mut delta = infinity;
            let mut column1 = 0usize;
            for column_index in 1..=column_count {
                if used[column_index] {
                    continue;
                }
                let current =
                    matrix[current_row - 1][column_index - 1] - u[current_row] - v[column_index];
                if current < minimum[column_index] {
                    minimum[column_index] = current;
                    previous_column[column_index] = column0;
                }
                if minimum[column_index] < delta {
                    delta = minimum[column_index];
                    column1 = column_index;
                }
            }
            for column_index in 0..=column_count {
                if used[column_index] {
                    u[matched_row[column_index]] += delta;
                    v[column_index] -= delta;
                } else {
                    minimum[column_index] -= delta;
                }
            }
            column0 = column1;
            if matched_row[column0] == 0 {
                break;
            }
        }
        loop {
            let column1 = previous_column[column0];
            matched_row[column0] = matched_row[column1];
            column0 = column1;
            if column0 == 0 {
                break;
            }
        }
    }

    let mut pairs: Vec<(String, String)> = Vec::new();
    for column_index in 1..=column_count {
        let row_index = matched_row[column_index];
        if row_index == 0 {
            continue;
        }
        let row = rows[row_index - 1];
        let column = columns[column_index - 1];
        if rows_are_mentors {
            pairs.push((row.clone(), column.clone()));
        } else {
            pairs.push((column.clone(), row.clone()));
        }
    }
    pairs
}

fn recent_pair_occurrences(
    first_key: &str,
    second_key: &str,
    relation: PairRelation,
    lookback: i32,
    pair_history: Option<&PairHistory>,
) -> usize {
    let pair_history = match pair_history {
        Some(pair_history) => pair_history,
        None => return 0,
    };
    if pair_history.history_count == 0 || lookback == 0 {
        return 0;
    }
    let key = student_pair_key(first_key, second_key);
    let history = match pair_history.pairs.get(&key) {
        Some(history) => history,
        None => return 0,
    };
    let relation_type = match relation {
        PairRelation::DeskMate => "desk_mate".to_string(),
        PairRelation::AdjacentAny => "adjacent_any".to_string(),
    };
    let mut relation_types = HashSet::new();
    relation_types.insert(relation_type);
    history.recent_occurrence_count(&relation_types, Some(lookback)) as usize
}

// ---------------------------------------------------------------------------
// relation_satisfied + adjacency helpers
// ---------------------------------------------------------------------------

fn relation_satisfied(
    first_seat_id: &str,
    second_seat_id: &str,
    relation: PairRelation,
    context: &SoftObjectiveContext,
) -> bool {
    let first = context.seat_by_id.get(first_seat_id);
    let second = context.seat_by_id.get(second_seat_id);
    let (first, second) = match (first, second) {
        (Some(first), Some(second)) if first.seat_id != second.seat_id => (first, second),
        _ => return false,
    };
    match relation {
        PairRelation::DeskMate => {
            first.row == second.row
                && (i64::from(first.col) - i64::from(second.col)).abs() == 1
        }
        PairRelation::AdjacentAny => {
            let edge = normalize_edge(&first.seat_id, &second.seat_id);
            context.adjacency_edges.contains(&edge)
        }
    }
}

/// `seattrellis.history.student_pair_key`: sorted keys joined by `|`.
pub fn student_pair_key(first: &str, second: &str) -> String {
    let mut keys = [first.to_string(), second.to_string()];
    keys.sort();
    format!("{}|{}", keys[0], keys[1])
}

/// `normalize_edge`: the two seat ids as a sorted tuple.
pub fn normalize_edge(first: &str, second: &str) -> (String, String) {
    let mut keys = [first.to_string(), second.to_string()];
    keys.sort();
    (keys[0].clone(), keys[1].clone())
}

/// Build an undirected adjacency graph for enabled seats
/// (mirrors `build_adjacency_edges`).
pub fn build_adjacency_edges(layout: &Layout) -> HashSet<(String, String)> {
    let config = &layout.adjacency;
    let enabled = layout.enabled_seats();
    let mut edges: HashSet<(String, String)> = HashSet::new();

    for (index, first) in enabled.iter().enumerate() {
        for second in enabled.iter().skip(index + 1) {
            if are_adjacent(first, second, config) {
                edges.insert(normalize_edge(&first.seat_id, &second.seat_id));
            }
        }
    }
    for (first, second) in &config.custom_edges {
        if first != second
            && enabled.iter().any(|seat| seat.seat_id == *first)
            && enabled.iter().any(|seat| seat.seat_id == *second)
        {
            edges.insert(normalize_edge(first, second));
        }
    }
    edges
}

fn are_adjacent(first: &Seat, second: &Seat, config: &AdjacencyConfig) -> bool {
    if let Some(max_distance) = config.max_distance {
        return if config.use_xy_distance {
            seat_distance(first, second) <= max_distance
        } else {
            // Row/col deltas in f64: i32 subtraction of saturated extreme
            // coordinates would overflow (debug panic) and `.pow(2)` on the
            // i32 difference would overflow the square before the cast.
            let row_col_distance = ((first.row as f64 - second.row as f64).powi(2)
                + (first.col as f64 - second.col as f64).powi(2))
                .sqrt();
            row_col_distance <= max_distance
        };
    }
    // Deltas in i64 for overflow safety with saturated extreme coordinates.
    let row_delta = (i64::from(first.row) - i64::from(second.row)).abs();
    let col_delta = (i64::from(first.col) - i64::from(second.col)).abs();
    if row_delta == 0 && 0 < col_delta && col_delta <= i64::from(config.max_col_delta) {
        return config.include_horizontal;
    }
    if col_delta == 0 && 0 < row_delta && row_delta <= i64::from(config.max_row_delta) {
        return config.include_vertical;
    }
    if row_delta != 0 && col_delta != 0 {
        return config.include_diagonal
            && row_delta <= i64::from(config.max_row_delta)
            && col_delta <= i64::from(config.max_col_delta);
    }
    false
}

fn seat_distance(first: &Seat, second: &Seat) -> f64 {
    let dx = first.x_default() - second.x_default();
    let dy = first.y_default() - second.y_default();
    (dx * dx + dy * dy).sqrt()
}

// ---------------------------------------------------------------------------
// small helpers
// ---------------------------------------------------------------------------

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// Python `round()`: round half to even (banker's rounding). The ported cost
/// formula uses `int(round(complement_error * 10_000))`.
fn python_round(value: f64) -> i64 {
    let floor = value.floor();
    let frac = value - floor;
    if frac < 0.5 {
        floor as i64
    } else if frac > 0.5 {
        (floor + 1.0) as i64
    } else {
        let floor_int = floor as i64;
        if floor_int % 2 == 0 {
            floor_int
        } else {
            floor_int + 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn student(key: &str, score: Option<f64>) -> Student {
        Student::new(key, score)
    }

    #[test]
    fn percentiles_with_ties() {
        let students = [
            student("A", Some(50.0)),
            student("B", Some(60.0)),
            student("C", Some(60.0)),
            student("D", Some(80.0)),
            student("E", Some(90.0)),
            student("F", Some(90.0)),
            student("G", Some(90.0)),
        ];
        let p = score_rank_percentiles(&students);
        assert_eq!(p.len(), 7);
        assert_eq!(p["A"], 0.0);
        assert_eq!(p["B"], 0.25);
        assert_eq!(p["C"], 0.25);
        assert_eq!(p["D"], 0.5);
        assert!((p["E"] - 0.8333333333333334).abs() < 1e-15);
        assert_eq!(p["E"], p["F"]);
        assert_eq!(p["F"], p["G"]);
    }

    #[test]
    fn percentiles_empty_for_single_or_missing() {
        assert!(score_rank_percentiles(&[student("A", Some(50.0))]).is_empty());
        assert!(score_rank_percentiles(&[student("A", None), student("B", None)]).is_empty());
        assert!(score_rank_percentiles(&[]).is_empty());
    }

    #[test]
    fn percentiles_equal_scores_share_tie_group() {
        // Equal scores are one tie group regardless of key ordering:
        // (10,A10), (10,A2) -> avg rank 0.5 -> 0.25 each; B -> 1.0.
        let students = [
            student("A10", Some(10.0)),
            student("A2", Some(10.0)),
            student("B", Some(20.0)),
        ];
        let p = score_rank_percentiles(&students);
        assert_eq!(p["A10"], 0.25);
        assert_eq!(p["A2"], 0.25);
        assert_eq!(p["B"], 1.0);
    }
}
