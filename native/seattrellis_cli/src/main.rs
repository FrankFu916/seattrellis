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
}

impl ExportFormat {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.to_ascii_lowercase().as_str() {
            "svg" => Ok(ExportFormat::Svg),
            "html" => Ok(ExportFormat::Html),
            "png" => Ok(ExportFormat::Png),
            "pdf" => Ok(ExportFormat::Pdf),
            other => Err(format!(
                "unknown format '{other}' (expected svg, html, png or pdf)"
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
    HistoryReport(HistoryReportArgs),
    PairReport(PairReportArgs),
    Repair(RepairArgs),
    ProjectInfo(ProjectArgs),
    ProjectValidate(ProjectArgs),
    ProjectSolve(ProjectArgs),
    ProjectExport(ProjectArgs),
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
        flag_value(&parsed, "--format")?.ok_or("export requires --format <svg|html|png|pdf>")?,
    )?;
    let output = flag_value(&parsed, "--output")?.ok_or("export requires --output <file>")?;
    Ok(Command::Export(ExportArgs {
        problem: PathBuf::from(problem),
        solution: PathBuf::from(solution),
        format,
        output: PathBuf::from(output),
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
        "candidates" => parse_candidates(&text[1..]),
        "history-report" => parse_history_report(&text[1..]),
        "pair-report" => parse_pair_report(&text[1..]),
        "repair" => parse_repair(&text[1..]),
        "project-info" => parse_project_command("project-info", &text[1..]),
        "project-validate" => parse_project_command("project-validate", &text[1..]),
        "project-solve" => parse_project_command("project-solve", &text[1..]),
        "project-export" => parse_project_command("project-export", &text[1..]),
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
}
