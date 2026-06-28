from __future__ import annotations

import argparse
from pathlib import Path

from seattrellis.io.json_files import InputFileError
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.presets import (
    export_preset,
    format_preset,
    format_preset_list,
    get_preset,
)
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

    # --version callback
    def _version_callback(value: bool) -> None:
        if value:
            from seattrellis import __version__
            typer.echo(f"seattrellis {__version__}")
            raise typer.Exit()

    @app.callback()
    def _main_callback(
        version: bool = typer.Option(
            False,
            "--version",
            "-V",
            help="Show version and exit.",
            callback=_version_callback,
            is_eager=True,
        ),
    ) -> None:
        pass

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

    @app.command("doctor", help="Check environment: Python, optional deps, examples, outputs.")
    def doctor_command() -> None:
        _run_typer_action(lambda: typer.echo(run_doctor()))

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
        output_format: str = typer.Option(..., "--format", help="Export format: excel, png, html, pdf, docx, print-html."),
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
        output_format: str | None = typer.Option(None, "--format", help="Export format: excel, png, html, pdf, docx, print-html."),
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


# ---------------------------------------------------------------------------
# All business logic has moved to seattrellis.service.
# Re-export everything so existing callers (tests, web layer, argparse dispatch)
# can keep using ``cli.solve_with_report(...)`` etc.
# ---------------------------------------------------------------------------

from seattrellis.service import (  # noqa: E402, F401
    # Public API
    export,
    init_demo,
    project_export,
    project_info,
    project_init,
    project_solve,
    project_validate,
    run_doctor,
    run_history_report,
    run_pair_report,
    run_validate,
    solve,
    solve_with_report,
    # Private helpers still referenced by CLI dispatch
    _friendly_error,
    _solve_output_label,
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
    from seattrellis import __version__

    parser = argparse.ArgumentParser(prog="seattrellis", description="SeatTrellis classroom seating optimizer.")
    parser.add_argument("--version", "-V", action="version", version=f"seattrellis {__version__}")
    subparsers = parser.add_subparsers(dest="command", required=True)

    # doctor
    doctor_parser = subparsers.add_parser("doctor", help="Check environment.")
    doctor_parser.set_defaults(func=lambda args: print(run_doctor()))

    # init-demo
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
    if args.command == "doctor":
        print(run_doctor())
    elif args.command == "init-demo":
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


if __name__ == "__main__":
    main()
