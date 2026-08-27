//! Colored usage text rendered through a `Styler`.
//!
//! The layout mirrors the Python CLI's Typer help: a bold title, bold section
//! headers, cyan command names, and bold flags.

use crate::style::Styler;

pub fn render_usage(styler: &Styler) -> String {
    let mut out = String::new();

    // ASCII mark of the SeatTrellis brand: a ring of seat tiles with one
    // empty (dashed) seat that the rotation arrow points into. Plain ASCII
    // only, so it survives every terminal font; color is applied through the
    // styler and only on a TTY.
    let accent = |text: &str| styler.cyan(text);
    let dim = |text: &str| styler.yellow(text);
    let seat = |text: &str| styler.bold(text);
    let mut mark = String::new();
    // top row: three filled seats
    mark.push_str(&format!("  {} {} {}\n", accent("+--+ +--+ +--+"), "", ""));
    mark.push_str(&format!("  {} {} {}\n", accent("|##| |##| |##|"), "", ""));
    // middle: arc down the right side, arrow pointing to the empty seat
    mark.push_str(&format!("  {}      {}\n", accent("+--+"), accent("\\")));
    mark.push_str(&format!(
        "  {}       {}   {}\n",
        seat("+--+"),
        accent("o"),
        accent("\\")
    ));
    mark.push_str(&format!(
        "  {} {}    {}\n",
        seat("|##|"),
        dim("+--+ +--+"),
        accent("/")
    ));
    mark.push_str(&format!("  {} {}\n", seat("+--+"), dim("|..| |..|")));
    mark.push_str(&format!("     {}\n", dim("+--+ +--+")));
    out.push_str(&mark);
    out.push_str(&styler.bold("  S e a t T r e l l i s"));
    out.push_str("\n     classroom seating, solved — not negotiated\n\n");

    out.push_str(&styler.bold("USAGE:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push_str(" <COMMAND> [OPTIONS]\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push_str(" --version\n\n");

    out.push_str(&styler.bold("COMMANDS:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("doctor"));
    out.push_str("       Check the environment (binary/version/temp dir).\n    ");
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
    out.push_str(&styler.cyan("edit"));
    out.push_str("       Apply manual edit operations to a snapshot or candidate set.\n    ");
    out.push_str(&styler.cyan("score"));
    out.push_str("      Score a fixed assignment with the PlanScore breakdown.\n    ");
    out.push_str(&styler.cyan("project-init"));
    out.push_str("  Create a project workspace file.\n    ");
    out.push_str(&styler.cyan("project-list"));
    out.push_str("   List recent projects under a root.\n    ");
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
    out.push_str(&styler.cyan("project-privacy"));
    out.push_str(" Scan a project for sensitive fields.\n    ");
    out.push_str(&styler.cyan("project-pack"));
    out.push_str("    Back up a project as a .seattrellis.zip bundle.\n    ");
    out.push_str(&styler.cyan("project-restore"));
    out.push_str(" Restore a project bundle.\n    ");
    out.push_str(&styler.cyan("schema-list"));
    out.push_str("    List the v2 artifact registry.\n    ");
    out.push_str(&styler.cyan("schema-export"));
    out.push_str("   Write the JSON Schema for one artifact kind.\n    ");
    out.push_str(&styler.cyan("schema-migrate"));
    out.push_str("  Validate and rewrite a versioned JSON artifact.\n    ");
    out.push_str(&styler.cyan("solve"));
    out.push_str("    Solve a seating problem and print a summary of the result.\n    ");
    out.push_str(&styler.cyan("export"));
    out.push_str("   Render a solved seating plan as SVG, HTML, PNG, PDF, XLSX, DOCX,\n    ");
    out.push_str("or PPTX.\n    ");
    out.push_str(&styler.cyan("help"));
    out.push_str("     Show this help.\n\n");

    out.push_str(&styler.bold("DOCTOR:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("doctor"));
    out.push_str("\n\n      Prints the binary name, version, core API version and a\n      temp-dir writability probe (fails with exit 2 when not writable).\n\n");

    out.push_str(&styler.bold("EDIT:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("edit"));
    out.push_str(" --snapshot <snapshot.json> --operation <op>... [--candidate <id>]\n");
    out.push_str("             [--operations-file <file>] [--output <file>] [--strict]\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--snapshot"));
    out.push_str(" <file>  Snapshot or candidate-set JSON. Required.\n      ");
    out.push_str(&styler.bold("--candidate"));
    out.push_str(" <id>   Candidate ID for a candidate set (default: recommended).\n      ");
    out.push_str(&styler.bold("--operation"));
    out.push_str(
        " <op>    String operation, repeatable and ordered. Examples: swap:STU001:STU002,\n",
    );
    out.push_str(
        "             move:STU003:R2C2, batch-move:STU001=R1C2,STU002=R1C1, unseat:STU004,\n",
    );
    out.push_str("             lock-seat:R1C1, lock-student:STU001, unlock-seat:R1C1.\n      ");
    out.push_str(&styler.bold("--operations-file"));
    out.push_str(" <file> JSON operation log applied before --operation values.\n      ");
    out.push_str(&styler.bold("--strict"));
    out.push_str("       Fail instead of writing when hard constraints are violated.\n\n");

    out.push_str(&styler.bold("VALIDATE:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("validate"));
    out.push_str(" --problem <problem.json> [--preset <name>] [--history <snapshot.json>]...\n");
    out.push_str("             [--history-dir <dir>] [--strict]\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>  Solve-request JSON (CoreSolveRequest). Required.\n      ");
    out.push_str(&styler.bold("--preset"));
    out.push_str(" <name>  Preset name for preset-context warnings.\n      ");
    out.push_str(&styler.bold("--history"));
    out.push_str(" <file>  History snapshot counted for preset history warnings.\n      ");
    out.push_str(&styler.bold("--history-dir"));
    out.push_str(" <dir>  Directory scanned for *.snapshot.json files (joins --history).\n      ");
    out.push_str(&styler.bold("--strict"));
    out.push_str("       Treat warnings as validation failures.\n\n");

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
    out.push_str(" --problem <problem.json> [--history <snapshot.json>]... [--history-dir <dir>] [--output <file>]\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>   Solve-request JSON providing students + layout. Required.\n      ");
    out.push_str(&styler.bold("--history"));
    out.push_str(" <file>  Snapshot JSON (repeatable).\n      ");
    out.push_str(&styler.bold("--history-dir"));
    out.push_str(" <dir>  Directory scanned for *.snapshot.json files.\n      ");
    out.push_str(&styler.bold("--output"));
    out.push_str(" <file>   Also write the JSON report to a file.\n\n");

    out.push_str(&styler.bold("PAIR-REPORT:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push(' ');
    out.push_str(&styler.cyan("pair-report"));
    out.push_str(
        " --problem <problem.json> [--history <snapshot.json>]... [--history-dir <dir>]\n",
    );
    out.push_str("                           [--top <n>] [--within-distance <n>]\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>   Solve-request JSON providing students + layout. Required.\n      ");
    out.push_str(&styler.bold("--history"));
    out.push_str(" <file>  Snapshot JSON (repeatable).\n      ");
    out.push_str(&styler.bold("--history-dir"));
    out.push_str(" <dir>  Directory scanned for *.snapshot.json files.\n      ");
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
    out.push_str(
        "              [--lock-student <key>]... [--lock-seat <seat>]... [--affected <key>]...\n",
    );
    out.push_str("              [--ignore-saved-locks] [--output <file>]\n\n");
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
    out.push_str(&styler.bold("--ignore-saved-locks"));
    out.push_str("  Do not reuse locks persisted in the snapshot metadata.\n      ");
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
    out.push_str(&styler.bold("--strict"));
    out.push_str("       project-validate only: treat warnings as failures.\n      ");
    out.push_str(&styler.bold("--candidates"));
    out.push_str(" <n>    project-solve only: candidate count 1-20 (default: the\n      ");
    out.push_str("        project's default_candidates).\n      ");
    out.push_str(&styler.bold("--report"));
    out.push_str(" <file>  project-solve only: also write a plan comparison report.\n      ");
    out.push_str(&styler.bold("--format"));
    out.push_str(
        " <f>       project-export only: svg|html|print-html|png|pdf|xlsx|docx|pptx\n      ",
    );
    out.push_str("        (default: the project's default_export_format).\n      ");
    out.push_str(&styler.bold("--template"));
    out.push_str(" <t>    project-export only: teacher (default; real names, ids and\n      ");
    out.push_str("        detail fields) or public (anonymized wall copy: no names,\n      ");
    out.push_str("        no student ids, no height/vision).\n      ");
    out.push_str(&styler.bold("--orientation"));
    out.push_str(" <o> project-export only: portrait|landscape|auto\n      ");
    out.push_str("        (default auto: print-html prints landscape A4, other\n      ");
    out.push_str("        formats portrait).\n      ");
    out.push_str(&styler.bold("--locale"));
    out.push_str(" <l>    project-export only: export text language, en (default)\n      ");
    out.push_str("        or zh.\n      ");
    out.push_str(&styler.bold("--candidate"));
    out.push_str(" <id>   project-export only: candidate ID for a candidate-set\n      ");
    out.push_str("        snapshot (default: recommended).\n      ");
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
    out.push_str(" --problem <problem.json> --solution <result.json> \\\n                           --format <svg|html|png|pdf|xlsx|docx|pptx> --output <file>\n\n");
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
    out.push_str(", ");
    out.push_str(&styler.cyan("pdf"));
    out.push_str(", ");
    out.push_str(&styler.cyan("xlsx"));
    out.push_str(", ");
    out.push_str(&styler.cyan("docx"));
    out.push_str(", or ");
    out.push_str(&styler.cyan("pptx"));
    out.push_str(". Required.\n      ");
    out.push_str(&styler.bold("--output"));
    out.push_str(" <file>    Write the rendered plan to <file>. Required.\n\n");

    out.push_str(&styler.bold("EXIT STATUS:"));
    out.push_str("\n    0 on success; 2 on invalid input/arguments; 3 infeasible; 4 timeout;\n    5 unknown; 70 internal error; 130 cancelled (frozen v2 table, plan M1-03).\n");
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
        assert!(text.contains("DOCTOR:"));
        assert!(text.contains("EDIT:"));
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
        assert!(text.contains("edit"));
        assert!(text.contains("doctor"));
        assert!(text.contains("project-solve"));
        assert!(text.contains("project-export"));
        assert!(text.contains("project-privacy"));
        assert!(text.contains("project-pack"));
        assert!(text.contains("project-restore"));
        assert!(text.contains("export"));
    }

    #[test]
    fn colored_usage_wraps_command_names() {
        let text = render_usage(&Styler::for_stream(true));
        assert!(text.contains("\x1b[36msolve\x1b[0m"));
        assert!(text.contains("\x1b[1mCOMMANDS:\x1b[0m"));
    }
}
