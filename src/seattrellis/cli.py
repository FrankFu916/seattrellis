from __future__ import annotations

import argparse
from pathlib import Path

from seattrellis.demo import create_demo_files
from seattrellis.exporters import export_snapshot
from seattrellis.io.json_files import load_layout, load_rules, load_snapshot, write_json_model
from seattrellis.io.students import read_students
from seattrellis.solver import solve_seating

try:
    import typer
except Exception:  # pragma: no cover - used only when Typer is not installed.
    typer = None  # type: ignore[assignment]


if typer is not None:
    app = typer.Typer(help="SeatTrellis classroom seating optimizer.")

    @app.command("init-demo")
    def init_demo_command(
        output_dir: Path = typer.Option(Path("."), "--output-dir", "-o", help="Directory to create examples in."),
        overwrite: bool = typer.Option(False, "--overwrite", help="Overwrite existing demo files."),
    ) -> None:
        paths = init_demo(output_dir=output_dir, overwrite=overwrite)
        typer.echo(f"Demo files ready in {paths['students_csv'].parent}")

    @app.command("solve")
    def solve_command(
        students: Path = typer.Option(..., "--students", help="CSV or Excel student file."),
        layout: Path = typer.Option(..., "--layout", help="Classroom layout JSON."),
        rules: Path = typer.Option(..., "--rules", help="Rules JSON."),
        output: Path = typer.Option(Path("outputs/latest.snapshot.json"), "--output", "-o", help="Snapshot path."),
        time_limit_seconds: float = typer.Option(10.0, "--time-limit", help="Solver time limit in seconds."),
    ) -> None:
        snapshot_path = solve(
            students_path=students,
            layout_path=layout,
            rules_path=rules,
            output_path=output,
            time_limit_seconds=time_limit_seconds,
        )
        typer.echo(f"Snapshot written to {snapshot_path}")

    @app.command("export")
    def export_command(
        snapshot: Path = typer.Option(..., "--snapshot", help="Snapshot JSON path."),
        output_format: str = typer.Option(..., "--format", help="Export format: excel, png, html."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Output file path."),
    ) -> None:
        output_path = export(snapshot_path=snapshot, output_format=output_format, output_path=output)
        typer.echo(f"Export written to {output_path}")

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
    time_limit_seconds: float = 10.0,
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
    _run_argparse()


def _run_argparse() -> None:
    parser = argparse.ArgumentParser(prog="seattrellis")
    subparsers = parser.add_subparsers(dest="command", required=True)

    init_parser = subparsers.add_parser("init-demo")
    init_parser.add_argument("--output-dir", "-o", default=".")
    init_parser.add_argument("--overwrite", action="store_true")

    solve_parser = subparsers.add_parser("solve")
    solve_parser.add_argument("--students", required=True)
    solve_parser.add_argument("--layout", required=True)
    solve_parser.add_argument("--rules", required=True)
    solve_parser.add_argument("--output", "-o", default="outputs/latest.snapshot.json")
    solve_parser.add_argument("--time-limit", type=float, default=10.0)

    export_parser = subparsers.add_parser("export")
    export_parser.add_argument("--snapshot", required=True)
    export_parser.add_argument("--format", required=True)
    export_parser.add_argument("--output", "-o", default=None)

    args = parser.parse_args()
    if args.command == "init-demo":
        paths = init_demo(output_dir=args.output_dir, overwrite=args.overwrite)
        print(f"Demo files ready in {paths['students_csv'].parent}")
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


if __name__ == "__main__":
    main()
