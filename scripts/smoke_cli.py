#!/usr/bin/env python
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable, Sequence


@dataclass(frozen=True)
class SmokeCommand:
    name: str
    args: list[str]
    outputs: list[str]


@dataclass(frozen=True)
class SmokeResult:
    name: str
    elapsed_seconds: float
    outputs: list[str]


def main() -> None:
    args = _parse_args()
    repo_root = Path(__file__).resolve().parents[1]
    command = shlex.split(args.command) if args.command else [
        sys.executable,
        "-m",
        "seattrellis.cli",
    ]
    workdir, cleanup = _prepare_workdir(args.workdir)
    try:
        results = run_smoke(
            command=command,
            workdir=workdir,
            repo_root=repo_root,
            optional=args.optional,
            backends=_parse_backends(args.backends),
            time_limit=args.time_limit,
            include_pdf=args.include_pdf,
        )
        if args.json_report:
            report_path = Path(args.json_report)
            report_path.parent.mkdir(parents=True, exist_ok=True)
            report_path.write_text(
                json.dumps(
                    {
                        "workdir": str(workdir),
                        "command": command,
                        "results": [asdict(result) for result in results],
                    },
                    ensure_ascii=False,
                    indent=2,
                )
                + "\n",
                encoding="utf-8",
            )
        print(f"CLI smoke completed: {len(results)} commands passed in {workdir}")
    finally:
        if cleanup and not args.keep_workdir:
            shutil.rmtree(workdir, ignore_errors=True)


