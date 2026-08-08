//! SeatTrellis standalone CLI — a small solve + export binary built on
//! `seattrellis_core`.
//!
//! Command-line parsing is deliberately hand-written (`std::env::args_os`) to
//! avoid pulling in clap and to keep the release binary small. See `USAGE`.

mod commands;
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

#[derive(Debug, PartialEq, Eq)]
pub struct SolveArgs {
    pub problem: PathBuf,
    pub seed: Option<u64>,
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
enum Command {
    Help,
    Version,
    Validate(ValidateArgs),
    Solve(SolveArgs),
    Export(ExportArgs),
}

/// Parse `--name value` / `--name=value` tokens against the allowed flag set.
///
/// Returns the flags in order as `(name, value)` pairs; value is `Some` only
/// for flags declared with `takes_value`. Repeated flags keep the last value.
fn parse_flags(tokens: &[String], allowed: &[Flag]) -> Result<Vec<(String, Option<String>)>, String> {
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
fn flag_value<'a>(parsed: &'a [(String, Option<String>)], name: &str) -> Result<Option<&'a str>, String> {
    let mut found: Option<&'a str> = None;
    for (parsed_name, value) in parsed {
        if parsed_name == name {
            found = Some(value.as_deref().ok_or_else(|| format!("option '{name}' requires a value"))?);
        }
    }
    Ok(found)
}

fn parse_solve(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag { name: "--problem", takes_value: true },
        Flag { name: "--seed", takes_value: true },
        Flag { name: "--output", takes_value: true },
        Flag { name: "--help", takes_value: false },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem = flag_value(&parsed, "--problem")?
        .ok_or("solve requires --problem <file>")?;
    let seed = match flag_value(&parsed, "--seed")? {
        Some(raw) => Some(
            raw.parse::<u64>()
                .map_err(|error| format!("invalid seed '{raw}': {error}"))?,
        ),
        None => None,
    };
    let output = flag_value(&parsed, "--output")?.map(PathBuf::from);
    Ok(Command::Solve(SolveArgs {
        problem: PathBuf::from(problem),
        seed,
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

fn parse_export(tokens: &[String]) -> Result<Command, String> {
    const FLAGS: &[Flag] = &[
        Flag { name: "--problem", takes_value: true },
        Flag { name: "--solution", takes_value: true },
        Flag { name: "--format", takes_value: true },
        Flag { name: "--output", takes_value: true },
        Flag { name: "--help", takes_value: false },
    ];
    let parsed = parse_flags(tokens, FLAGS)?;
    if parsed.iter().any(|(name, _)| name == "--help") {
        return Ok(Command::Help);
    }
    let problem = flag_value(&parsed, "--problem")?
        .ok_or("export requires --problem <file>")?;
    let solution = flag_value(&parsed, "--solution")?
        .ok_or("export requires --solution <file>")?;
    let format = ExportFormat::parse(
        flag_value(&parsed, "--format")?
            .ok_or("export requires --format <svg|html|png|pdf>")?,
    )?;
    let output = flag_value(&parsed, "--output")?
        .ok_or("export requires --output <file>")?;
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
        assert_eq!(parse_args(&args_of(&["--version"])).unwrap(), Command::Version);
        assert_eq!(parse_args(&args_of(&["-V"])).unwrap(), Command::Version);
        assert_eq!(parse_args(&args_of(&["version"])).unwrap(), Command::Version);
        // --help inside a subcommand shows help too.
        assert_eq!(parse_args(&args_of(&["solve", "--help"])).unwrap(), Command::Help);
        assert_eq!(parse_args(&args_of(&["export", "--help"])).unwrap(), Command::Help);
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
            "solve", "--problem=p.json", "--seed", "7", "--output=out.json",
        ]))
        .unwrap()
        else {
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
        assert!(error.contains("unknown option '--bogus'"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_missing_option_value() {
        let error = parse_args(&args_of(&["solve", "--problem"])).unwrap_err();
        assert!(error.contains("requires a value"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_bad_seed() {
        let error = parse_args(&args_of(&["solve", "--problem", "p", "--seed", "abc"])).unwrap_err();
        assert!(error.contains("invalid seed 'abc'"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_unknown_command() {
        let error = parse_args(&args_of(&["frobnicate"])).unwrap_err();
        assert!(error.contains("unknown command 'frobnicate'"), "unexpected error: {error}");
    }

    #[test]
    fn parse_export_with_all_options() {
        let Command::Export(args) = parse_args(&args_of(&[
            "export", "--problem", "p.json", "--solution", "s.json", "--format", "svg", "--output",
            "o.svg",
        ]))
        .unwrap()
        else {
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
            "export", "--problem=p", "--solution=s", "--format=html", "--output=o",
        ]))
        .unwrap()
        else {
            panic!("expected an export command");
        };
        assert_eq!(args.format, ExportFormat::Html);
    }

    #[test]
    fn rejects_bad_format() {
        let error = parse_args(&args_of(&[
            "export", "--problem", "p", "--solution", "s", "--format", "bmp", "--output", "o",
        ]))
        .unwrap_err();
        assert!(error.contains("unknown format 'bmp'"), "unexpected error: {error}");
    }

    #[test]
    fn parse_export_png_and_pdf_formats() {
        let Command::Export(args) = parse_args(&args_of(&[
            "export", "--problem=p", "--solution=s", "--format=png", "--output=o",
        ]))
        .unwrap()
        else {
            panic!("expected an export command");
        };
        assert_eq!(args.format, ExportFormat::Png);

        let Command::Export(args) = parse_args(&args_of(&[
            "export", "--problem=p", "--solution=s", "--format", "PDF", "--output", "o",
        ]))
        .unwrap()
        else {
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
        assert!(error.contains("not valid UTF-8"), "unexpected error: {error}");
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
        assert_eq!(exit_code_for(classify_solve_error("unsupported api_version 99")), 2);
        assert_eq!(exit_code_for(classify_solve_error("native solve requires at least one seat")), 2);
        assert_eq!(exit_code_for(classify_solve_error("Duplicate student identifiers: STU001")), 2);
        // Internal faults exit 70.
        assert_eq!(exit_code_for(classify_solve_error("solver panicked while ranking")), 70);
    }
}
