//! Minimal ANSI styling with automatic terminal detection.
//!
//! Colors are emitted only when the relevant stream is a terminal, so piped
//! or redirected output stays plain.  The palette mirrors the Python CLI's
//! Typer styling: cyan commands, bold section headers, red errors.

use std::io::IsTerminal;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

/// Applies ANSI colors to a stream when that stream is a terminal.
#[derive(Clone, Copy)]
pub struct Styler {
    enabled: bool,
}

impl Styler {
    /// Colors based on whether stdout is a terminal.
    pub fn stdout() -> Self {
        Self::for_stream(std::io::stdout().is_terminal())
    }

    /// Colors based on whether stderr is a terminal.
    pub fn stderr() -> Self {
        Self::for_stream(std::io::stderr().is_terminal())
    }

    pub(crate) fn for_stream(enabled: bool) -> Self {
        Self { enabled }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("{code}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    pub fn bold(&self, text: &str) -> String {
        self.paint(BOLD, text)
    }

    pub fn cyan(&self, text: &str) -> String {
        self.paint(CYAN, text)
    }

    pub fn green(&self, text: &str) -> String {
        self.paint(GREEN, text)
    }

    pub fn yellow(&self, text: &str) -> String {
        self.paint(YELLOW, text)
    }

    pub fn red(&self, text: &str) -> String {
        self.paint(RED, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_styler_returns_plain_text() {
        let styler = Styler::for_stream(false);
        assert_eq!(styler.bold("x"), "x");
        assert_eq!(styler.cyan("solve"), "solve");
        assert_eq!(styler.red("error"), "error");
    }

    #[test]
    fn enabled_styler_wraps_ansi_codes() {
        let styler = Styler::for_stream(true);
        assert_eq!(styler.cyan("solve"), "\x1b[36msolve\x1b[0m");
        assert_eq!(styler.red("error"), "\x1b[31merror\x1b[0m");
    }
}