def run_smoke(
    *,
    command: Sequence[str],
    workdir: Path,
    repo_root: Path,
    optional: str,
    backends: list[str],
    time_limit: float,
    include_pdf: bool,
) -> list[SmokeResult]:
    commands = _commands(
        optional=optional,
        backends=backends,
        time_limit=time_limit,
        include_pdf=include_pdf,
    )
    env = _subprocess_env(repo_root)
    results: list[SmokeResult] = []
    for index, item in enumerate(commands, start=1):
        print(f"[{index:02d}/{len(commands):02d}] {item.name}")
        started = time.perf_counter()
        completed = subprocess.run(
            [*command, *item.args],
            cwd=workdir,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        elapsed = round(time.perf_counter() - started, 3)
        if completed.returncode != 0:
            raise SystemExit(
                _format_failure(
                    command=[*command, *item.args],
                    cwd=workdir,
                    completed=completed,
                )
            )
        _check_outputs(workdir, item.outputs)
        results.append(
            SmokeResult(
                name=item.name,
                elapsed_seconds=elapsed,
                outputs=item.outputs,
            )
        )
    return results


def _commands(
    *,
    optional: str,
    backends: list[str],
    time_limit: float,
    include_pdf: bool,
) -> list[SmokeCommand]:
    commands = [
        SmokeCommand("show help", ["--help"], []),
        SmokeCommand("initialize demo", ["init-demo", "--force"], [
            "examples/students.csv",
            "examples/classroom.json",
            "examples/project.seattrellis.json",
        ]),
        SmokeCommand("run doctor", ["doctor"], []),
        SmokeCommand("list presets", ["presets", "list"], []),
        SmokeCommand("show daily preset", ["presets", "show", "daily"], []),
        SmokeCommand(
            "export daily preset",
            ["presets", "export", "daily", "--output", "outputs/daily.rules.json"],
            ["outputs/daily.rules.json"],
        ),
        SmokeCommand("list JSON Schemas", ["schema", "list"], []),
        SmokeCommand(
            "export JSON Schemas",
            ["schema", "export", "--output-dir", "outputs/schemas"],
            [
                "outputs/schemas/project.schema.json",
                "outputs/schemas/seating-snapshot.schema.json",
            ],
        ),
        SmokeCommand(
            "migrate snapshot schema",
            [
                "schema",
                "migrate",
                "--input",
                "examples/history/week1.snapshot.json",
                "--output",
                "outputs/week1.migrated.snapshot.json",
            ],
            ["outputs/week1.migrated.snapshot.json"],
        ),
        SmokeCommand(
            "validate daily preset",
            [
                "validate",
                "--students",
                "examples/students.csv",
                "--layout",
                "examples/classroom.json",
                "--preset",
                "daily",
                "--history-dir",
                "examples/history",
            ],
            [],
        ),
    ]
    for backend in backends:
        commands.append(
            SmokeCommand(
                f"solve daily preset with {backend}",
                [
                    "solve",
                    "--students",
                    "examples/students.csv",
                    "--layout",
                    "examples/classroom.json",
                    "--preset",
                    "daily",
                    "--history-dir",
                    "examples/history",
                    "--backend",
                    backend,
                    "--time-limit",
                    str(time_limit),
                    "--output",
                    f"outputs/daily-{backend}.snapshot.json",
                ],
                [f"outputs/daily-{backend}.snapshot.json"],
            )
        )
    commands.extend(
        [
            SmokeCommand(
                "read project",
                ["project-info", "--project", "examples/project.seattrellis.json"],
                [],
            ),
            SmokeCommand(
                "validate project",
                ["project-validate", "--project", "examples/project.seattrellis.json"],
                [],
            ),
            SmokeCommand(
                "solve project candidates",
                [
                    "project-solve",
                    "--project",
                    "examples/project.seattrellis.json",
                    "--candidates",
                    "3",
                    "--backend",
                    "fallback",
                    "--time-limit",
                    str(time_limit),
                    "--output",
                    "outputs/project.candidates.json",
                    "--report",
                    "outputs/project-plan-report.json",
                ],
                ["outputs/project.candidates.json", "outputs/project-plan-report.json"],
            ),
            SmokeCommand(
                "export project html",
                [
                    "project-export",
                    "--project",
                    "examples/project.seattrellis.json",
                    "--snapshot",
                    "outputs/project.candidates.json",
                    "--candidate",
                    "recommended",
                    "--format",
                    "html",
                    "--output",
                    "outputs/project-recommended.html",
                ],
                ["outputs/project-recommended.html"],
            ),
            SmokeCommand(
                "validate explicit rules",
                [
                    "validate",
                    "--students",
                    "examples/students.csv",
                    "--layout",
                    "examples/classroom.json",
                    "--rules",
                    "examples/rules.json",
                ],
                [],
            ),
            SmokeCommand(
                "history report",
                [
                    "history-report",
                    "--students",
                    "examples/students.csv",
                    "--layout",
                    "examples/classroom.json",
                    "--history-dir",
                    "examples/history",
                ],
                [],
            ),
            SmokeCommand(
                "pair report",
                [
                    "pair-report",
                    "--students",
                    "examples/students.csv",
                    "--layout",
                    "examples/classroom.json",
                    "--history-dir",
                    "examples/history",
                ],
                [],
            ),
            SmokeCommand(
                "solve neighbor rules",
                [
                    "solve",
                    "--students",
                    "examples/students.csv",
                    "--layout",
                    "examples/classroom.json",
                    "--rules",
                    "examples/rules_neighbor_avoidance.json",
                    "--history-dir",
                    "examples/history",
                    "--backend",
                    "fallback",
                    "--time-limit",
                    str(time_limit),
                    "--output",
                    "outputs/neighbor-aware.snapshot.json",
                ],
                ["outputs/neighbor-aware.snapshot.json"],
            ),
            SmokeCommand(
                "export neighbor html",
                [
                    "export",
                    "--snapshot",
                    "outputs/neighbor-aware.snapshot.json",
                    "--format",
                    "html",
                    "--output",
                    "outputs/neighbor-aware.html",
                ],
                ["outputs/neighbor-aware.html"],
            ),
            SmokeCommand(
                "edit solved snapshot",
                [
                    "edit",
                    "--snapshot",
                    "outputs/neighbor-aware.snapshot.json",
                    "--operation",
                    "swap:STU001:STU002",
                    "--operation",
                    "lock-seat:R4C3",
                    "--output",
                    "outputs/neighbor-aware-edited.snapshot.json",
                ],
                ["outputs/neighbor-aware-edited.snapshot.json"],
            ),
            SmokeCommand(
                "export edited html",
                [
                    "export",
                    "--snapshot",
                    "outputs/neighbor-aware-edited.snapshot.json",
                    "--format",
                    "html",
                    "--output",
                    "outputs/neighbor-aware-edited.html",
                ],
                ["outputs/neighbor-aware-edited.html"],
            ),
            SmokeCommand(
                "solve candidate set",
                [
                    "solve",
                    "--students",
                    "examples/students.csv",
                    "--layout",
                    "examples/classroom.json",
                    "--rules",
                    "examples/rules_multi_candidate.json",
                    "--history-dir",
                    "examples/history",
                    "--backend",
                    "fallback",
                    "--time-limit",
                    str(time_limit),
                    "--candidates",
                    "3",
                    "--output",
                    "outputs/candidates.json",
                    "--report",
                    "outputs/plan-report.json",
                ],
                ["outputs/candidates.json", "outputs/plan-report.json"],
            ),
            SmokeCommand(
                "export print html with privacy options",
                [
                    "export",
                    "--snapshot",
                    "outputs/candidates.json",
                    "--candidate",
                    "recommended",
                    "--format",
                    "print-html",
                    "--template",
                    "teacher",
                    "--hide-notes",
                    "--hide-special-needs",
                    "--orientation",
                    "landscape",
                    "--page-scale",
                    "0.9",
                    "--locale",
                    "en",
                    "--output",
                    "outputs/recommended-print.html",
                ],
                ["outputs/recommended-print.html"],
            ),
            SmokeCommand(
                "export candidate comparison html",
                [
                    "export",
                    "--snapshot",
                    "outputs/candidates.json",
                    "--candidate-scope",
                    "all",
                    "--format",
                    "html",
                    "--output",
                    "outputs/candidate-comparison.html",
                ],
                ["outputs/candidate-comparison.html"],
            ),
        ]
    )
    commands.extend(_optional_commands(optional=optional, include_pdf=include_pdf))
    return commands


def _optional_commands(*, optional: str, include_pdf: bool) -> list[SmokeCommand]:
    commands: list[SmokeCommand] = []
    if _optional_enabled(optional, "openpyxl"):
        commands.extend(
            [
                SmokeCommand(
                    "solve Excel students",
                    [
                        "solve",
                        "--students",
                        "examples/students.xlsx",
                        "--layout",
                        "examples/classroom.json",
                        "--rules",
                        "examples/rules.json",
                        "--history-dir",
                        "examples/history",
                        "--backend",
                        "fallback",
                        "--output",
                        "outputs/excel.snapshot.json",
                    ],
                    ["outputs/excel.snapshot.json"],
                ),
                SmokeCommand(
                    "export Excel",
                    [
                        "export",
                        "--snapshot",
                        "outputs/excel.snapshot.json",
                        "--format",
                        "excel",
                        "--output",
                        "outputs/excel-export.xlsx",
                    ],
                    ["outputs/excel-export.xlsx"],
                ),
            ]
        )
    if _optional_enabled(optional, "PIL"):
        commands.append(
            SmokeCommand(
                "export PNG",
                [
                    "export",
                    "--snapshot",
                    "outputs/neighbor-aware.snapshot.json",
                    "--format",
                    "png",
                    "--output",
                    "outputs/neighbor-aware.png",
                ],
                ["outputs/neighbor-aware.png"],
            )
        )
    if _optional_enabled(optional, "docx"):
        commands.append(
            SmokeCommand(
                "export DOCX",
                [
                    "export",
                    "--snapshot",
                    "outputs/candidates.json",
                    "--candidate",
                    "recommended",
                    "--format",
                    "docx",
                    "--template",
                    "public",
                    "--output",
                    "outputs/recommended.docx",
                ],
                ["outputs/recommended.docx"],
            )
        )
    if include_pdf and _optional_enabled(optional, "weasyprint"):
        commands.append(
            SmokeCommand(
                "export PDF",
                [
                    "export",
                    "--snapshot",
                    "outputs/candidates.json",
                    "--candidate",
                    "recommended",
                    "--format",
                    "pdf",
                    "--template",
                    "public",
                    "--output",
                    "outputs/recommended.pdf",
                ],
                ["outputs/recommended.pdf"],
            )
        )
    return commands


def _optional_enabled(optional: str, module_name: str) -> bool:
    available = importlib.util.find_spec(module_name) is not None
    if optional == "yes" and not available:
        raise SystemExit(f"Optional dependency is required but missing: {module_name}")
    return available and optional != "no"


def _parse_backends(value: str) -> list[str]:
    backends = [item.strip() for item in value.split(",") if item.strip()]
    if not backends:
        raise SystemExit("At least one backend is required.")
    return backends


def _prepare_workdir(value: str | None) -> tuple[Path, bool]:
    if value:
        path = Path(value).resolve()
        path.mkdir(parents=True, exist_ok=True)
        return path, False
    return Path(tempfile.mkdtemp(prefix="seattrellis-cli-smoke-")), True


def _subprocess_env(repo_root: Path) -> dict[str, str]:
    env = os.environ.copy()
    src_path = str(repo_root / "src")
    existing = env.get("PYTHONPATH")
    env["PYTHONPATH"] = src_path if not existing else os.pathsep.join([src_path, existing])
    return env


def _check_outputs(workdir: Path, outputs: Iterable[str]) -> None:
    for output in outputs:
        path = workdir / output
        if not path.exists():
            raise SystemExit(f"Expected output was not created: {path}")
        if path.stat().st_size == 0:
            raise SystemExit(f"Expected output is empty: {path}")
        if path.suffix == ".json":
            json.loads(path.read_text(encoding="utf-8"))


def _format_failure(
    *,
    command: Sequence[str],
    cwd: Path,
    completed: subprocess.CompletedProcess[str],
) -> str:
    return "\n".join(
        [
            "CLI smoke command failed.",
            f"cwd: {cwd}",
            f"command: {shlex.join(command)}",
            f"exit code: {completed.returncode}",
            "--- stdout ---",
            completed.stdout.strip(),
            "--- stderr ---",
            completed.stderr.strip(),
        ]
    )


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run a real CLI workflow smoke test.")
    parser.add_argument("--workdir", default=None, help="Optional working directory.")
    parser.add_argument("--keep-workdir", action="store_true", help="Keep temporary workdir.")
    parser.add_argument(
        "--command",
        default=None,
        help="CLI command prefix. Defaults to the current Python module entrypoint.",
    )
    parser.add_argument(
        "--optional",
        choices=["auto", "yes", "no"],
        default="auto",
        help="Run optional Excel/PNG/DOCX checks when dependencies are available.",
    )
    parser.add_argument(
        "--backends",
        default="fallback",
        help="Comma-separated solver backends for the daily solve smoke.",
    )
    parser.add_argument("--time-limit", type=float, default=3.0, help="Solver time limit.")
    parser.add_argument(
        "--include-pdf",
        action="store_true",
        help="Also run PDF export when WeasyPrint is available.",
    )
    parser.add_argument("--json-report", default=None, help="Optional JSON report path.")
    args = parser.parse_args()
    if args.time_limit < 0.1:
        raise SystemExit("--time-limit must be at least 0.1 seconds.")
    return args


if __name__ == "__main__":
    main()
