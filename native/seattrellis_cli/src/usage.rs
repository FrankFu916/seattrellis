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
    out.push_str(&styler.cyan("solve"));
    out.push_str("    Solve a seating problem and print a summary of the result.\n    ");
    out.push_str(&styler.cyan("export"));
    out.push_str("   Render a solved seating plan as SVG, HTML, PNG, or PDF.\n    ");
    out.push_str(&styler.cyan("help"));
    out.push_str("     Show this help.\n\n");

    out.push_str(&styler.bold("SOLVE:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push_str(" ");
    out.push_str(&styler.cyan("solve"));
    out.push_str(" --problem <problem.json> [--seed <n>] [--output <result.json>]\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>  Solve-request JSON (CoreSolveRequest). Required.\n      ");
    out.push_str(&styler.bold("--seed"));
    out.push_str(" <n>        Override the problem's solver seed.\n      ");
    out.push_str(&styler.bold("--output"));
    out.push_str(" <file>   Also write the full result JSON (CoreSolveResponse) to <file>.\n\n");

    out.push_str(&styler.bold("EXPORT:"));
    out.push_str("\n    ");
    out.push_str(&styler.cyan("seattrellis_cli"));
    out.push_str(" ");
    out.push_str(&styler.cyan("export"));
    out.push_str(" --problem <problem.json> --solution <result.json> \\\n                           --format <svg|html|png|pdf> --output <file>\n\n");
    out.push_str("      ");
    out.push_str(&styler.bold("--problem"));
    out.push_str(" <file>   The same solve-request JSON used for solve (seat grid). Required.\n      ");
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
        assert!(text.contains("SOLVE:"));
        assert!(text.contains("EXPORT:"));
        assert!(text.contains("EXIT STATUS:"));
        assert!(text.contains("solve"));
        assert!(text.contains("export"));
    }

    #[test]
    fn colored_usage_wraps_command_names() {
        let text = render_usage(&Styler::for_stream(true));
        assert!(text.contains("\x1b[36msolve\x1b[0m"));
        assert!(text.contains("\x1b[1mCOMMANDS:\x1b[0m"));
    }
}
