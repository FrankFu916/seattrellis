#!/usr/bin/env python3
"""Expose bounded native compiler/test diagnostics in public CI annotations."""
from __future__ import annotations

import re
import sys
from pathlib import Path

from report_rust_test_failure import annotation_data


def failure_diagnostics(output: str) -> list[str]:
    diagnostics = []
    for line in output.splitlines():
        if re.search(r"^.*\.swift:\d+(?::\d+)?: error:|^error:", line):
            diagnostics.append(line[:1000])
        elif re.search(r"^Test Case .+ failed", line):
            diagnostics.append(line[:1000])
    return list(dict.fromkeys(diagnostics))[:12] or [
        "Native macOS check failed; inspect the build log for the underlying tool error."
    ]


def main() -> int:
    try:
        output = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
        for message in failure_diagnostics(output):
            print(f"::error title=Native macOS check failure::{annotation_data(message)}")
    except (OSError, IndexError) as error:
        print(f"Could not publish native diagnostics: {error}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
