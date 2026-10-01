#!/usr/bin/env python3
"""Expose bounded Rust test diagnostics while preserving the caller's exit code."""
from __future__ import annotations

import os
import re
import sys
from pathlib import Path


def failure_diagnostics(output: str) -> list[str]:
    failed = re.findall(r"^test (.+?) \.\.\. FAILED\s*$", output, re.MULTILINE)
    diagnostics = [f"Failed Rust test: {name}" for name in dict.fromkeys(failed)]
    # Assertion locations and compiler errors identify the next investigation
    # without publishing entire captured request bodies or arbitrary test output.
    for line in output.splitlines():
        if re.search(r"^thread .+ panicked at |^error(?:\[|:)|^assertion .+ failed", line):
            diagnostics.append(line[:1000])
    return list(dict.fromkeys(diagnostics))[:12] or [
        "Rust workspace tests failed before a test name or compiler diagnostic was captured; inspect the job log."
    ]


def annotation_data(value: str) -> str:
    return value.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def main() -> int:
    try:
        output = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
        diagnostics = failure_diagnostics(output)
        for message in diagnostics:
            print(f"::error title=Rust workspace test failure::{annotation_data(message)}")
        destination = os.environ.get("GITHUB_STEP_SUMMARY")
        if destination:
            with Path(destination).open("a", encoding="utf-8") as summary:
                summary.write("### Rust workspace test failure\n\n")
                summary.write("\n".join(f"- {message}" for message in diagnostics) + "\n")
    except (OSError, IndexError) as error:
        print(f"Could not publish Rust test diagnostics: {error}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
