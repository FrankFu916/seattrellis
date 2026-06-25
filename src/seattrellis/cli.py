from __future__ import annotations

import argparse
from pathlib import Path
from typing import Sequence

from seattrellis.candidates import generate_candidate_set
from seattrellis.demo import create_demo_files
from seattrellis.exporters import export_snapshot
from seattrellis.history import (
    build_fairness_report,
    build_pair_history,
    build_pair_history_report,
    build_seat_history,
    format_history_report,
    format_pair_history_report,
    load_history_snapshots,
)
from seattrellis.io.json_files import (
    InputFileError,
    load_layout,
    load_seating_artifact,
    write_json_model,
)
from seattrellis.io.project import (
    ProjectPaths,
    find_latest_project_artifact,
    load_project_paths,
    write_project,
)
from seattrellis.io.students import read_students
from seattrellis.io.validation import validate_files, validate_loaded_inputs
from seattrellis.models.candidate import CandidatePlan, CandidateSet, MultiSolveOptions
from seattrellis.models.project import SeatTrellisProject
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.presets import (
    export_preset,
    format_preset,
    format_preset_list,
    get_preset,
    load_rules_with_preset,
    preset_context_warnings,
)
from seattrellis.scoring import build_plan_comparison_report
from seattrellis.solver import SeatTrellisSolveError

try:
    import typer
except Exception:  # pragma: no cover - used only when Typer is not installed.
    typer = None  # type: ignore[assignment]


