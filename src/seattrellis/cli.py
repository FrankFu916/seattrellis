from __future__ import annotations

import argparse
from pathlib import Path

from seattrellis.demo import create_demo_files
from seattrellis.exporters import export_snapshot
from seattrellis.io.json_files import InputFileError, load_layout, load_rules, load_snapshot, write_json_model
from seattrellis.io.students import read_students
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.solver import SeatTrellisSolveError, solve_seating

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

    @app.command("solve", help="Generate a seating snapshot from students, layout, and rules.")
    def solve_command(
        students: Path = typer.Option(..., "--students", help="CSV or Excel student file."),
        layout: Path = typer.Option(..., "--layout", help="Classroom layout JSON."),
        rules: Path = typer.Option(..., "--rules", help="Rules JSON."),
        output: Path = typer.Option(Path("outputs/latest.snapshot.json"), "--output", "-o", help="Snapshot path."),
        time_limit_seconds: float = typer.Option(3.0, "--time-limit", help="Solver time limit in seconds."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                f"Snapshot written to {solve(students_path=students, layout_path=layout, rules_path=rules, output_path=output, time_limit_seconds=time_limit_seconds)}"
            )
        )

    @app.command("export", help="Export a snapshot to Excel, PNG, or HTML.")
    def export_command(
        snapshot: Path = typer.Option(..., "--snapshot", help="Snapshot JSON path."),
        output_format: str = typer.Option(..., "--format", help="Export format: excel, png, html."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Output file path."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                f"Export written to {export(snapshot_path=snapshot, output_format=output_format, output_path=output)}"
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
    time_limit_seconds: float = 3.0,
) -> Path:
    students = read_students(students_path)
    layout = load_layout(layout_path)
    rules = load_rules(rules_path)
    solution = solve_seating(students, layout, rules, seed=rules.seed, time_limit_seconds=time_limit_seconds)
    snapshot = solution.to_snapshot(students=students, layout=layout, rules=rules, seed=rules.seed)
    return write_json_model(snapshot, output_path)


def export(
    *,
    snapshot_path: str | Path,
    output_format: str,
    output_path: str | Path | None = None,
) -> Path:
    snapshot = load_snapshot(snapshot_path)
    return export_snapshot(snapshot, output_format, output_path)


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
    solve_parser.add_argument("--time-limit", type=float, default=3.0)

    export_parser = subparsers.add_parser("export", help="Export a snapshot.")
    export_parser.add_argument("--snapshot", required=True)
    export_parser.add_argument("--format", required=True)
    export_parser.add_argument("--output", "-o", default=None)

    args = parser.parse_args()
    if args.command == "init-demo":
        paths = init_demo(output_dir=args.output_dir, overwrite=args.overwrite)
        print(f"Demo files ready in {paths['students_csv'].parent}")
        if not args.overwrite:
            print("Existing files were kept. Use --force to overwrite demo files.")
    elif args.command == "solve":
        path = solve(
            students_path=args.students,
            layout_path=args.layout,
            rules_path=args.rules,
            output_path=args.output,
            time_limit_seconds=args.time_limit,
        )
        print(f"Snapshot written to {path}")
    elif args.command == "export":
        path = export(snapshot_path=args.snapshot, output_format=args.format, output_path=args.output)
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


def _friendly_error(exc: Exception) -> str:
    return str(exc) or exc.__class__.__name__


if __name__ == "__main__":
    main()
