//! SeatTrellis standalone CLI — a small solve + export binary built on
//! `seattrellis_core`.
//!
//! Command-line parsing is deliberately hand-written (`std::env::args_os`) to
//! avoid pulling in clap and to keep the release binary small. See `USAGE`.

mod commands;
mod project;
mod render;
mod style;
mod usage;

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use seattrellis_core::{classify_solve_error, SolveStatus};

use crate::style::Styler;
use crate::usage::render_usage;

/// Version reported by `--version` (kept in sync with Cargo.toml).
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// An option flag the hand-written parser knows about.
struct Flag {
    name: &'static str,
    takes_value: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Svg,
    Html,
    Png,
    Pdf,
    Xlsx,
    Docx,
    Pptx,
}

impl ExportFormat {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.to_ascii_lowercase().as_str() {
            "svg" => Ok(ExportFormat::Svg),
            "html" => Ok(ExportFormat::Html),
            "png" => Ok(ExportFormat::Png),
            "pdf" => Ok(ExportFormat::Pdf),
            "xlsx" => Ok(ExportFormat::Xlsx),
            "docx" => Ok(ExportFormat::Docx),
            "pptx" => Ok(ExportFormat::Pptx),
            other => Err(format!(
                "unknown format '{other}' (expected svg, html, png, pdf, xlsx, docx or pptx)"
            )),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct SolveArgs {
    pub problem: PathBuf,
    pub seed: Option<u64>,
    pub time_limit: Option<f64>,
    pub output: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidateArgs {
    pub problem: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ExportArgs {
    pub problem: PathBuf,
    pub solution: PathBuf,
    pub format: ExportFormat,
    pub output: PathBuf,
    /// `public` | `teacher` (default). Only the Office formats honour it in
    /// the CLI: the renderers receive the privacy-filtered grid.
    pub template: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrecheckArgs {
    pub problem: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AuditArgs {
    pub problem: PathBuf,
    pub solution: PathBuf,
}

/// `score`: fixed-assignment PlanScore breakdown (plan §6.2/§6.6 parity
/// evidence). `assignment` is an inline JSON array of `[student, seat]`
/// index pairs; `latest_snapshot` and `diversity` are optional.
#[derive(Debug, PartialEq)]
pub struct ScoreArgs {
    pub problem: PathBuf,
    pub assignment: String,
    pub latest_snapshot: Option<PathBuf>,
    pub diversity: Option<f64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CandidatesArgs {
    pub problem: PathBuf,
    pub count: usize,
}

/// `doctor`: environment diagnostics (plan §5.5 CLI surface).
#[derive(Debug, PartialEq, Eq)]
pub struct DoctorArgs {}

/// `project-init`: create a `seattrellis_project` workspace file in a
/// directory that already carries `students.csv` / `layout.json` /
/// `rules.json` (plan §5.5 project lifecycle).
#[derive(Debug, PartialEq, Eq)]
pub struct ProjectInitArgs {
    pub dir: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectListArgs {
    pub root: PathBuf,
    pub limit: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectPrivacyArgs {
    pub project: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectPackArgs {
    pub project: PathBuf,
    pub output: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectRestoreArgs {
    pub bundle: PathBuf,
    pub output_dir: PathBuf,
    pub force: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HistoryReportArgs {
    pub problem: PathBuf,
    pub history: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectArgs {
    pub project: PathBuf,
    pub seed: Option<u64>,
    pub format: Option<String>,
    pub output: Option<PathBuf>,
    /// Saved plan to render: `project-export` exports an existing snapshot
    /// (the result of `project-solve --output <file>`), it never re-solves.
    pub snapshot: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectRotateArgs {
    pub project: PathBuf,
    pub periods: usize,
    pub seed: Option<u64>,
    pub output: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectEditArgs {
    pub project: PathBuf,
    pub snapshot: Option<PathBuf>,
    /// `--operation <json>` values, applied in order after any
    /// `--operations-file` entries.
    pub operations: Vec<String>,
    pub operations_file: Option<PathBuf>,
    pub output: Option<PathBuf>,
    /// Fail instead of writing when the edited plan violates hard rules.
    pub strict: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectRepairArgs {
    pub project: PathBuf,
    pub snapshot: Option<PathBuf>,
    pub affected: Vec<String>,
    pub locked_students: Vec<String>,
    pub locked_seats: Vec<String>,
    pub output: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SchemaExportArgs {
    pub kind: String,
    pub output: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SchemaMigrateArgs {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub in_place: bool,
    pub dry_run: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RepairArgs {
    pub problem: PathBuf,
    pub snapshot: PathBuf,
    pub affected: Vec<String>,
    pub locked_students: Vec<String>,
    pub locked_seats: Vec<String>,
    pub output: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PairReportArgs {
    pub problem: PathBuf,
    pub history: Vec<PathBuf>,
    pub top: usize,
    pub within_distance: i32,
}

#[derive(Debug, PartialEq)]
enum Command {
    Help,
    Version,
    Validate(ValidateArgs),
    Precheck(PrecheckArgs),
    Audit(AuditArgs),
    Score(ScoreArgs),
    Candidates(CandidatesArgs),
    Doctor(DoctorArgs),
    ProjectInit(ProjectInitArgs),
    ProjectList(ProjectListArgs),
    ProjectPrivacy(ProjectPrivacyArgs),
    ProjectPack(ProjectPackArgs),
    ProjectRestore(ProjectRestoreArgs),
    HistoryReport(HistoryReportArgs),
    PairReport(PairReportArgs),
    Repair(RepairArgs),
    ProjectInfo(ProjectArgs),
    ProjectValidate(ProjectArgs),
    ProjectSolve(ProjectArgs),
    ProjectExport(ProjectArgs),
    ProjectRotate(ProjectRotateArgs),
    ProjectEdit(ProjectEditArgs),
    ProjectRepair(ProjectRepairArgs),
    SchemaList,
    SchemaExport(SchemaExportArgs),
    SchemaMigrate(SchemaMigrateArgs),
    Solve(SolveArgs),
    Export(ExportArgs),
}

/// Parse `--name value` / `--name=value` tokens against the allowed flag set.
///
/// Returns the flags in order as `(name, value)` pairs; value is `Some` only
/// for flags declared with `takes_value`. Repeated flags keep the last value.
fn parse_flags(
    tokens: &[String],
    allowed: &[Flag],
) -> Result<Vec<(String, Option<String>)>, String> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if !token.starts_with("--") {
            return Err(format!("unexpected argument '{token}'"));
        }
        let (name, inline) = match token.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (token.as_str(), None),
        };
        let spec = allowed
            .iter()
            .find(|flag| flag.name == name)
            .ok_or_else(|| format!("unknown option '{name}'"))?;
        let value = if spec.takes_value {
            match inline {
                Some(value) => Some(value),
                None => {
                    index += 1;
                    if index >= tokens.len() {
                        return Err(format!("option '{name}' requires a value"));
                    }
                    let value = tokens[index].clone();
                    if value.starts_with("--") {
                        return Err(format!("option '{name}' requires a value"));
                    }
                    Some(value)
                }
            }
        } else {
            if inline.is_some() {
                return Err(format!("option '{name}' does not take a value"));
            }
            None
        };
        out.push((name.to_string(), value));
        index += 1;
    }
    Ok(out)
}

/// Look up a flag's value in the parsed pairs (last occurrence wins).
fn flag_value<'a>(
    parsed: &'a [(String, Option<String>)],
    name: &str,
) -> Result<Option<&'a str>, String> {
    let mut found: Option<&'a str> = None;
    for (parsed_name, value) in parsed {
        if parsed_name == name {
            found = Some(
                value
                    .as_deref()
                    .ok_or_else(|| format!("option '{name}' requires a value"))?,
            );
        }
    }
    Ok(found)
}

fn parse_solve(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--seed",
            takes_value: true,
        },
        Flag {
            name: "--time-limit",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem = flag_value(&parsed, "--problem")?.ok_or("solve requires --problem <file>")?;
    let seed = match flag_value(&parsed, "--seed")? {
        Some(raw) => Some(
            raw.parse::<u64>()
                .map_err(|error| format!("invalid seed '{raw}': {error}"))?,
        ),
        None => None,
    };
    let time_limit = match flag_value(&parsed, "--time-limit")? {
        Some(raw) => {
            let seconds = raw
                .parse::<f64>()
                .map_err(|error| format!("invalid --time-limit '{raw}': {error}"))?;
            if !seconds.is_finite() || seconds <= 0.0 {
                return Err("--time-limit must be a positive number of seconds".to_string());
            }
            Some(seconds)
        }
        None => None,
    };
    let output = flag_value(&parsed, "--output")?.map(PathBuf::from);
    Ok(Command::Solve(SolveArgs {
        problem: PathBuf::from(problem),
        seed,
        time_limit,
        output,
    }))
}

fn parse_validate(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem = flag_value(&parsed, "--problem")?.ok_or("validate requires --problem <file>")?;
    Ok(Command::Validate(ValidateArgs {
        problem: PathBuf::from(problem),
    }))
}

fn parse_precheck(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem = flag_value(&parsed, "--problem")?.ok_or("precheck requires --problem <file>")?;
    Ok(Command::Precheck(PrecheckArgs {
        problem: PathBuf::from(problem),
    }))
}

fn parse_audit(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--solution",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem = flag_value(&parsed, "--problem")?.ok_or("audit requires --problem <file>")?;
    let solution = flag_value(&parsed, "--solution")?.ok_or("audit requires --solution <file>")?;
    Ok(Command::Audit(AuditArgs {
        problem: PathBuf::from(problem),
        solution: PathBuf::from(solution),
    }))
}

fn parse_score(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--assignment",
            takes_value: true,
        },
        Flag {
            name: "--latest-snapshot",
            takes_value: true,
        },
        Flag {
            name: "--diversity",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem = flag_value(&parsed, "--problem")?.ok_or("score requires --problem <file>")?;
    let assignment = flag_value(&parsed, "--assignment")?
        .ok_or("score requires --assignment <json>")?
        .to_string();
    let latest_snapshot = flag_value(&parsed, "--latest-snapshot")?.map(PathBuf::from);
    let diversity = match flag_value(&parsed, "--diversity")? {
        Some(raw) => Some(
            raw.parse::<f64>()
                .map_err(|error| format!("invalid --diversity '{raw}': {error}"))?,
        ),
        None => None,
    };
    Ok(Command::Score(ScoreArgs {
        problem: PathBuf::from(problem),
        assignment,
        latest_snapshot,
        diversity,
    }))
}

fn parse_candidates(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--count",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem =
        flag_value(&parsed, "--problem")?.ok_or("candidates requires --problem <file>")?;
    let count = match flag_value(&parsed, "--count")? {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|error| format!("invalid --count '{raw}': {error}"))?,
        None => 5,
    };
    Ok(Command::Candidates(CandidatesArgs {
        problem: PathBuf::from(problem),
        count,
    }))
}

fn parse_history_report(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--history",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem =
        flag_value(&parsed, "--problem")?.ok_or("history-report requires --problem <file>")?;
    let history = parsed
        .iter()
        .filter(|(name, _)| name == "--history")
        .filter_map(|(_, value)| value.clone())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if history.is_empty() {
        return Err("history-report requires at least one --history <snapshot.json>".to_string());
    }
    Ok(Command::HistoryReport(HistoryReportArgs {
        problem: PathBuf::from(problem),
        history,
    }))
}

fn parse_pair_report(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--history",
            takes_value: true,
        },
        Flag {
            name: "--top",
            takes_value: true,
        },
        Flag {
            name: "--within-distance",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem =
        flag_value(&parsed, "--problem")?.ok_or("pair-report requires --problem <file>")?;
    let history = parsed
        .iter()
        .filter(|(name, _)| name == "--history")
        .filter_map(|(_, value)| value.clone())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if history.is_empty() {
        return Err("pair-report requires at least one --history <snapshot.json>".to_string());
    }
    let top = match flag_value(&parsed, "--top")? {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|error| format!("invalid --top '{raw}': {error}"))?,
        None => 10,
    };
    let within_distance = match flag_value(&parsed, "--within-distance")? {
        Some(raw) => raw
            .parse::<i32>()
            .map_err(|error| format!("invalid --within-distance '{raw}': {error}"))?,
        None => 2,
    };
    Ok(Command::PairReport(PairReportArgs {
        problem: PathBuf::from(problem),
        history,
        top,
        within_distance,
    }))
}

fn parse_project_command(command: &str, tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--project",
            takes_value: true,
        },
        Flag {
            name: "--seed",
            takes_value: true,
        },
        Flag {
            name: "--format",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--snapshot",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let project = flag_value(&parsed, "--project")?
        .ok_or_else(|| format!("{command} requires --project <file>"))?;
    let seed = match flag_value(&parsed, "--seed")? {
        Some(raw) => Some(
            raw.parse::<u64>()
                .map_err(|error| format!("invalid seed '{raw}': {error}"))?,
        ),
        None => None,
    };
    let format = flag_value(&parsed, "--format")?.map(str::to_string);
    let output = flag_value(&parsed, "--output")?.map(PathBuf::from);
    let snapshot = flag_value(&parsed, "--snapshot")?.map(PathBuf::from);
    let args = ProjectArgs {
        project: PathBuf::from(project),
        seed,
        format,
        output,
        snapshot,
    };
    Ok(match command {
        "project-info" => Command::ProjectInfo(args),
        "project-validate" => Command::ProjectValidate(args),
        "project-solve" => Command::ProjectSolve(args),
        "project-export" => Command::ProjectExport(args),
        _ => return Err(format!("unknown project command {command}")),
    })
}

fn parse_project_rotate(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--project",
            takes_value: true,
        },
        Flag {
            name: "--periods",
            takes_value: true,
        },
        Flag {
            name: "--seed",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let project =
        flag_value(&parsed, "--project")?.ok_or("project-rotate requires --project <file>")?;
    let periods = match flag_value(&parsed, "--periods")? {
        None => 4,
        Some(raw) => raw.parse::<usize>().map_err(|error| {
            format!("invalid --periods value '{raw}': {error} (expected 1..=20)")
        })?,
    };
    if periods == 0 || periods > 20 {
        return Err(format!("--periods must be between 1 and 20, got {periods}"));
    }
    let seed = match flag_value(&parsed, "--seed")? {
        Some(raw) => Some(
            raw.parse::<u64>()
                .map_err(|error| format!("invalid seed '{raw}': {error}"))?,
        ),
        None => None,
    };
    let output = flag_value(&parsed, "--output")?.map(PathBuf::from);
    Ok(Command::ProjectRotate(ProjectRotateArgs {
        project: PathBuf::from(project),
        periods,
        seed,
        output,
    }))
}

fn parse_project_edit(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--project",
            takes_value: true,
        },
        Flag {
            name: "--snapshot",
            takes_value: true,
        },
        Flag {
            name: "--operation",
            takes_value: true,
        },
        Flag {
            name: "--operations-file",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--strict",
            takes_value: false,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let project =
        flag_value(&parsed, "--project")?.ok_or("project-edit requires --project <file>")?;
    let operations = parsed
        .iter()
        .filter(|(name, _)| name == "--operation")
        .filter_map(|(_, value)| value.clone())
        .collect();
    let operations_file = flag_value(&parsed, "--operations-file")?.map(PathBuf::from);
    let snapshot = flag_value(&parsed, "--snapshot")?.map(PathBuf::from);
    let output = flag_value(&parsed, "--output")?.map(PathBuf::from);
    let strict = parsed.iter().any(|(name, _)| name == "--strict");
    Ok(Command::ProjectEdit(ProjectEditArgs {
        project: PathBuf::from(project),
        snapshot,
        operations,
        operations_file,
        output,
        strict,
    }))
}

fn parse_project_repair(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--project",
            takes_value: true,
        },
        Flag {
            name: "--snapshot",
            takes_value: true,
        },
        Flag {
            name: "--affected",
            takes_value: true,
        },
        Flag {
            name: "--locked-students",
            takes_value: true,
        },
        Flag {
            name: "--locked-seats",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let project =
        flag_value(&parsed, "--project")?.ok_or("project-repair requires --project <file>")?;
    let affected = flag_value(&parsed, "--affected")?
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let locked_students = flag_value(&parsed, "--locked-students")?
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let locked_seats = flag_value(&parsed, "--locked-seats")?
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let snapshot = flag_value(&parsed, "--snapshot")?.map(PathBuf::from);
    let output = flag_value(&parsed, "--output")?.map(PathBuf::from);
    Ok(Command::ProjectRepair(ProjectRepairArgs {
        project: PathBuf::from(project),
        snapshot,
        affected,
        locked_students,
        locked_seats,
        output,
    }))
}

fn parse_schema_list(tokens: &[String]) -> Result<Command, String> {
    if tokens.iter().any(|token| token == "--help") {
        return Ok(Command::Help);
    }
    if !tokens.is_empty() {
        return Err(format!("schema-list takes no arguments: {tokens:?}"));
    }
    Ok(Command::SchemaList)
}

fn parse_schema_export(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--kind",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let kind = flag_value(&parsed, "--kind")?
        .ok_or("schema-export requires --kind <student_roster|classroom_layout|ruleset|seating_snapshot|project|project_bundle_manifest|candidate_set|rotation_plan>")?;
    let output = flag_value(&parsed, "--output")?.map(PathBuf::from);
    Ok(Command::SchemaExport(SchemaExportArgs {
        kind: kind.to_string(),
        output,
    }))
}

fn parse_schema_migrate(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--input",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--in-place",
            takes_value: false,
        },
        Flag {
            name: "--dry-run",
            takes_value: false,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let input = flag_value(&parsed, "--input")?.ok_or("schema-migrate requires --input <file>")?;
    let output = flag_value(&parsed, "--output")?.map(PathBuf::from);
    let in_place = parsed.iter().any(|(name, _)| name == "--in-place");
    let dry_run = parsed.iter().any(|(name, _)| name == "--dry-run");
    if in_place && dry_run {
        return Err("schema-migrate: --in-place and --dry-run are mutually exclusive".to_string());
    }
    Ok(Command::SchemaMigrate(SchemaMigrateArgs {
        input: PathBuf::from(input),
        output,
        in_place,
        dry_run,
    }))
}

fn parse_doctor(tokens: &[String]) -> Result<Command, String> {
    if tokens.iter().any(|token| token == "--help") {
        return Ok(Command::Help);
    }
    if !tokens.is_empty() {
        return Err(format!("doctor takes no arguments: {tokens:?}"));
    }
    Ok(Command::Doctor(DoctorArgs {}))
}

fn parse_project_init(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--dir",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let dir = flag_value(&parsed, "--dir")?.ok_or("project-init requires --dir <directory>")?;
    Ok(Command::ProjectInit(ProjectInitArgs {
        dir: PathBuf::from(dir),
    }))
}

fn parse_project_list(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--root",
            takes_value: true,
        },
        Flag {
            name: "--limit",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let root = flag_value(&parsed, "--root")?.unwrap_or(".").to_string();
    let limit = match flag_value(&parsed, "--limit")? {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|error| format!("invalid --limit '{raw}': {error}"))?,
        None => 20,
    };
    Ok(Command::ProjectList(ProjectListArgs {
        root: PathBuf::from(root),
        limit,
    }))
}

fn parse_project_privacy(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--project",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let project =
        flag_value(&parsed, "--project")?.ok_or("project-privacy requires --project <file>")?;
    Ok(Command::ProjectPrivacy(ProjectPrivacyArgs {
        project: PathBuf::from(project),
    }))
}

fn parse_project_pack(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--project",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let project =
        flag_value(&parsed, "--project")?.ok_or("project-pack requires --project <file>")?;
    let output = flag_value(&parsed, "--output")?.ok_or("project-pack requires --output <file>")?;
    Ok(Command::ProjectPack(ProjectPackArgs {
        project: PathBuf::from(project),
        output: PathBuf::from(output),
    }))
}

fn parse_project_restore(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--bundle",
            takes_value: true,
        },
        Flag {
            name: "--output-dir",
            takes_value: true,
        },
        Flag {
            name: "--force",
            takes_value: false,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let bundle =
        flag_value(&parsed, "--bundle")?.ok_or("project-restore requires --bundle <file>")?;
    let output_dir = flag_value(&parsed, "--output-dir")?
        .ok_or("project-restore requires --output-dir <directory>")?;
    let force = parsed.iter().any(|(name, _)| name == "--force");
    Ok(Command::ProjectRestore(ProjectRestoreArgs {
        bundle: PathBuf::from(bundle),
        output_dir: PathBuf::from(output_dir),
        force,
    }))
}

fn parse_repair(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--snapshot",
            takes_value: true,
        },
        Flag {
            name: "--affected",
            takes_value: true,
        },
        Flag {
            name: "--lock-student",
            takes_value: true,
        },
        Flag {
            name: "--lock-seat",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem = flag_value(&parsed, "--problem")?.ok_or("repair requires --problem <file>")?;
    let snapshot = flag_value(&parsed, "--snapshot")?.ok_or("repair requires --snapshot <file>")?;
    let affected = parsed
        .iter()
        .filter(|(name, _)| name == "--affected")
        .filter_map(|(_, value)| value.clone())
        .collect();
    let locked_students = parsed
        .iter()
        .filter(|(name, _)| name == "--lock-student")
        .filter_map(|(_, value)| value.clone())
        .collect();
    let locked_seats = parsed
        .iter()
        .filter(|(name, _)| name == "--lock-seat")
        .filter_map(|(_, value)| value.clone())
        .collect();
    let output = flag_value(&parsed, "--output")?.map(PathBuf::from);
    Ok(Command::Repair(RepairArgs {
        problem: PathBuf::from(problem),
        snapshot: PathBuf::from(snapshot),
        affected,
        locked_students,
        locked_seats,
        output,
    }))
}

fn parse_export(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag {
            name: "--problem",
            takes_value: true,
        },
        Flag {
            name: "--solution",
            takes_value: true,
        },
        Flag {
            name: "--format",
            takes_value: true,
        },
        Flag {
            name: "--output",
            takes_value: true,
        },
        Flag {
            name: "--template",
            takes_value: true,
        },
        Flag {
            name: "--help",
            takes_value: false,
        },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem = flag_value(&parsed, "--problem")?.ok_or("export requires --problem <file>")?;
    let solution = flag_value(&parsed, "--solution")?.ok_or("export requires --solution <file>")?;
    let format = ExportFormat::parse(
        flag_value(&parsed, "--format")?
            .ok_or("export requires --format <svg|html|png|pdf|xlsx|docx|pptx>")?,
    )?;
    let output = flag_value(&parsed, "--output")?.ok_or("export requires --output <file>")?;
    let template = flag_value(&parsed, "--template")?.unwrap_or("teacher");
    if template != "public" && template != "teacher" {
        return Err(format!(
            "unknown export template '{template}' (expected public or teacher)"
        ));
    }
    Ok(Command::Export(ExportArgs {
        problem: PathBuf::from(problem),
        solution: PathBuf::from(solution),
        format,
        output: PathBuf::from(output),
        template: template.to_string(),
    }))
}

fn parse_args(args: &[OsString]) -> Result<Command, String> {
    let text: Vec<String> = args
        .iter()
        .map(|arg| {
            arg.to_str()
                .map(str::to_string)
                .ok_or_else(|| format!("argument is not valid UTF-8: {arg:?}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let Some(command) = text.first() else {
        return Ok(Command::Help);
    };
    match command.as_str() {
        "--help" | "-h" | "help" => Ok(Command::Help),
        "--version" | "-V" | "version" => Ok(Command::Version),
        "validate" => parse_validate(&text[1..]),
        "precheck" => parse_precheck(&text[1..]),
        "audit" => parse_audit(&text[1..]),
        "score" => parse_score(&text[1..]),
        "doctor" => parse_doctor(&text[1..]),
        "candidates" => parse_candidates(&text[1..]),
        "history-report" => parse_history_report(&text[1..]),
        "pair-report" => parse_pair_report(&text[1..]),
        "repair" => parse_repair(&text[1..]),
        "project-init" => parse_project_init(&text[1..]),
        "project-list" => parse_project_list(&text[1..]),
        "project-privacy" => parse_project_privacy(&text[1..]),
        "project-pack" => parse_project_pack(&text[1..]),
        "project-restore" => parse_project_restore(&text[1..]),
        "project-info" => parse_project_command("project-info", &text[1..]),
        "project-validate" => parse_project_command("project-validate", &text[1..]),
        "project-solve" => parse_project_command("project-solve", &text[1..]),
        "project-export" => parse_project_command("project-export", &text[1..]),
        "project-rotate" => parse_project_rotate(&text[1..]),
        "project-edit" => parse_project_edit(&text[1..]),
        "project-repair" => parse_project_repair(&text[1..]),
        "schema-list" => parse_schema_list(&text[1..]),
        "schema-export" => parse_schema_export(&text[1..]),
        "schema-migrate" => parse_schema_migrate(&text[1..]),
        "solve" => parse_solve(&text[1..]),
        "export" => parse_export(&text[1..]),
        other => Err(format!("unknown command '{other}'")),
    }
}

/// Frozen CLI v2 exit codes (plan §四.1 / M1-03):
/// Solved 0, InvalidInput 2, ProvenInfeasible 3, Timeout 4, Unknown 5,
/// InternalError 70, user cancel 130.
fn exit_code_for(status: SolveStatus) -> u8 {
    match status {
        SolveStatus::Solved => 0,
        SolveStatus::InvalidInput => 2,
        SolveStatus::ProvenInfeasible => 3,
        SolveStatus::Timeout => 4,
        SolveStatus::Unknown => 5,
        SolveStatus::Cancelled => 130,
        SolveStatus::InternalError => 70,
    }
}

fn run_command(command: Command) -> ExitCode {
    match command {
        Command::Help => {
            let styler = Styler::stdout();
            println!("{}", render_usage(&styler));
            ExitCode::SUCCESS
        }
        Command::Version => {
            let styler = Styler::stdout();
            println!(
                "{} {}",
                styler.bold("seattrellis_cli"),
                styler.cyan(VERSION)
            );
            ExitCode::SUCCESS
        }
        Command::Validate(args) => match commands::run_validate(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                // validate only ever judges input: InvalidInput (frozen 2).
                ExitCode::from(2)
            }
        },
        Command::Precheck(args) => match commands::run_precheck(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                // Precheck only judges input: InvalidInput (frozen 2).
                ExitCode::from(2)
            }
        },
        Command::Candidates(args) => match commands::run_candidates(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::Audit(args) => match commands::run_audit(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::Score(args) => match commands::run_score(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::Doctor(_) => match commands::run_doctor() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectInit(args) => match commands::run_project_init(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectList(args) => match commands::run_project_list(&args) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectPrivacy(args) => match commands::run_project_privacy(&args) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectPack(args) => match commands::run_project_pack(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectRestore(args) => match commands::run_project_restore(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::HistoryReport(args) => match commands::run_history_report(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::PairReport(args) => match commands::run_pair_report(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::Repair(args) => match commands::run_repair(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectInfo(args) => match project::project_info(&args.project) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectValidate(args) => match project::project_validate(&args.project) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectSolve(args) => match commands::run_project_solve(&args) {
            Ok(status) => ExitCode::from(exit_code_for(status)),
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(exit_code_for(classify_solve_error(&message)))
            }
        },
        Command::ProjectExport(args) => match commands::run_project_export(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectRotate(args) => match commands::run_project_rotate(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectEdit(args) => match commands::run_project_edit(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::ProjectRepair(args) => match commands::run_project_repair(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::SchemaList => match commands::run_schema_list() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::SchemaExport(args) => match commands::run_schema_export(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::SchemaMigrate(args) => match commands::run_schema_migrate(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(2)
            }
        },
        Command::Solve(args) => match commands::run_solve(&args) {
            Ok(status) => ExitCode::from(exit_code_for(status)),
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                ExitCode::from(exit_code_for(classify_solve_error(&message)))
            }
        },
        Command::Export(args) => match commands::run_export(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                let styler = Styler::stderr();
                eprintln!("{}: {message}", styler.red("error"));
                // Export failures are not part of the frozen solve table;
                // keep them internal (70) until an export status contract lands.
                ExitCode::from(70)
            }
        },
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    match parse_args(&args) {
        Ok(command) => run_command(command),
        Err(message) => {
            let styler = Styler::stderr();
            eprintln!("{}: {message}", styler.red("error"));
            eprintln!("run '{} --help' for usage", styler.cyan("seattrellis_cli"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_of(tokens: &[&str]) -> Vec<OsString> {
        tokens.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_arguments_show_help() {
        assert_eq!(parse_args(&[]).unwrap(), Command::Help);
    }

    #[test]
    fn help_and_version_are_recognised() {
        assert_eq!(parse_args(&args_of(&["--help"])).unwrap(), Command::Help);
        assert_eq!(parse_args(&args_of(&["-h"])).unwrap(), Command::Help);
        assert_eq!(parse_args(&args_of(&["help"])).unwrap(), Command::Help);
        assert_eq!(
            parse_args(&args_of(&["--version"])).unwrap(),
            Command::Version
        );
        assert_eq!(parse_args(&args_of(&["-V"])).unwrap(), Command::Version);
        assert_eq!(
            parse_args(&args_of(&["version"])).unwrap(),
            Command::Version
        );
        // --help inside a subcommand shows help too.
        assert_eq!(
            parse_args(&args_of(&["solve", "--help"])).unwrap(),
            Command::Help
        );
        assert_eq!(
            parse_args(&args_of(&["export", "--help"])).unwrap(),
            Command::Help
        );
    }

    #[test]
    fn parse_solve_minimal() {
        let Command::Solve(args) = parse_args(&args_of(&["solve", "--problem", "p.json"])).unwrap()
        else {
            panic!("expected a solve command");
        };
        assert_eq!(args.problem, PathBuf::from("p.json"));
        assert_eq!(args.seed, None);
        assert_eq!(args.output, None);
    }

    #[test]
    fn parse_validate_requires_a_problem() {
        let Command::Validate(args) =
            parse_args(&args_of(&["validate", "--problem", "p.json"])).unwrap()
        else {
            panic!("expected a validate command");
        };
        assert_eq!(args.problem, PathBuf::from("p.json"));

        let error = parse_args(&args_of(&["validate"])).unwrap_err();
        assert!(error.contains("--problem"), "unexpected error: {error}");
    }

    #[test]
    fn parse_solve_with_all_options() {
        let Command::Solve(args) = parse_args(&args_of(&[
            "solve",
            "--problem=p.json",
            "--seed",
            "7",
            "--output=out.json",
        ]))
        .unwrap() else {
            panic!("expected a solve command");
        };
        assert_eq!(args.problem, PathBuf::from("p.json"));
        assert_eq!(args.seed, Some(7));
        assert_eq!(args.output, Some(PathBuf::from("out.json")));
    }

    #[test]
    fn solve_requires_problem() {
        let error = parse_args(&args_of(&["solve", "--seed", "3"])).unwrap_err();
        assert!(error.contains("--problem"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_unknown_option() {
        let error = parse_args(&args_of(&["solve", "--problem", "p", "--bogus", "x"])).unwrap_err();
        assert!(
            error.contains("unknown option '--bogus'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_missing_option_value() {
        let error = parse_args(&args_of(&["solve", "--problem"])).unwrap_err();
        assert!(
            error.contains("requires a value"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_bad_seed() {
        let error =
            parse_args(&args_of(&["solve", "--problem", "p", "--seed", "abc"])).unwrap_err();
        assert!(
            error.contains("invalid seed 'abc'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_unknown_command() {
        let error = parse_args(&args_of(&["frobnicate"])).unwrap_err();
        assert!(
            error.contains("unknown command 'frobnicate'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn parse_export_with_all_options() {
        let Command::Export(args) = parse_args(&args_of(&[
            "export",
            "--problem",
            "p.json",
            "--solution",
            "s.json",
            "--format",
            "svg",
            "--output",
            "o.svg",
        ]))
        .unwrap() else {
            panic!("expected an export command");
        };
        assert_eq!(args.problem, PathBuf::from("p.json"));
        assert_eq!(args.solution, PathBuf::from("s.json"));
        assert_eq!(args.format, ExportFormat::Svg);
        assert_eq!(args.output, PathBuf::from("o.svg"));
        assert_eq!(args.template, "teacher");
    }

    #[test]
    fn parse_export_inline_values_and_html_format() {
        let Command::Export(args) = parse_args(&args_of(&[
            "export",
            "--problem=p",
            "--solution=s",
            "--format=html",
            "--output=o",
        ]))
        .unwrap() else {
            panic!("expected an export command");
        };
        assert_eq!(args.format, ExportFormat::Html);
    }

    #[test]
    fn rejects_bad_format() {
        let error = parse_args(&args_of(&[
            "export",
            "--problem",
            "p",
            "--solution",
            "s",
            "--format",
            "bmp",
            "--output",
            "o",
        ]))
        .unwrap_err();
        assert!(
            error.contains("unknown format 'bmp'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn parse_export_png_and_pdf_formats() {
        let Command::Export(args) = parse_args(&args_of(&[
            "export",
            "--problem=p",
            "--solution=s",
            "--format=png",
            "--output=o",
        ]))
        .unwrap() else {
            panic!("expected an export command");
        };
        assert_eq!(args.format, ExportFormat::Png);

        let Command::Export(args) = parse_args(&args_of(&[
            "export",
            "--problem=p",
            "--solution=s",
            "--format",
            "PDF",
            "--output",
            "o",
        ]))
        .unwrap() else {
            panic!("expected an export command");
        };
        assert_eq!(args.format, ExportFormat::Pdf, "format is case-insensitive");
    }

    #[test]
    fn export_requires_all_options() {
        let error = parse_args(&args_of(&["export", "--format", "svg"])).unwrap_err();
        assert!(error.contains("--problem"), "unexpected error: {error}");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_argument() {
        use std::os::unix::ffi::OsStringExt;
        let args = vec![
            OsString::from_vec(b"solve".to_vec()),
            OsString::from_vec(vec![0xff]),
        ];
        let error = parse_args(&args).unwrap_err();
        assert!(
            error.contains("not valid UTF-8"),
            "unexpected error: {error}"
        );
    }

    // ------------------------------------------------------------------
    // M1-03: frozen CLI exit-code table (plan §四.1)
    // ------------------------------------------------------------------

    #[test]
    fn exit_code_table_is_frozen() {
        let cases = [
            (SolveStatus::Solved, 0),
            (SolveStatus::InvalidInput, 2),
            (SolveStatus::ProvenInfeasible, 3),
            (SolveStatus::Timeout, 4),
            (SolveStatus::Unknown, 5),
            (SolveStatus::Cancelled, 130),
            (SolveStatus::InternalError, 70),
        ];
        for (status, expected) in cases {
            assert_eq!(exit_code_for(status), expected, "status {status:?}");
        }
    }

    #[test]
    fn solve_error_classification_drives_exit_codes() {
        // Invalid input must exit 2, never 1 or 70.
        assert_eq!(
            exit_code_for(classify_solve_error("unsupported api_version 99")),
            2
        );
        assert_eq!(
            exit_code_for(classify_solve_error(
                "native solve requires at least one seat"
            )),
            2
        );
        assert_eq!(
            exit_code_for(classify_solve_error(
                "Duplicate student identifiers: STU001"
            )),
            2
        );
        // Internal faults exit 70.
        assert_eq!(
            exit_code_for(classify_solve_error("solver panicked while ranking")),
            70
        );
    }

    // ------------------------------------------------------------------
    // Plan §5.5: project lifecycle (solve saves a snapshot, export renders
    // the SAVED plan and never re-solves)
    // ------------------------------------------------------------------

    fn write_project_workspace(dir: &std::path::Path) -> std::path::PathBuf {
        use std::fs;
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("project.json"),
            r#"{"kind":"seattrellis_project","name":"Lifecycle","students":"students.csv","layout":"layout.json","rules":"rules.json","history_dir":"history","outputs_dir":"outputs"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("students.csv"),
            "id,name\n1,Alice\n2,Bob\n3,Carol\n4,Dan\n",
        )
        .unwrap();
        fs::write(
            dir.join("layout.json"),
            r#"{"layout_id":"l","name":"Room","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
                {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
                {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R2C1","R2C2"],["R1C1","R2C1"],["R1C2","R2C2"]]}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("rules.json"),
            r#"{"seed":7,"soft":{"score_balance":{"enabled":false,"weight":0}}}"#,
        )
        .unwrap();
        dir.join("project.json")
    }

    #[test]
    fn project_lifecycle_solves_then_exports_the_saved_plan() {
        let dir = std::env::temp_dir().join(format!(
            "seattrellis-cli-lifecycle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = write_project_workspace(&dir);
        let snapshot = dir.join("snapshot.json");
        let output = dir.join("plan.svg");

        // project-solve writes the snapshot.
        let status = commands::run_project_solve(&ProjectArgs {
            project: project.clone(),
            seed: None,
            format: None,
            output: Some(snapshot.clone()),
            snapshot: None,
        })
        .expect("project-solve should succeed");
        assert_eq!(status, SolveStatus::Solved);
        assert!(snapshot.is_file());

        // project-export renders the saved snapshot, never re-solving.
        commands::run_project_export(&ProjectArgs {
            project: project.clone(),
            seed: None,
            format: Some("svg".to_string()),
            output: Some(output.clone()),
            snapshot: Some(snapshot.clone()),
        })
        .expect("project-export should succeed");
        let rendered = std::fs::read_to_string(&output).unwrap();
        assert!(rendered.contains("<svg"), "expected an SVG document");
        assert!(rendered.contains("Alice"), "saved plan names are rendered");

        // project-export without --snapshot explains the lifecycle.
        let error = commands::run_project_export(&ProjectArgs {
            project: project.clone(),
            seed: None,
            format: Some("svg".to_string()),
            output: Some(dir.join("missing.svg")),
            snapshot: None,
        })
        .expect_err("project-export without --snapshot must refuse to re-solve");
        assert!(
            error.contains("project-solve --output"),
            "unexpected error: {error}"
        );

        // The saved plan is validated at the export boundary: a snapshot
        // that violates a hard rule must be refused, not exported. Roster
        // keys come from the student_id column ("1".."4").
        std::fs::write(
            dir.join("bad.json"),
            r#"{"assignments":[{"student_key":"1","seat_id":"R1C1"},{"student_key":"2","seat_id":"R1C1"},{"student_key":"3","seat_id":"R2C1"},{"student_key":"4","seat_id":"R2C2"}]}"#,
        )
        .unwrap();
        let error = commands::run_project_export(&ProjectArgs {
            project,
            seed: None,
            format: Some("svg".to_string()),
            output: Some(dir.join("bad.svg")),
            snapshot: Some(dir.join("bad.json")),
        })
        .expect_err("a snapshot that double-occupies a seat must be refused");
        assert!(
            error.contains("not valid") || error.contains("more than once"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_lifecycle_init_pack_restore_and_privacy() {
        // §5.5 full project lifecycle through the CLI: init -> info ->
        // solve -> privacy -> pack -> restore -> list.
        let dir = std::env::temp_dir().join(format!(
            "seattrellis-cli-lifecycle2-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("students.csv"), "id,name\n1,A\n2,B\n3,C\n4,D\n").unwrap();
        std::fs::write(
            dir.join("layout.json"),
            r#"{"layout_id":"l","name":"Room","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
                {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
                {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R2C1","R2C2"],["R1C1","R2C1"],["R1C2","R2C2"]]}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("rules.json"), r#"{"seed":7,"soft":{}}"#).unwrap();

        // init
        commands::run_project_init(&ProjectInitArgs { dir: dir.clone() }).unwrap();
        let project = dir.join("seattrellis.project.json");
        assert!(project.is_file());
        // info
        let info = crate::project::project_info(&project).unwrap();
        assert!(info.contains("students   students.csv (ok)"), "got: {info}");
        // solve (project-solve writes the snapshot)
        commands::run_project_solve(&ProjectArgs {
            project: project.clone(),
            seed: None,
            format: None,
            output: Some(dir.join("snapshot.json")),
            snapshot: None,
        })
        .unwrap();
        // privacy: fail-closed verdict on a teacher project
        let privacy = commands::run_project_privacy(&ProjectPrivacyArgs {
            project: project.clone(),
        })
        .unwrap();
        assert!(privacy.contains("\"verdict\""), "got: {privacy}");
        // pack + restore into a fresh dir
        let bundle = dir.join("bundle.zip");
        commands::run_project_pack(&ProjectPackArgs {
            project: project.clone(),
            output: bundle.clone(),
        })
        .unwrap();
        let restored_root = std::env::temp_dir().join(format!(
            "seattrellis-cli-restored-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        commands::run_project_restore(&ProjectRestoreArgs {
            bundle: bundle.clone(),
            output_dir: restored_root.clone(),
            force: false,
        })
        .unwrap();
        let restored_project = restored_root.join("seattrellis.project.json");
        assert!(restored_project.is_file());
        assert!(crate::project::project_validate(&restored_project).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&restored_root);
    }

    #[test]
    fn project_rotate_edit_repair_and_schema_group() {
        // §5.5/§5.7 item 3: the remaining project lifecycle commands plus the
        // schema group, end to end on a real workspace.
        let dir = std::env::temp_dir().join(format!(
            "seattrellis-cli-lifecycle3-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("students.csv"), "id,name
1,A
2,B
3,C
4,D
").unwrap();
        std::fs::write(
            dir.join("layout.json"),
            r#"{"layout_id":"l","name":"Room","seats":[
                {"seat_id":"R1C1","row":1,"col":1,"x":0.0,"y":0.0,"enabled":true},
                {"seat_id":"R1C2","row":1,"col":2,"x":1.0,"y":0.0,"enabled":true},
                {"seat_id":"R2C1","row":2,"col":1,"x":0.0,"y":1.0,"enabled":true},
                {"seat_id":"R2C2","row":2,"col":2,"x":1.0,"y":1.0,"enabled":true}
            ],"adjacency":{"edges":[["R1C1","R1C2"],["R2C1","R2C2"],["R1C1","R2C1"],["R1C2","R2C2"]]}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("rules.json"), r#"{"seed":7,"soft":{}}"#).unwrap();
        commands::run_project_init(&ProjectInitArgs { dir: dir.clone() }).unwrap();
        let project = dir.join("seattrellis.project.json");
        let snapshot = dir.join("outputs").join("plan.snapshot.json");

        // project-solve -> saved snapshot (CoreSolveResponse shape).
        commands::run_project_solve(&ProjectArgs {
            project: project.clone(),
            seed: None,
            format: None,
            output: Some(snapshot.clone()),
            snapshot: None,
        })
        .unwrap();
        assert!(snapshot.is_file());

        // project-rotate: 2 periods, persisted into the project outputs.
        commands::run_project_rotate(&ProjectRotateArgs {
            project: project.clone(),
            periods: 2,
            seed: None,
            output: None,
        })
        .unwrap();
        let rotation = dir.join("outputs").join("rotation-plan.json");
        assert!(rotation.is_file());
        let plan: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&rotation).unwrap()).unwrap();
        assert_eq!(plan["kind"], "rotation_plan");
        assert_eq!(plan["periods"].as_array().unwrap().len(), 2);

        // project-edit: swap two students, then a lock via operations file.
        let edited = dir.join("outputs").join("edited.snapshot.json");
        commands::run_project_edit(&ProjectEditArgs {
            project: project.clone(),
            snapshot: Some(snapshot.clone()),
            operations: vec![
                r#"{"kind":"swap_students","payload":{"first_student":"1","second_student":"2"}}"#
                    .to_string(),
            ],
            operations_file: None,
            output: Some(edited.clone()),
            strict: false,
        })
        .unwrap();
        let edited_doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&edited).unwrap()).unwrap();
        let assignments = edited_doc["assignments"].as_array().unwrap();
        assert_eq!(assignments.len(), 4);

        // Strict mode must refuse an edit that violates hard rules: moving
        // student 1 to a seat that breaks the fixed-seat rule... there is no
        // hard rule here, so a strict swap stays valid.
        let ops_file = dir.join("ops.json");
        std::fs::write(
            &ops_file,
            r#"{"operations":[{"kind":"lock_student","payload":{"student_key":"1"}}]}"#,
        )
        .unwrap();
        commands::run_project_edit(&ProjectEditArgs {
            project: project.clone(),
            snapshot: Some(snapshot.clone()),
            operations: vec![],
            operations_file: Some(ops_file),
            output: Some(dir.join("outputs").join("locked.snapshot.json")),
            strict: true,
        })
        .unwrap();

        // project-repair: re-solve the edited snapshot with a lock.
        commands::run_project_repair(&ProjectRepairArgs {
            project: project.clone(),
            snapshot: Some(edited.clone()),
            affected: vec![],
            locked_students: vec!["1".to_string()],
            locked_seats: vec![],
            output: Some(dir.join("outputs").join("repaired.snapshot.json")),
        })
        .unwrap();
        assert!(dir.join("outputs").join("repaired.snapshot.json").is_file());

        // schema group: list, export, migrate.
        commands::run_schema_list().unwrap();
        let schema_out = dir.join("roster.schema.json");
        commands::run_schema_export(&SchemaExportArgs {
            kind: "roster".to_string(),
            output: Some(schema_out.clone()),
        })
        .unwrap();
        let schema_text = std::fs::read_to_string(&schema_out).unwrap();
        assert!(schema_text.contains("$defs"), "schema export looks like JSON Schema");
        let v1 = dir.join("v1-roster.json");
        std::fs::write(
            &v1,
            r#"{"kind":"student_roster","schema_version":1,"data":{"students":[
                {"student_id":"1","name":"A","gender":"F","height_cm":165,"score":88,
                 "vision":"0.8","tags":[],"needs":[]}]}}"#,
        )
        .unwrap();
        let v2_out = dir.join("v2-roster.json");
        commands::run_schema_migrate(&SchemaMigrateArgs {
            input: v1,
            output: Some(v2_out.clone()),
            in_place: false,
            dry_run: false,
        })
        .unwrap();
        let migrated: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&v2_out).unwrap()).unwrap();
        assert_eq!(migrated["kind"], "student_roster");
        assert_eq!(migrated["schema_version"], 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
