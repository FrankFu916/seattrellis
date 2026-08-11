//! Colored usage text rendered through a `Styler`.
//!
//! The layout mirrors the Python CLI's Typer help: a bold title, bold section
//! headers, cyan command names, and bold flags.

use crate::style::Styler;

pub fn render_usage(styler: &Styler) -> String {
    let mut out = String::new();
    out.push_str(&styler.bold("SeatTrellis CLI"));
    out.push_str(" — standalone solve + export tool.\n\n");

    out.push_str(&styler.bold("USAGE:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push_str(" <COMMAND> [OPTIONS]\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push_str(" --version\n\n");

    out.push_str(&styler.bold("COMMANDS:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("validate"));
    out.push_str(" Check a solve-request JSON without running the search.\n    ");
    out.push_str(&styler.cyan("precheck"));
    out.push_str("  Report candidate seat domains and infeasibility reasons.\n    ");
    out.push_str(&styler.cyan("audit"));
    out.push_str("     Audit a solved plan: hard-rule status + soft breakdown.\n    ");
    out.push_str(&styler.cyan("candidates"));
    out.push_str("  Generate a diverse candidate set.\n    ");
    out.push_str(&styler.cyan("history-report"));
    out.push_str("  Summarize historical seating snapshots.\n    ");
    out.push_str(&styler.cyan("pair-report"));
    out.push_str("     Summarize historical desk-mate / neighbor pairs.\n    ");
    out.push_str(&styler.cyan("repair"));
    out.push_str("    Re-solve a snapshot while preserving anchors.\n    ");
    out.push_str(&styler.cyan("project-info"));
    out.push_str("  Show a project workspace summary.\n    ");
    out.push_str(&styler.cyan("project-validate"));
    out.push_str(" Validate a project and its files.\n    ");
    out.push_str(&styler.cyan("project-solve"));
    out.push_str("   Solve a project workspace.\n    ");
    out.push_str(&styler.cyan("project-export"));
    out.push_str("  Export a project plan.\n    ");
    out.push_str(&styler.cyan("project-rotate"));
    out.push_str("  Generate future seating periods for a project.\n    ");
    out.push_str(&styler.cyan("project-edit"));
    out.push_str("    Apply manual edits to a project seating artifact.\n    ");
    out.push_str(&styler.cyan("project-repair"));
    out.push_str("  Re-solve a project artifact preserving anchors.\n    ");
    out.push_str(&styler.cyan("schema-list"));
    out.push_str("    List the v2 artifact registry.\n    ");
    out.push_str(&styler.cyan("schema-export"));
    out.push_str("   Write the JSON Schema for one artifact kind.\n    ");
    out.push_str(&styler.cyan("schema-migrate"));
    out.push_str("  Validate and rewrite a versioned JSON artifact.\n    ");
    out.push_str(&styler.cyan("solve"));
    out.push_str("    Solve a seating problem and print a summary of the result.\n    ");
    out.push_str(&styler.cyan("export"));
    out.push_str("   Render a solved seating plan as SVG, HTML, PNG, or PDF.\n    ");
    out.push_str(&styler.cyan("help"));
    out.push_str("     Show this help.\n\n");

    out.push_str(&styler.bold("VALIDATE:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("validate"));
    out.push_str(" --problem <problem.json>\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>  Solve-request JSON (CoreSolveRequest). Required.\n\n");

    out.push_str(&styler.bold("PRECHECK:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("precheck"));
    out.push_str(" --problem <problem.json>\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>  Solve-request JSON (CoreSolveRequest). Required.\n\n");

    out.push_str(&styler.bold("AUDIT:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("audit"));
    out.push_str(" --problem <problem.json> --solution <result.json>\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>   The solve-request JSON used for solve. Required.\n      ");
    out.push_str(&styler.bold("--solution"));
    out.push_str(" <file>  The solve result JSON (CoreSolveResponse). Required.\n\n");

    out.push_str(&styler.bold("SCORE:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("score"));
    out.push_str(" --problem <problem.json> --assignment <json> [--latest-snapshot <file>] [--diversity <f>]\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>    Solve-request JSON (CoreSolveRequest). Required.\n      ");
    out.push_str(&styler.bold("--assignment"));
    out.push_str(" <json>  Inline [[student, seat], ...] index pairs. Required.\n      ");
    out.push_str(&styler.bold("--latest-snapshot"));
    out.push_str(" <file>  Optional snapshot for the stability dimension.\n      ");
    out.push_str(&styler.bold("--diversity"));
    out.push_str(" <f>       Optional diversity score for the diversity dimension.\n\n");

    out.push_str(&styler.bold("CANDIDATES:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("candidates"));
    out.push_str(" --problem <problem.json> [--count <n>]\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>  Solve-request JSON (CoreSolveRequest). Required.\n      ");
    out.push_str(&styler.bold("--count"));
    out.push_str(" <n>        Candidate set size (1-20, default 5).\n\n");

    out.push_str(&styler.bold("HISTORY-REPORT:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("history-report"));
    out.push_str(" --problem <problem.json> --history <snapshot.json>...\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>   Solve-request JSON providing students + layout. Required.\n      ");
    out.push_str(&styler.bold("--history"));
    out.push_str(" <file>  Snapshot JSON (repeatable). Required.\n\n");

    out.push_str(&styler.bold("PAIR-REPORT:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("pair-report"));
    out.push_str(" --problem <problem.json> --history <snapshot.json>... [--top <n>]\n");
    out.push_str("                           [--within-distance <n>]\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>   Solve-request JSON providing students + layout. Required.\n      ");
    out.push_str(&styler.bold("--history"));
    out.push_str(" <file>  Snapshot JSON (repeatable). Required.\n      ");
    out.push_str(&styler.bold("--top"));
    out.push_str(" <n>        High-frequency pairs to display (default 10).\n      ");
    out.push_str(&styler.bold("--within-distance"));
    out.push_str(" <n>  Chebyshev distance threshold (default 2).\n\n");

    out.push_str(&styler.bold("REPAIR:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("repair"));
    out.push_str(" --problem <problem.json> --snapshot <snapshot.json>\n");
    out.push_str("              [--lock-student <key>]... [--lock-seat <seat>]... [--affected <key>]... [--output <file>]\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>    Solve-request JSON (students/layout/rules). Required.\n      ");
    out.push_str(&styler.bold("--snapshot"));
    out.push_str(" <file>  Current snapshot JSON (assignments). Required.\n      ");
    out.push_str(&styler.bold("--lock-student"));
    out.push_str(" <key>   Student that keeps its current seat (repeatable).\n      ");
    out.push_str(&styler.bold("--lock-seat"));
    out.push_str(" <seat>    Seat whose occupant keeps it (repeatable).\n      ");
    out.push_str(&styler.bold("--affected"));
    out.push_str(" <key>    Bounds the re-solve scope (repeatable).\n      ");
    out.push_str(&styler.bold("--output"));
    out.push_str(" <file>   Write the repaired snapshot (default: stdout).\n\n");

    out.push_str(&styler.bold("PROJECT:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push_str(
        " project-info|project-validate|project-solve|project-export --project <project.json>\n",
    );
    out.push_str("      ");
    out.push_str(&styler.bold("--project"));
    out.push_str(" <file>  Portable project workspace file. Required.\n      ");
    out.push_str(&styler.bold("--seed"));
    out.push_str(" <n>        Override the project's solver seed.\n      ");
    out.push_str(&styler.bold("--format"));
    out.push_str(" <f>       project-export only: svg|html|png|pdf.\n      ");
    out.push_str(&styler.bold("--output"));
    out.push_str(" <file>   project-solve/export write their artifact here.\n      ");
    out.push_str(&styler.bold("--snapshot"));
    out.push_str(" <file>  project-export only: the saved plan to render (the\n      ");
    out.push_str("        result of 'project-solve --output <snapshot.json>'). Exporting\n      ");
    out.push_str("        never re-solves; it renders exactly the saved plan.\n\n");

    out.push_str(&styler.bold("SOLVE:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("solve"));
    out.push_str(
        " --problem <problem.json> [--seed <n>] [--time-limit <sec>] [--output <result.json>]\n\n",
    );
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>  Solve-request JSON (CoreSolveRequest). Required.\n      ");
    out.push_str(&styler.bold("--seed"));
    out.push_str(" <n>        Override the problem's solver seed.\n      ");
    out.push_str(&styler.bold("--time-limit"));
    out.push_str(" <sec>   Wall-clock budget; exhausted searches report Timeout.\n      ");
    out.push_str(&styler.bold("--output"));
    out.push_str(" <file>   Also write the full result JSON (CoreSolveResponse) to <file>.\n\n");

    out.push_str(&styler.bold("EXPORT:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("export"));
    out.push_str(" --problem <problem.json> --solution <result.json> \\\n                           --format <svg|html|png|pdf> --output <file>\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(
        " <file>   The same solve-request JSON used for solve (seat grid). Required.\n      ",
    );
    out.push_str(&styler.bold("--solution"));
    out.push_str(" <file>  The solve result JSON (CoreSolveResponse). Required.\n      ");
    out.push_str(&styler.bold("--format"));
    out.push_str(" <f>       Output format: ");
    out.push_str(&styler.cyan("svg"));
    out.push_str(", ");
    out.push_str(&styler.cyan("html"));
    out.push_str(", ");
    out.push_str(&styler.cyan("png"));
    out.push_str(", or ");
    out.push_str(&styler.cyan("pdf"));
    out.push_str(". Required.\n      ");
    out.push_str(&styler.bold("--output"));
    out.push_str(" <file>    Write the rendered plan to <file>. Required.\n\n");

    out.push_str(&styler.bold("EXIT STATUS:"));
    out.push_str("\n    0 on success; 1 on error (bad arguments or unreadable input files).\n    An infeasible solve is a valid result and still exits 0.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Styler;

    #[test]
    fn plain_usage_contains_all_sections() {
        let text = render_usage(&Styler::for_stream(false));
        assert!(text.contains("USAGE:"));
        assert!(text.contains("COMMANDS:"));
        assert!(text.contains("PRECHECK:"));
        assert!(text.contains("AUDIT:"));
        assert!(text.contains("CANDIDATES:"));
        assert!(text.contains("HISTORY-REPORT:"));
        assert!(text.contains("PAIR-REPORT:"));
        assert!(text.contains("REPAIR:"));
        assert!(text.contains("PROJECT:"));
        assert!(text.contains("SOLVE:"));
        assert!(text.contains("EXPORT:"));
        assert!(text.contains("EXIT STATUS:"));
        assert!(text.contains("solve"));
        assert!(text.contains("precheck"));
        assert!(text.contains("audit"));
        assert!(text.contains("candidates"));
        assert!(text.contains("history-report"));
        assert!(text.contains("pair-report"));
        assert!(text.contains("repair"));
        assert!(text.contains("project-solve"));
        assert!(text.contains("project-export"));
        assert!(text.contains("export"));
    }

    #[test]
    fn colored_usage_wraps_command_names() {
        let text = render_usage(&Styler::for_stream(true));
        assert!(text.contains("\x1b[36msolve\x1b[0m"));
        assert!(text.contains("\x1b[1mCOMMANDS:\x1b[0m"));
    }
}
