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
    load_rules,
    load_seating_artifact,
    write_json_model,
)
from seattrellis.io.students import read_students
from seattrellis.io.validation import validate_files, validate_loaded_inputs
from seattrellis.models.candidate import CandidatePlan, CandidateSet, MultiSolveOptions
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.optional import MissingOptionalDependencyError
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
        rules: Path = typer.Option(..., "--rules", help="Rules JSON."),
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
        rules: Path = typer.Option(..., "--rules", help="Rules JSON."),
        strict: bool = typer.Option(False, "--strict", help="Treat warnings as validation failures."),
    ) -> None:
        _run_typer_action(lambda: typer.echo(run_validate(students_path=students, layout_path=layout, rules_path=rules, strict=strict)))

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

else:
    app = None


def init_demo(output_dir: str | Path = ".", *, overwrite: bool = False) -> dict[str, Path]:
    return create_demo_files(output_dir, overwrite=overwrite)


def solve(
    *,
    students_path: str | Path,
    layout_path: str | Path,
    rules_path: str | Path,
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
    rules_path: str | Path,
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
    rules = load_rules(rules_path)
    validate_loaded_inputs(students, layout, rules).raise_for_errors(title="Input validation failed.")
    history_snapshots = load_history_snapshots(history_paths=history_paths, history_dir=history_dir)
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
    if candidate_count == 1:
        snapshot = candidate_set.candidates[0].snapshot
        path = write_json_model(snapshot, output_path)
        fairness = snapshot.metrics.get("fairness", {})
        summary = _format_solve_fairness_summary(fairness) if fairness else None
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
) -> Path:
    artifact = load_seating_artifact(snapshot_path)
    if isinstance(artifact, CandidateSet):
        candidate = artifact.get_candidate(candidate_id or "recommended")
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
    rules_path: str | Path,
    strict: bool = False,
) -> str:
    report = validate_files(students_path=students_path, layout_path=layout_path, rules_path=rules_path)
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

    solve_parser = subparsers.add_parser("solve", help="Generate a seating snapshot.")
    solve_parser.add_argument("--students", required=True)
    solve_parser.add_argument("--layout", required=True)
    solve_parser.add_argument("--rules", required=True)
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
    validate_parser.add_argument("--rules", required=True)
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

    args = parser.parse_args()
    if args.command == "init-demo":
        paths = init_demo(output_dir=args.output_dir, overwrite=args.overwrite)
        print(f"Demo files ready in {paths['students_csv'].parent}")
        if not args.overwrite:
            print("Existing files were kept. Use --force to overwrite demo files.")
    elif args.command == "solve":
        path, summary = solve_with_report(
            students_path=args.students,
            layout_path=args.layout,
            rules_path=args.rules,
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


if __name__ == "__main__":
    main()