if typer is not None:
    app = typer.Typer(
        help="SeatTrellis classroom seating optimizer.",
        no_args_is_help=True,
    )
    presets_app = typer.Typer(
        help="List, inspect, and export built-in rules presets.",
        no_args_is_help=True,
    )
    app.add_typer(presets_app, name="presets")

    @presets_app.command("list", help="List built-in seating scenario presets.")
    def presets_list_command() -> None:
        typer.echo(format_preset_list())

    @presets_app.command("show", help="Show preset metadata and generated rules JSON.")
    def presets_show_command(
        preset: str = typer.Argument(..., help="Preset name."),
    ) -> None:
        _run_typer_action(lambda: typer.echo(format_preset(get_preset(preset))))

    @presets_app.command("export", help="Export a preset as a standard rules JSON file.")
    def presets_export_command(
        preset: str = typer.Argument(..., help="Preset name."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Rules JSON output path."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(f"Preset rules written to {export_preset(preset, output)}")
        )

    @app.command("init-demo", help="Create fictional demo input files under examples/.")
    def init_demo_command(
        output_dir: Path = typer.Option(Path("."), "--output-dir", "-o", help="Directory to create examples in."),
        force: bool = typer.Option(False, "--force", "--overwrite", help="Overwrite existing demo files."),
    ) -> None:
        _run_typer_action(lambda: _print_demo_result(init_demo(output_dir=output_dir, overwrite=force), force))

    @app.command("solve", help="Generate one snapshot or multiple scored candidate plans.")
    def solve_command(
        students: Path = typer.Option(..., "--students", help="CSV or Excel student file."),
        layout: Path = typer.Option(..., "--layout", help="Classroom layout JSON."),
        rules: Path | None = typer.Option(
            None,
            "--rules",
            help="Optional rules JSON. When combined with --preset, user fields override the preset.",
        ),
        preset: str | None = typer.Option(None, "--preset", help="Built-in rules preset name."),
        output: Path = typer.Option(
            Path("outputs/latest.snapshot.json"),
            "--output",
            "-o",
            help="Snapshot or candidate-set path.",
        ),
        history: list[Path] = typer.Option([], "--history", help="Historical snapshot JSON path. Can be repeated."),
        history_dir: Path | None = typer.Option(None, "--history-dir", help="Directory containing historical *.snapshot.json files."),
        time_limit_seconds: float = typer.Option(3.0, "--time-limit", help="Solver time limit in seconds."),
        candidates: int = typer.Option(1, "--candidates", help="Number of distinct candidate plans to generate (1-20)."),
        seed: int | None = typer.Option(None, "--seed", help="Override the rules-file seed."),
        report: Path | None = typer.Option(None, "--report", help="Optional plan comparison report JSON path."),
    ) -> None:
        _run_typer_action(
            lambda: _print_solve_result(
                solve_with_report(
                    students_path=students,
                    layout_path=layout,
                    rules_path=rules,
                    preset_name=preset,
                    output_path=output,
                    history_paths=history,
                    history_dir=history_dir,
                    time_limit_seconds=time_limit_seconds,
                    candidate_count=candidates,
                    seed=seed,
                    report_path=report,
                )
            )
        )

    @app.command("validate", help="Validate input files and hard-rule conflicts without solving.")
    def validate_command(
        students: Path = typer.Option(..., "--students", help="CSV or Excel student file."),
        layout: Path = typer.Option(..., "--layout", help="Classroom layout JSON."),
        rules: Path | None = typer.Option(
            None,
            "--rules",
            help="Optional rules JSON. When combined with --preset, user fields override the preset.",
        ),
        preset: str | None = typer.Option(None, "--preset", help="Built-in rules preset name."),
        history: list[Path] = typer.Option([], "--history", help="Historical snapshot JSON path. Can be repeated."),
        history_dir: Path | None = typer.Option(None, "--history-dir", help="Directory containing historical *.snapshot.json files."),
        strict: bool = typer.Option(False, "--strict", help="Treat warnings as validation failures."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                run_validate(
                    students_path=students,
                    layout_path=layout,
                    rules_path=rules,
                    preset_name=preset,
                    history_paths=history,
                    history_dir=history_dir,
                    strict=strict,
                )
            )
        )

    @app.command("export", help="Export a snapshot to Excel, PNG, or HTML.")
    def export_command(
        snapshot: Path = typer.Option(..., "--snapshot", help="Snapshot JSON path."),
        output_format: str = typer.Option(..., "--format", help="Export format: excel, png, html."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Output file path."),
        candidate: str | None = typer.Option(
            None,
            "--candidate",
            help="Candidate ID for a candidate set, or 'recommended'.",
        ),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                "Export written to "
                f"{export(snapshot_path=snapshot, output_format=output_format, output_path=output, candidate_id=candidate)}"
            )
        )

    @app.command("history-report", help="Summarize historical seating snapshots.")
    def history_report_command(
        students: Path = typer.Option(..., "--students", help="CSV or Excel student file."),
        layout: Path = typer.Option(..., "--layout", help="Classroom layout JSON."),
        history: list[Path] = typer.Option([], "--history", help="Historical snapshot JSON path. Can be repeated."),
        history_dir: Path | None = typer.Option(None, "--history-dir", help="Directory containing historical *.snapshot.json files."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Optional JSON report output path."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                run_history_report(
                    students_path=students,
                    layout_path=layout,
                    history_paths=history,
                    history_dir=history_dir,
                    output_path=output,
                )
            )
        )

    @app.command("pair-report", help="Summarize historical desk-mate and neighbor pairs.")
    def pair_report_command(
        students: Path = typer.Option(..., "--students", help="CSV or Excel student file."),
        layout: Path = typer.Option(..., "--layout", help="Classroom layout JSON."),
        history: list[Path] = typer.Option([], "--history", help="Historical snapshot JSON path. Can be repeated."),
        history_dir: Path | None = typer.Option(None, "--history-dir", help="Directory containing historical *.snapshot.json files."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Optional JSON report output path."),
        top: int = typer.Option(10, "--top", help="Number of high-frequency pairs to display."),
        within_distance: int = typer.Option(2, "--within-distance", help="Chebyshev distance threshold for within_distance."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                run_pair_report(
                    students_path=students,
                    layout_path=layout,
                    history_paths=history,
                    history_dir=history_dir,
                    output_path=output,
                    top=top,
                    within_distance=within_distance,
                )
            )
        )

    @app.command("project-init", help="Create a portable local project workspace file.")
    def project_init_command(
        project: Path = typer.Option(
            Path("seattrellis.project.json"),
            "--project",
            help="Project JSON path.",
        ),
        name: str = typer.Option("SeatTrellis Project", "--name", help="Project display name."),
        students: str = typer.Option("students.csv", "--students", help="Relative student file path."),
        layout: str = typer.Option("classroom.json", "--layout", help="Relative classroom layout path."),
        rules: str = typer.Option("rules.json", "--rules", help="Relative rules path."),
        history_dir: str | None = typer.Option(None, "--history-dir", help="Optional relative history directory."),
        outputs_dir: str = typer.Option("outputs", "--outputs-dir", help="Relative generated-output directory."),
        candidates: int = typer.Option(5, "--candidates", help="Default candidate count (1-20)."),
        force: bool = typer.Option(False, "--force", help="Overwrite an existing project file."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                "Project file written to "
                f"{project_init(project_path=project, name=name, students=students, layout=layout, rules=rules, history_dir=history_dir, outputs_dir=outputs_dir, candidates=candidates, force=force)}"
            )
        )

    @app.command("project-info", help="Show project settings and referenced-path status.")
    def project_info_command(
        project: Path = typer.Option(
            Path("seattrellis.project.json"),
            "--project",
            help="Project JSON path.",
        ),
    ) -> None:
        _run_typer_action(lambda: typer.echo(project_info(project_path=project)))

    @app.command("project-validate", help="Validate the inputs referenced by a project file.")
    def project_validate_command(
        project: Path = typer.Option(
            Path("seattrellis.project.json"),
            "--project",
            help="Project JSON path.",
        ),
        strict: bool = typer.Option(False, "--strict", help="Treat warnings as validation failures."),
    ) -> None:
        _run_typer_action(lambda: typer.echo(project_validate(project_path=project, strict=strict)))

    @app.command("project-solve", help="Solve using inputs and defaults from a project file.")
    def project_solve_command(
        project: Path = typer.Option(
            Path("seattrellis.project.json"),
            "--project",
            help="Project JSON path.",
        ),
        candidates: int | None = typer.Option(None, "--candidates", help="Override the default candidate count."),
        seed: int | None = typer.Option(None, "--seed", help="Override the rules-file seed."),
        time_limit_seconds: float = typer.Option(3.0, "--time-limit", help="Solver time limit in seconds."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Override the output JSON path."),
        report: Path | None = typer.Option(None, "--report", help="Optional plan comparison report JSON path."),
    ) -> None:
        _run_typer_action(
            lambda: _print_solve_result(
                project_solve(
                    project_path=project,
                    candidate_count=candidates,
                    seed=seed,
                    time_limit_seconds=time_limit_seconds,
                    output_path=output,
                    report_path=report,
                )
            )
        )

    @app.command("project-export", help="Export the latest or selected project seating artifact.")
    def project_export_command(
        project: Path = typer.Option(
            Path("seattrellis.project.json"),
            "--project",
            help="Project JSON path.",
        ),
        snapshot: Path | None = typer.Option(None, "--snapshot", help="Snapshot or candidate-set JSON path."),
        output_format: str | None = typer.Option(None, "--format", help="Export format: excel, png, html."),
        candidate: str | None = typer.Option(
            None,
            "--candidate",
            help="Candidate ID, or 'recommended'.",
        ),
        output: Path | None = typer.Option(None, "--output", "-o", help="Override the exported file path."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                "Export written to "
                f"{project_export(project_path=project, snapshot_path=snapshot, output_format=output_format, candidate_id=candidate, output_path=output)}"
            )
        )

else:
    app = None


def init_demo(output_dir: str | Path = ".", *, overwrite: bool = False) -> dict[str, Path]:
    return create_demo_files(output_dir, overwrite=overwrite)


def solve(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    rules_path: str | Path | None = None,
    preset_name: str | None = None,
    output_path: str | Path = "outputs/latest.snapshot.json",
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    time_limit_seconds: float = 3.0,
    candidate_count: int = 1,
    seed: int | None = None,
    report_path: str | Path | None = None,
) -> Path:
    path, _summary = solve_with_report(
        students_path=students_path,
        layout_path=layout_path,
        rules_path=rules_path,
        preset_name=preset_name,
        output_path=output_path,
        history_paths=history_paths,
        history_dir=history_dir,
        time_limit_seconds=time_limit_seconds,
        candidate_count=candidate_count,
        seed=seed,
        report_path=report_path,
    )
    return path


def solve_with_report(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    rules_path: str | Path | None = None,
    preset_name: str | None = None,
    output_path: str | Path = "outputs/latest.snapshot.json",
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    time_limit_seconds: float = 3.0,
    candidate_count: int = 1,
    seed: int | None = None,
    report_path: str | Path | None = None,
) -> tuple[Path, str | None]:
    students = read_students(students_path)
    layout = load_layout(layout_path)
    rules, preset = load_rules_with_preset(
        rules_path=rules_path,
        preset_name=preset_name,
    )
    history_snapshots = load_history_snapshots(history_paths=history_paths, history_dir=history_dir)
    validation = validate_loaded_inputs(students, layout, rules)
    validation.raise_for_errors(title="Input validation failed.")
    preset_warnings = preset_context_warnings(
        preset,
        students,
        history_count=len(history_snapshots),
        rules=rules,
    )
    seat_history = build_seat_history(students, layout, history_snapshots)
    pair_rule = rules.soft.avoid_recent_neighbors
    pair_history = build_pair_history(
        students,
        layout,
        history_snapshots,
        lookback=pair_rule.lookback,
        within_distance=pair_rule.within_distance,
    )
    options = MultiSolveOptions(
        candidate_count=candidate_count,
        seed=rules.seed if seed is None else seed,
    )
    candidate_set = generate_candidate_set(
        students,
        layout,
        rules,
        history=seat_history,
        pair_history=pair_history,
        history_snapshots=history_snapshots,
        options=options,
        time_limit_seconds=time_limit_seconds,
    )
    _apply_preset_metadata(
        candidate_set,
        preset_name=preset.name if preset is not None else None,
        rules_overlay=rules_path is not None and preset is not None,
        warnings=preset_warnings,
    )
    if candidate_count == 1:
        snapshot = candidate_set.candidates[0].snapshot
        path = write_json_model(snapshot, output_path)
        fairness = snapshot.metrics.get("fairness", {})
        summary = _format_solve_fairness_summary(fairness) if fairness else None
        summary = _append_warnings(summary, preset_warnings)
    else:
        path = write_json_model(candidate_set, output_path)
        summary = _format_candidate_set_summary(candidate_set)
    if report_path is not None:
        report = build_plan_comparison_report(candidate_set, history_snapshots=history_snapshots)
        report_output = write_json_model(report, report_path)
        report_line = f"Full report written to {report_output}"
        summary = f"{summary}\n\n{report_line}" if summary else report_line
    return path, summary


def export(
    *,
    snapshot_path: str | Path,
    output_format: str,
    output_path: str | Path | None = None,
    candidate_id: str | None = None,
    default_candidate_id: str = "recommended",
) -> Path:
    artifact = load_seating_artifact(snapshot_path)
    if isinstance(artifact, CandidateSet):
        candidate = artifact.get_candidate(candidate_id or default_candidate_id)
        snapshot = _snapshot_with_candidate_metadata(candidate)
    else:
        if candidate_id is not None:
            raise ValueError("--candidate can only be used when --snapshot is a candidate set.")
        snapshot = artifact
    return export_snapshot(snapshot, output_format, output_path)


def run_validate(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    rules_path: str | Path | None = None,
    preset_name: str | None = None,
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    strict: bool = False,
) -> str:
    history_snapshots = load_history_snapshots(
        history_paths=history_paths,
        history_dir=history_dir,
    )
    report = validate_files(
        students_path=students_path,
        layout_path=layout_path,
        rules_path=rules_path,
        preset_name=preset_name,
        history_count=len(history_snapshots),
    )
    report.raise_for_errors(strict=strict)
    return report.format_success()


def run_history_report(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    output_path: str | Path | None = None,
) -> str:
    students = read_students(students_path)
    layout = load_layout(layout_path)
    snapshots = load_history_snapshots(history_paths=history_paths, history_dir=history_dir)
    history = build_seat_history(students, layout, snapshots)
    report = build_fairness_report(history)
    if output_path is not None:
        write_json_model(report, output_path)
    return format_history_report(report)


def run_pair_report(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    history_paths: Sequence[str | Path] | None = None,
    history_dir: str | Path | None = None,
    output_path: str | Path | None = None,
    top: int = 10,
    within_distance: int = 2,
) -> str:
    if top <= 0:
        raise ValueError("top must be positive.")
    if within_distance <= 0:
        raise ValueError("within_distance must be positive.")
    students = read_students(students_path)
    layout = load_layout(layout_path)
    snapshots = load_history_snapshots(history_paths=history_paths, history_dir=history_dir)
    pair_history = build_pair_history(students, layout, snapshots, within_distance=within_distance)
    report = build_pair_history_report(pair_history, top=top)
    if output_path is not None:
        write_json_model(report, output_path)
    return format_pair_history_report(report, top=top)


def project_init(
    *,
    project_path: str | Path = "seattrellis.project.json",
    name: str = "SeatTrellis Project",
    students: str = "students.csv",
    layout: str = "classroom.json",
    rules: str = "rules.json",
    history_dir: str | None = None,
    outputs_dir: str = "outputs",
    candidates: int = 5,
    force: bool = False,
) -> Path:
    project = SeatTrellisProject(
        name=name,
        students=students,
        layout=layout,
        rules=rules,
        history_dir=history_dir,
        outputs_dir=outputs_dir,
        default_candidates=candidates,
    )
    return write_project(project, project_path, overwrite=force)


def project_info(*, project_path: str | Path = "seattrellis.project.json") -> str:
    project, paths = load_project_paths(project_path)
    lines = [
        f"Project: {project.name}",
        f"Project file: {paths.project_file}",
        f"Schema version: {project.schema_version}",
        "",
        "Paths:",
        _format_project_path("students", project.students, paths.students),
        _format_project_path("layout", project.layout, paths.layout),
        _format_project_path("rules", project.rules, paths.rules),
    ]
    if project.history_dir is None:
        lines.append("- history_dir: not configured")
    else:
        lines.append(_format_project_path("history_dir", project.history_dir, paths.history_dir))
    lines.extend(
        [
            _format_project_path("outputs_dir", project.outputs_dir, paths.outputs_dir),
            "",
            "Defaults:",
            f"- candidates: {project.default_candidates}",
            f"- candidate: {project.default_candidate}",
            f"- export format: {project.default_export_format}",
        ]
    )
    return "\n".join(lines)


def project_validate(
    *,
    project_path: str | Path = "seattrellis.project.json",
    strict: bool = False,
) -> str:
    _project, paths = load_project_paths(
        project_path,
        require_inputs=True,
        require_history=True,
    )
    return run_validate(
        students_path=paths.students,
        layout_path=paths.layout,
        rules_path=paths.rules,
        strict=strict,
    )


def project_solve(
    *,
    project_path: str | Path = "seattrellis.project.json",
    candidate_count: int | None = None,
    seed: int | None = None,
    time_limit_seconds: float = 3.0,
    output_path: str | Path | None = None,
    report_path: str | Path | None = None,
) -> tuple[Path, str | None]:
    project, paths = load_project_paths(
        project_path,
        require_inputs=True,
        require_history=True,
        create_outputs=True,
    )
    count = project.default_candidates if candidate_count is None else candidate_count
    if not 1 <= count <= 20:
        raise ValueError("candidates must be between 1 and 20.")
    if output_path is None:
        filename = "latest.snapshot.json" if count == 1 else "latest.candidates.json"
        output_path = paths.outputs_dir / filename
    return solve_with_report(
        students_path=paths.students,
        layout_path=paths.layout,
        rules_path=paths.rules,
        output_path=output_path,
        history_dir=paths.history_dir,
        time_limit_seconds=time_limit_seconds,
        candidate_count=count,
        seed=seed,
        report_path=report_path,
    )


def project_export(
    *,
    project_path: str | Path = "seattrellis.project.json",
    snapshot_path: str | Path | None = None,
    output_format: str | None = None,
    candidate_id: str | None = None,
    output_path: str | Path | None = None,
) -> Path:
    project, paths = load_project_paths(project_path, create_outputs=True)
    selected_snapshot = (
        Path(snapshot_path)
        if snapshot_path is not None
        else find_latest_project_artifact(paths.outputs_dir)
    )
    selected_format = output_format or project.default_export_format
    if output_path is None:
        output_path = paths.outputs_dir / f"seating.{_export_extension(selected_format)}"
    return export(
        snapshot_path=selected_snapshot,
        output_format=selected_format,
        output_path=output_path,
        candidate_id=candidate_id,
        default_candidate_id=project.default_candidate,
    )


def main() -> None:
    if typer is not None:
        app()
        return
    try:
        _run_argparse()
    except (InputFileError, MissingOptionalDependencyError, SeatTrellisSolveError, ValueError, OSError) as exc:
        print(f"Error: {_friendly_error(exc)}")
        raise SystemExit(1) from exc


def _run_argparse() -> None:
    parser = argparse.ArgumentParser(prog="seattrellis", description="SeatTrellis classroom seating optimizer.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    init_parser = subparsers.add_parser("init-demo", help="Create fictional demo input files.")
    init_parser.add_argument("--output-dir", "-o", default=".")
    init_parser.add_argument("--force", "--overwrite", dest="overwrite", action="store_true")

    presets_parser = subparsers.add_parser("presets", help="Manage built-in rules presets.")
    preset_subparsers = presets_parser.add_subparsers(dest="preset_command", required=True)
    preset_subparsers.add_parser("list", help="List built-in presets.")
    preset_show_parser = preset_subparsers.add_parser("show", help="Show one preset.")
    preset_show_parser.add_argument("preset")
    preset_export_parser = preset_subparsers.add_parser("export", help="Export one preset.")
    preset_export_parser.add_argument("preset")
    preset_export_parser.add_argument("--output", "-o", default=None)

    solve_parser = subparsers.add_parser("solve", help="Generate a seating snapshot.")
    solve_parser.add_argument("--students", required=True)
    solve_parser.add_argument("--layout", required=True)
    solve_parser.add_argument("--rules", default=None)
    solve_parser.add_argument("--preset", default=None)
    solve_parser.add_argument("--output", "-o", default="outputs/latest.snapshot.json")
    solve_parser.add_argument("--history", action="append", default=[])
    solve_parser.add_argument("--history-dir", default=None)
    solve_parser.add_argument("--time-limit", type=float, default=3.0)
    solve_parser.add_argument("--candidates", type=int, default=1)
    solve_parser.add_argument("--seed", type=int, default=None)
    solve_parser.add_argument("--report", default=None)

    validate_parser = subparsers.add_parser("validate", help="Validate input files without solving.")
    validate_parser.add_argument("--students", required=True)
    validate_parser.add_argument("--layout", required=True)
    validate_parser.add_argument("--rules", default=None)
    validate_parser.add_argument("--preset", default=None)
    validate_parser.add_argument("--history", action="append", default=[])
    validate_parser.add_argument("--history-dir", default=None)
    validate_parser.add_argument("--strict", action="store_true")

    export_parser = subparsers.add_parser("export", help="Export a snapshot.")
    export_parser.add_argument("--snapshot", required=True)
    export_parser.add_argument("--format", required=True)
    export_parser.add_argument("--output", "-o", default=None)
    export_parser.add_argument("--candidate", default=None)

    history_parser = subparsers.add_parser("history-report", help="Summarize historical seating snapshots.")
    history_parser.add_argument("--students", required=True)
    history_parser.add_argument("--layout", required=True)
    history_parser.add_argument("--history", action="append", default=[])
    history_parser.add_argument("--history-dir", default=None)
    history_parser.add_argument("--output", "-o", default=None)

    pair_parser = subparsers.add_parser("pair-report", help="Summarize historical desk-mate and neighbor pairs.")
    pair_parser.add_argument("--students", required=True)
    pair_parser.add_argument("--layout", required=True)
    pair_parser.add_argument("--history", action="append", default=[])
    pair_parser.add_argument("--history-dir", default=None)
    pair_parser.add_argument("--output", "-o", default=None)
    pair_parser.add_argument("--top", type=int, default=10)
    pair_parser.add_argument("--within-distance", type=int, default=2)

    project_init_parser = subparsers.add_parser("project-init", help="Create a project workspace file.")
    project_init_parser.add_argument("--project", default="seattrellis.project.json")
    project_init_parser.add_argument("--name", default="SeatTrellis Project")
    project_init_parser.add_argument("--students", default="students.csv")
    project_init_parser.add_argument("--layout", default="classroom.json")
    project_init_parser.add_argument("--rules", default="rules.json")
    project_init_parser.add_argument("--history-dir", default=None)
    project_init_parser.add_argument("--outputs-dir", default="outputs")
    project_init_parser.add_argument("--candidates", type=int, default=5)
    project_init_parser.add_argument("--force", action="store_true")

    project_info_parser = subparsers.add_parser("project-info", help="Show project settings.")
    project_info_parser.add_argument("--project", default="seattrellis.project.json")

    project_validate_parser = subparsers.add_parser("project-validate", help="Validate project inputs.")
    project_validate_parser.add_argument("--project", default="seattrellis.project.json")
    project_validate_parser.add_argument("--strict", action="store_true")

    project_solve_parser = subparsers.add_parser("project-solve", help="Solve a project.")
    project_solve_parser.add_argument("--project", default="seattrellis.project.json")
    project_solve_parser.add_argument("--candidates", type=int, default=None)
    project_solve_parser.add_argument("--seed", type=int, default=None)
    project_solve_parser.add_argument("--time-limit", type=float, default=3.0)
    project_solve_parser.add_argument("--output", "-o", default=None)
    project_solve_parser.add_argument("--report", default=None)

    project_export_parser = subparsers.add_parser("project-export", help="Export a project artifact.")
    project_export_parser.add_argument("--project", default="seattrellis.project.json")
    project_export_parser.add_argument("--snapshot", default=None)
    project_export_parser.add_argument("--format", default=None)
    project_export_parser.add_argument("--candidate", default=None)
    project_export_parser.add_argument("--output", "-o", default=None)

    args = parser.parse_args()
    if args.command == "init-demo":
        paths = init_demo(output_dir=args.output_dir, overwrite=args.overwrite)
        print(f"Demo files ready in {paths['students_csv'].parent}")
        if not args.overwrite:
            print("Existing files were kept. Use --force to overwrite demo files.")
    elif args.command == "presets":
        if args.preset_command == "list":
            print(format_preset_list())
        elif args.preset_command == "show":
            print(format_preset(get_preset(args.preset)))
        elif args.preset_command == "export":
            print(f"Preset rules written to {export_preset(args.preset, args.output)}")
    elif args.command == "solve":
        path, summary = solve_with_report(
            students_path=args.students,
            layout_path=args.layout,
            rules_path=args.rules,
            preset_name=args.preset,
            output_path=args.output,
            history_paths=args.history,
            history_dir=args.history_dir,
            time_limit_seconds=args.time_limit,
            candidate_count=args.candidates,
            seed=args.seed,
            report_path=args.report,
        )
        print(f"{_solve_output_label(summary)} written to {path}")
        if summary:
            print(summary)
    elif args.command == "validate":
        print(
            run_validate(
                students_path=args.students,
                layout_path=args.layout,
                rules_path=args.rules,
                preset_name=args.preset,
                history_paths=args.history,
                history_dir=args.history_dir,
                strict=args.strict,
            )
        )
    elif args.command == "export":
        path = export(
            snapshot_path=args.snapshot,
            output_format=args.format,
            output_path=args.output,
            candidate_id=args.candidate,
        )
        print(f"Export written to {path}")
    elif args.command == "history-report":
        print(
            run_history_report(
                students_path=args.students,
                layout_path=args.layout,
                history_paths=args.history,
                history_dir=args.history_dir,
                output_path=args.output,
            )
        )
    elif args.command == "pair-report":
        print(
            run_pair_report(
                students_path=args.students,
                layout_path=args.layout,
                history_paths=args.history,
                history_dir=args.history_dir,
                output_path=args.output,
                top=args.top,
                within_distance=args.within_distance,
            )
        )
    elif args.command == "project-init":
        path = project_init(
            project_path=args.project,
            name=args.name,
            students=args.students,
            layout=args.layout,
            rules=args.rules,
            history_dir=args.history_dir,
            outputs_dir=args.outputs_dir,
            candidates=args.candidates,
            force=args.force,
        )
        print(f"Project file written to {path}")
    elif args.command == "project-info":
        print(project_info(project_path=args.project))
    elif args.command == "project-validate":
        print(project_validate(project_path=args.project, strict=args.strict))
    elif args.command == "project-solve":
        path, summary = project_solve(
            project_path=args.project,
            candidate_count=args.candidates,
            seed=args.seed,
            time_limit_seconds=args.time_limit,
            output_path=args.output,
            report_path=args.report,
        )
        print(f"{_solve_output_label(summary)} written to {path}")
        if summary:
            print(summary)
    elif args.command == "project-export":
        path = project_export(
            project_path=args.project,
            snapshot_path=args.snapshot,
            output_format=args.format,
            candidate_id=args.candidate,
            output_path=args.output,
        )
        print(f"Export written to {path}")


def _run_typer_action(action) -> None:
    try:
        action()
    except (InputFileError, MissingOptionalDependencyError, SeatTrellisSolveError, ValueError, OSError) as exc:
        typer.echo(f"Error: {_friendly_error(exc)}", err=True)
        raise typer.Exit(1) from exc


def _print_demo_result(paths: dict[str, Path], overwrite: bool) -> None:
    typer.echo(f"Demo files ready in {paths['students_csv'].parent}")
    if not overwrite:
        typer.echo("Existing files were kept. Use --force to overwrite demo files.")


def _print_solve_result(result: tuple[Path, str | None]) -> None:
    path, summary = result
    typer.echo(f"{_solve_output_label(summary)} written to {path}")
    if summary:
        typer.echo(summary)


def _solve_output_label(summary: str | None) -> str:
    return "Candidate set" if summary and summary.startswith("Generated ") else "Snapshot"


def _format_candidate_set_summary(candidate_set: CandidateSet) -> str:
    lines = [
        f"Generated {len(candidate_set.candidates)} candidate seating plans.",
        "",
        f"Recommended: {candidate_set.recommended_candidate_id}",
        "",
        "Candidate summary:",
    ]
    ranked = sorted(
        candidate_set.candidates,
        key=lambda candidate: (-candidate.total_score, candidate.candidate_id),
    )
    for candidate in ranked:
        breakdown = candidate.score.breakdown
        lines.append(
            f"- {candidate.candidate_id}: total {candidate.total_score:.1f} | "
            f"fair rotation {_dimension_rating(breakdown.fair_rotation_score.rating)} | "
            "neighbor repetition "
            f"{_neighbor_rating(breakdown.avoid_recent_neighbors_score.rating)} | "
            f"score balance {_dimension_rating(breakdown.score_balance_score.rating)}"
        )
    if candidate_set.warnings:
        lines.append("")
        lines.append("Warnings:")
        lines.extend(f"- {warning}" for warning in candidate_set.warnings)
    return "\n".join(lines)


def _apply_preset_metadata(
    candidate_set: CandidateSet,
    *,
    preset_name: str | None,
    rules_overlay: bool,
    warnings: Sequence[str],
) -> None:
    if preset_name is None:
        return
    metadata = {
        "name": preset_name,
        "user_rules_overlay": rules_overlay,
    }
    candidate_set.metadata["preset"] = metadata
    for warning in warnings:
        if warning not in candidate_set.warnings:
            candidate_set.warnings.append(warning)
    for candidate in candidate_set.candidates:
        candidate.snapshot.metadata["preset"] = metadata
        if warnings:
            candidate.snapshot.metadata["warnings"] = list(warnings)


def _append_warnings(summary: str | None, warnings: Sequence[str]) -> str | None:
    if not warnings:
        return summary
    warning_text = "\n".join(["Warnings:", *(f"- {warning}" for warning in warnings)])
    return f"{summary}\n\n{warning_text}" if summary else warning_text


def _dimension_rating(rating: str) -> str:
    return rating.replace("_", " ")


def _neighbor_rating(rating: str) -> str:
    if rating == "high":
        return "low"
    if rating == "low":
        return "high"
    return _dimension_rating(rating)


def _snapshot_with_candidate_metadata(candidate: CandidatePlan) -> SeatingSnapshot:
    metadata = dict(candidate.snapshot.metadata)
    metadata["candidate"] = {
        "candidate_id": candidate.candidate_id,
        "total_score": candidate.total_score,
        "hard_constraints_satisfied": candidate.hard_constraints_satisfied,
        "score_breakdown": {
            "fair_rotation_score": candidate.score.breakdown.fair_rotation_score.score,
            "avoid_recent_neighbors_score": candidate.score.breakdown.avoid_recent_neighbors_score.score,
            "score_balance_score": candidate.score.breakdown.score_balance_score.score,
            "height_preference_score": candidate.score.breakdown.height_preference_score.score,
            "vision_preference_score": candidate.score.breakdown.vision_preference_score.score,
            "diversity_score": candidate.score.breakdown.diversity_score.score,
            "stability_score": candidate.score.breakdown.stability_score.score,
        },
    }
    if hasattr(candidate.snapshot, "model_copy"):
        return candidate.snapshot.model_copy(update={"metadata": metadata})  # type: ignore[attr-defined,return-value]
    return candidate.snapshot.copy(update={"metadata": metadata})


def _format_solve_fairness_summary(fairness: object) -> str | None:
    if not isinstance(fairness, dict):
        return None
    history_count = fairness.get("history_count", 0)
    enabled_rules = fairness.get("enabled_rules", [])
    if not enabled_rules:
        message = fairness.get("message")
        if message:
            return f"Fairness: {message}"
        return f"Fairness: history snapshots={history_count}, no active fairness rules."
    fair_cost = fairness.get("fair_rotation_cost")
    neighbor_cost = fairness.get("avoid_recent_neighbors_cost")
    cost_parts = []
    if fair_cost is not None:
        cost_parts.append(f"fair_rotation_cost={fair_cost}")
    if neighbor_cost is not None:
        cost_parts.append(f"avoid_recent_neighbors_cost={neighbor_cost}")
    suffix = ", ".join(cost_parts) if cost_parts else f"enabled_rules={enabled_rules}"
    return f"Fairness: history snapshots={history_count}, {suffix}."


def _friendly_error(exc: Exception) -> str:
    return str(exc) or exc.__class__.__name__


def _format_project_path(label: str, configured: str, resolved: Path | None) -> str:
    if resolved is None:
        return f"- {label}: {configured} [not configured]"
    status = "exists" if resolved.exists() else "missing"
    return f"- {label}: {configured} -> {resolved} [{status}]"


def _export_extension(output_format: str) -> str:
    normalized = output_format.lower()
    if normalized in {"excel", "xlsx"}:
        return "xlsx"
    if normalized in {"html", "png"}:
        return normalized
    raise ValueError(f"Unsupported export format: {output_format}")


if __name__ == "__main__":
    main()
