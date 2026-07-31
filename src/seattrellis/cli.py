from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Mapping

from seattrellis.editing import (
    EditingOperation,
    EditingOperationKind,
    EditingPayloadValue,
)
from seattrellis.io.json_files import InputFileError
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.presets import (
    export_preset,
    format_preset,
    format_preset_list,
    get_preset,
)
from seattrellis.project_bundle import (
    list_recent_projects,
    pack_project,
    restore_project_bundle,
    scan_project_privacy,
)
from seattrellis.schema import (
    format_json_schema_artifacts,
    write_json_schema_files,
)
from seattrellis.schema_migration import migrate_json_file
from seattrellis.service_types import ExportRequest, PageOptions, PrivacyOptions
from seattrellis.solver import SeatTrellisSolveError
from seattrellis.solver.backend import SOLVER_BACKENDS

try:
    import typer
except Exception:  # pragma: no cover - used only when Typer is not installed.
    typer = None  # type: ignore[assignment]


def _build_export_request(
    *,
    output_format: str,
    output_path: str | Path | None,
    candidate_id: str | None,
    template: str,
    hide_score: bool,
    hide_notes: bool,
    hide_special_needs: bool,
    hide_height: bool,
    hide_vision: bool,
    anonymize: bool,
    orientation: str,
    scale: float,
    locale: str,
    candidate_scope: str = "selected",
) -> ExportRequest:
    privacy = None
    if any(
        [
            hide_score,
            hide_notes,
            hide_special_needs,
            hide_height,
            hide_vision,
            anonymize,
        ]
    ):
        defaults = PrivacyOptions.for_template(template)
        privacy = PrivacyOptions(
            hide_scores=defaults.hide_scores or hide_score,
            hide_notes=defaults.hide_notes or hide_notes,
            hide_special_needs=(
                defaults.hide_special_needs or hide_special_needs
            ),
            anonymize=anonymize,
            show_height=defaults.show_height and not hide_height,
            show_vision=defaults.show_vision and not hide_vision,
        )
    return ExportRequest(
        output_format=output_format,
        output_path=output_path,
        template=template,
        privacy=privacy,
        page=PageOptions(orientation=orientation, scale=scale),
        locale=locale,
        candidate_id=candidate_id,
        candidate_scope=candidate_scope,
    )


if typer is not None:
    app = typer.Typer(
        help="SeatTrellis classroom seating optimizer.",
        no_args_is_help=True,
    )
    presets_app = typer.Typer(
        help="List, inspect, and export built-in rules presets.",
        no_args_is_help=True,
    )
    schema_app = typer.Typer(
        help="Export JSON Schemas and normalize versioned JSON artifacts.",
        no_args_is_help=True,
    )
    app.add_typer(presets_app, name="presets")
    app.add_typer(schema_app, name="schema")

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

    @schema_app.command("list", help="List public JSON Schema documents.")
    def schema_list_command() -> None:
        typer.echo(format_json_schema_artifacts())

    @schema_app.command("export", help="Write public JSON Schema files.")
    def schema_export_command(
        output_dir: Path = typer.Option(
            Path("schemas"),
            "--output-dir",
            "-o",
            help="Directory for generated schema files.",
        ),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                "JSON Schema files written:\n"
                + "\n".join(str(path) for path in write_json_schema_files(output_dir))
            )
        )

    @schema_app.command("migrate", help="Validate and rewrite a versioned JSON artifact.")
    def schema_migrate_command(
        input_path: Path = typer.Option(..., "--input", "-i", help="Artifact JSON path."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Migrated JSON path."),
        in_place: bool = typer.Option(False, "--in-place", help="Rewrite the input file in place."),
        dry_run: bool = typer.Option(False, "--dry-run", help="Validate without writing files."),
        backup: bool = typer.Option(
            True,
            "--backup/--no-backup",
            help="Back up an existing destination before replacing it.",
        ),
    ) -> None:
        _run_typer_action(
            lambda: _print_schema_migration(
                migrate_json_file(
                    input_path,
                    output=output,
                    in_place=in_place,
                    dry_run=dry_run,
                    create_backup=backup,
                )
            )
        )

    @app.command("doctor", help="Check environment: Python, optional deps, examples, outputs.")
    def doctor_command() -> None:
        _run_typer_action(lambda: typer.echo(run_doctor()))

    @app.command(
        "workspace",
        help="Open the local browser workbench for everyday classroom planning.",
    )
    def workspace_command(
        host: str = typer.Option(
            "127.0.0.1",
            "--host",
            help="Loopback address used by the local workbench.",
        ),
        port: int = typer.Option(
            8765,
            "--port",
            help="Local workbench port.",
        ),
        open_browser: bool = typer.Option(
            True,
            "--open-browser/--no-open-browser",
            help="Open the workbench in the default browser after startup.",
        ),
    ) -> None:
        def start_workspace() -> None:
            from seattrellis.workspace_server import (
                WorkspaceServerOptions,
                run_workspace_server,
            )

            options = WorkspaceServerOptions(
                host=host,
                port=port,
                open_browser=open_browser,
            )
            typer.echo(f"SeatTrellis workspace: {options.browser_url}")
            run_workspace_server(options=options)

        _run_typer_action(start_workspace)

    @app.command(
        "desktop",
        help="Open the optional pywebview desktop workbench.",
    )
    def desktop_command(
        width: int = typer.Option(1280, "--width", help="Window width."),
        height: int = typer.Option(900, "--height", help="Window height."),
    ) -> None:
        def start_desktop() -> None:
            from seattrellis.desktop import DesktopOptions, run_desktop_app

            typer.echo("Starting SeatTrellis desktop workbench on the local machine.")
            run_desktop_app(options=DesktopOptions(width=width, height=height))

        _run_typer_action(start_desktop)

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
        backend: str = typer.Option(
            "auto",
            "--backend",
            help=f"Solver backend: {', '.join(SOLVER_BACKENDS)}.",
        ),
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
                    backend=backend,
                    candidate_count=candidates,
                    seed=seed,
                    report_path=report,
                )
            )
        )

    @app.command(
        "rotation-plan",
        help="Generate several future seating periods and a fairness summary.",
    )
    def rotation_plan_command(
        students: Path = typer.Option(..., "--students", help="CSV or Excel student file."),
        layout: Path = typer.Option(..., "--layout", help="Classroom layout JSON."),
        rules: Path | None = typer.Option(None, "--rules", help="Optional rules JSON."),
        preset: str | None = typer.Option(None, "--preset", help="Built-in rules preset name."),
        history: list[Path] = typer.Option([], "--history", help="Historical snapshot path; repeatable."),
        history_dir: Path | None = typer.Option(None, "--history-dir", help="Historical snapshot directory."),
        periods: int = typer.Option(4, "--periods", min=1, max=20, help="Number of future periods."),
        label: list[str] = typer.Option([], "--label", help="Period label; repeat once per period."),
        name: str = typer.Option("SeatTrellis Rotation Plan", "--name", help="Plan display name."),
        seed: int | None = typer.Option(None, "--seed", help="Base seed; each period advances it."),
        time_limit_seconds: float = typer.Option(3.0, "--time-limit", help="Solver time limit per period."),
        backend: str = typer.Option("auto", "--backend", help=f"Solver backend: {', '.join(SOLVER_BACKENDS)}."),
        output: Path = typer.Option(Path("outputs/rotation-plan.json"), "--output", "-o", help="Rotation plan JSON path."),
    ) -> None:
        _run_typer_action(
            lambda: _print_rotation_result(
                generate_rotation_plan(
                    students_path=students,
                    layout_path=layout,
                    rules_path=rules,
                    preset_name=preset,
                    history_paths=history,
                    history_dir=history_dir,
                    period_count=periods,
                    period_labels=label,
                    name=name,
                    seed=seed,
                    time_limit_seconds=time_limit_seconds,
                    backend=backend,
                    output_path=output,
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

    @app.command(
        "export",
        help="Export a snapshot or candidate to a supported output format.",
    )
    def export_command(
        snapshot: Path = typer.Option(..., "--snapshot", help="Snapshot JSON path."),
        output_format: str = typer.Option(..., "--format", help="Export format: excel, png, html, pdf, docx, print-html, svg, pptx."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Output file path."),
        candidate: str | None = typer.Option(
            None,
            "--candidate",
            help="Candidate ID for a candidate set, or 'recommended'.",
        ),
        candidate_scope: str = typer.Option(
            "selected",
            "--candidate-scope",
            help="Candidate scope: selected or all.",
        ),
        template: str = typer.Option(
            "public",
            "--template",
            help="Print template: public, teacher, or report.",
        ),
        hide_score: bool = typer.Option(
            False, "--hide-score", help="Hide student scores."
        ),
        hide_notes: bool = typer.Option(
            False, "--hide-notes", help="Hide student notes."
        ),
        hide_special_needs: bool = typer.Option(
            False,
            "--hide-special-needs",
            help="Hide student needs and tags.",
        ),
        hide_height: bool = typer.Option(
            False, "--hide-height", help="Hide student height."
        ),
        hide_vision: bool = typer.Option(
            False, "--hide-vision", help="Hide student vision information."
        ),
        anonymize: bool = typer.Option(
            False, "--anonymize", help="Replace student names with stable labels."
        ),
        orientation: str = typer.Option(
            "portrait",
            "--orientation",
            help="A4 page orientation: portrait or landscape.",
        ),
        scale: float = typer.Option(
            1.0, "--page-scale", help="Print scale from 0.5 to 2.0."
        ),
        locale: str = typer.Option(
            "zh", "--locale", help="Export locale: zh or en."
        ),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                "Export written to "
                + str(
                    export(
                        snapshot_path=snapshot,
                        request=_build_export_request(
                            output_format=output_format,
                            output_path=output,
                            candidate_id=candidate,
                            template=template,
                            hide_score=hide_score,
                            hide_notes=hide_notes,
                            hide_special_needs=hide_special_needs,
                            hide_height=hide_height,
                            hide_vision=hide_vision,
                            anonymize=anonymize,
                            orientation=orientation,
                            scale=scale,
                            locale=locale,
                            candidate_scope=candidate_scope,
                        ),
                    )
                )
            )
        )

    @app.command(
        "edit",
        help="Apply manual edit operations to a snapshot or candidate set.",
    )
    def edit_command(
        snapshot: Path = typer.Option(..., "--snapshot", help="Snapshot or candidate-set JSON path."),
        candidate: str | None = typer.Option(
            None,
            "--candidate",
            help="Candidate ID for a candidate set, or 'recommended'.",
        ),
        operation: list[str] = typer.Option(
            [],
            "--operation",
            "--op",
            help=(
                "Operation to apply, repeatable and ordered. Examples: "
                "swap:STU001:STU002, move:STU003:R2C2, unseat:STU004, "
                "lock-seat:R1C1."
            ),
        ),
        operations_file: Path | None = typer.Option(
            None,
            "--operations-file",
            help=(
                "JSON operation log to apply before any --operation values. "
                "Use a list or an object with an operations list."
            ),
        ),
        output: Path = typer.Option(
            Path("outputs/edited.snapshot.json"),
            "--output",
            "-o",
            help="Edited snapshot output path.",
        ),
        strict: bool = typer.Option(
            False,
            "--strict",
            help="Fail instead of writing when hard constraints are not satisfied.",
        ),
    ) -> None:
        _run_typer_action(
            lambda: _print_edit_result(
                edit_snapshot(
                    snapshot_path=snapshot,
                    output_path=output,
                    operations=_parse_edit_operations(
                        operation,
                        operations_file=operations_file,
                    ),
                    candidate_id=candidate,
                    strict=strict,
                )
            )
        )

    @app.command(
        "repair",
        help="Re-solve a seating draft while preserving locks or a local scope.",
    )
    def repair_command(
        snapshot: Path = typer.Option(
            ...,
            "--snapshot",
            help="Snapshot or candidate-set JSON path.",
        ),
        candidate: str | None = typer.Option(
            None,
            "--candidate",
            help="Candidate ID for a candidate set, or 'recommended'.",
        ),
        affected_student: list[str] = typer.Option(
            [],
            "--affected-student",
            help="Student key to include in the local repair. Can be repeated.",
        ),
        lock_student: list[str] = typer.Option(
            [],
            "--lock-student",
            help="Keep a student's current seat for this re-solve. Can be repeated.",
        ),
        lock_seat: list[str] = typer.Option(
            [],
            "--lock-seat",
            help="Keep a seat's occupant or reserve an empty seat. Can be repeated.",
        ),
        history: list[Path] = typer.Option(
            [],
            "--history",
            help="Historical snapshot JSON path. Can be repeated.",
        ),
        history_dir: Path | None = typer.Option(
            None,
            "--history-dir",
            help="Directory containing historical *.snapshot.json files.",
        ),
        ignore_saved_locks: bool = typer.Option(
            False,
            "--ignore-saved-locks",
            help="Do not reuse locks recorded by a prior edit operation.",
        ),
        seed: int | None = typer.Option(
            None,
            "--seed",
            help="Override the source snapshot seed.",
        ),
        time_limit_seconds: float = typer.Option(
            3.0,
            "--time-limit",
            help="Solver time limit in seconds.",
        ),
        backend: str = typer.Option(
            "auto",
            "--backend",
            help=f"Solver backend: {', '.join(SOLVER_BACKENDS)}.",
        ),
        output: Path = typer.Option(
            Path("outputs/repaired.snapshot.json"),
            "--output",
            "-o",
            help="Repaired snapshot output path.",
        ),
    ) -> None:
        _run_typer_action(
            lambda: _print_repair_result(
                repair_snapshot(
                    snapshot_path=snapshot,
                    output_path=output,
                    candidate_id=candidate,
                    affected_students=affected_student,
                    locked_students=lock_student,
                    locked_seats=lock_seat,
                    history_paths=history,
                    history_dir=history_dir,
                    reuse_saved_locks=not ignore_saved_locks,
                    seed=seed,
                    time_limit_seconds=time_limit_seconds,
                    backend=backend,
                )
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

    @app.command("project-list", help="List recent local SeatTrellis projects.")
    def project_list_command(
        root: Path = typer.Option(Path("."), "--root", help="Directory to search."),
        limit: int = typer.Option(20, "--limit", min=1, max=100, help="Maximum projects to show."),
    ) -> None:
        def render() -> None:
            projects = list_recent_projects(root, limit=limit)
            if not projects:
                typer.echo("No SeatTrellis projects found.")
                return
            for item in projects:
                typer.echo(f"{item.name}\t{item.path}\t{item.modified_at.isoformat()}")

        _run_typer_action(render)

    @app.command("project-privacy", help="Scan a project for sensitive fields before sharing.")
    def project_privacy_command(
        project: Path = typer.Option(Path("seattrellis.project.json"), "--project", help="Project JSON path."),
        include_outputs: bool = typer.Option(True, "--include-outputs/--no-include-outputs", help="Include generated outputs in the scan."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                scan_project_privacy(project, include_outputs=include_outputs).format()
            )
        )

    @app.command("project-pack", help="Back up a project as a .seattrellis.zip file.")
    def project_pack_command(
        project: Path = typer.Option(Path("seattrellis.project.json"), "--project", help="Project JSON path."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Bundle output path."),
        include_outputs: bool = typer.Option(True, "--include-outputs/--no-include-outputs", help="Include generated outputs."),
        force: bool = typer.Option(False, "--force", help="Replace an existing bundle."),
    ) -> None:
        def pack() -> None:
            result = pack_project(project, output, include_outputs=include_outputs, overwrite=force)
            typer.echo(f"Project bundle written to {result.path} ({result.file_count} files).")
            if not result.privacy.safe_for_public_sharing:
                typer.echo("Privacy note: the backup contains sensitive fields; review before sharing.")

        _run_typer_action(pack)

    @app.command("project-restore", help="Restore a .seattrellis.zip project backup.")
    def project_restore_command(
        bundle: Path = typer.Option(..., "--bundle", "-b", help="Project bundle path."),
        output_dir: Path = typer.Option(..., "--output-dir", "-o", help="Restore directory."),
        force: bool = typer.Option(False, "--force", help="Merge into a non-empty directory."),
    ) -> None:
        _run_typer_action(
            lambda: typer.echo(
                f"Project restored to {restore_project_bundle(bundle, output_dir, overwrite=force)}"
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
        backend: str = typer.Option(
            "auto",
            "--backend",
            help=f"Solver backend: {', '.join(SOLVER_BACKENDS)}.",
        ),
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
                    backend=backend,
                    output_path=output,
                    report_path=report,
                )
            )
        )

    @app.command("project-rotate", help="Generate future seating periods from a project file.")
    def project_rotate_command(
        project: Path = typer.Option(Path("seattrellis.project.json"), "--project", help="Project JSON path."),
        periods: int = typer.Option(4, "--periods", min=1, max=20, help="Number of future periods."),
        label: list[str] = typer.Option([], "--label", help="Period label; repeat once per period."),
        seed: int | None = typer.Option(None, "--seed", help="Base seed; each period advances it."),
        time_limit_seconds: float = typer.Option(3.0, "--time-limit", help="Solver time limit per period."),
        backend: str = typer.Option("auto", "--backend", help=f"Solver backend: {', '.join(SOLVER_BACKENDS)}."),
        output: Path | None = typer.Option(None, "--output", "-o", help="Rotation plan JSON path."),
    ) -> None:
        _run_typer_action(
            lambda: _print_rotation_result(
                project_rotate(
                    project_path=project,
                    period_count=periods,
                    period_labels=label,
                    seed=seed,
                    time_limit_seconds=time_limit_seconds,
                    backend=backend,
                    output_path=output,
                )
            )
        )

    @app.command("project-edit", help="Apply manual edits to a project seating artifact.")
    def project_edit_command(
        project: Path = typer.Option(
            Path("seattrellis.project.json"),
            "--project",
            help="Project JSON path.",
        ),
        snapshot: Path | None = typer.Option(
            None,
            "--snapshot",
            help="Snapshot or candidate-set JSON path. Defaults to latest project artifact.",
        ),
        candidate: str | None = typer.Option(
            None,
            "--candidate",
            help="Candidate ID, or 'recommended'.",
        ),
        operation: list[str] = typer.Option(
            [],
            "--operation",
            "--op",
            help="Operation to apply, repeatable and ordered.",
        ),
        operations_file: Path | None = typer.Option(
            None,
            "--operations-file",
            help=(
                "JSON operation log to apply before any --operation values. "
                "Use a list or an object with an operations list."
            ),
        ),
        output: Path | None = typer.Option(None, "--output", "-o", help="Edited snapshot output path."),
        strict: bool = typer.Option(
            False,
            "--strict",
            help="Fail instead of writing when hard constraints are not satisfied.",
        ),
    ) -> None:
        _run_typer_action(
            lambda: _print_edit_result(
                project_edit(
                    project_path=project,
                    snapshot_path=snapshot,
                    candidate_id=candidate,
                    operations=_parse_edit_operations(
                        operation,
                        operations_file=operations_file,
                    ),
                    output_path=output,
                    strict=strict,
                )
            )
        )

    @app.command(
        "project-repair",
        help="Re-solve the latest or selected project artifact with draft locks.",
    )
    def project_repair_command(
        project: Path = typer.Option(
            Path("seattrellis.project.json"),
            "--project",
            help="Project JSON path.",
        ),
        snapshot: Path | None = typer.Option(
            None,
            "--snapshot",
            help="Snapshot or candidate-set JSON path. Defaults to latest project artifact.",
        ),
        candidate: str | None = typer.Option(
            None,
            "--candidate",
            help="Candidate ID, or 'recommended'.",
        ),
        affected_student: list[str] = typer.Option(
            [],
            "--affected-student",
            help="Student key to include in the local repair. Can be repeated.",
        ),
        lock_student: list[str] = typer.Option(
            [],
            "--lock-student",
            help="Keep a student's current seat. Can be repeated.",
        ),
        lock_seat: list[str] = typer.Option(
            [],
            "--lock-seat",
            help="Keep an occupant or reserve an empty seat. Can be repeated.",
        ),
        ignore_saved_locks: bool = typer.Option(
            False,
            "--ignore-saved-locks",
            help="Do not reuse locks recorded by a prior edit operation.",
        ),
        seed: int | None = typer.Option(
            None,
            "--seed",
            help="Override the source snapshot seed.",
        ),
        time_limit_seconds: float = typer.Option(
            3.0,
            "--time-limit",
            help="Solver time limit in seconds.",
        ),
        backend: str = typer.Option(
            "auto",
            "--backend",
            help=f"Solver backend: {', '.join(SOLVER_BACKENDS)}.",
        ),
        output: Path | None = typer.Option(
            None,
            "--output",
            "-o",
            help="Repaired snapshot output path.",
        ),
    ) -> None:
        _run_typer_action(
            lambda: _print_repair_result(
                project_repair(
                    project_path=project,
                    snapshot_path=snapshot,
                    candidate_id=candidate,
                    affected_students=affected_student,
                    locked_students=lock_student,
                    locked_seats=lock_seat,
                    reuse_saved_locks=not ignore_saved_locks,
                    seed=seed,
                    time_limit_seconds=time_limit_seconds,
                    backend=backend,
                    output_path=output,
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
        output_format: str | None = typer.Option(None, "--format", help="Export format: excel, png, html, pdf, docx, print-html, svg, pptx."),
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
    edit_snapshot,
    export,
    generate_rotation_plan,
    init_demo,
    project_edit,
    project_export,
    project_info,
    project_init,
    project_solve,
    project_validate,
    project_repair,
    project_rotate,
    repair_snapshot,
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

    workspace_parser = subparsers.add_parser(
        "workspace",
        help="Open the local browser workbench.",
    )
    workspace_parser.add_argument("--host", default="127.0.0.1")
    workspace_parser.add_argument("--port", type=int, default=8765)
    workspace_parser.add_argument("--no-open-browser", action="store_true")

    desktop_parser = subparsers.add_parser(
        "desktop",
        help="Open the optional pywebview desktop workbench.",
    )
    desktop_parser.add_argument("--width", type=int, default=1280)
    desktop_parser.add_argument("--height", type=int, default=900)

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

    schema_parser = subparsers.add_parser("schema", help="Manage JSON Schemas and migrations.")
    schema_subparsers = schema_parser.add_subparsers(dest="schema_command", required=True)
    schema_subparsers.add_parser("list", help="List public JSON Schema documents.")
    schema_export_parser = schema_subparsers.add_parser("export", help="Write public JSON Schema files.")
    schema_export_parser.add_argument("--output-dir", "-o", default="schemas")
    schema_migrate_parser = schema_subparsers.add_parser("migrate", help="Validate and rewrite a versioned JSON artifact.")
    schema_migrate_parser.add_argument("--input", "-i", required=True)
    schema_migrate_parser.add_argument("--output", "-o", default=None)
    schema_migrate_parser.add_argument("--in-place", action="store_true")
    schema_migrate_parser.add_argument("--dry-run", action="store_true")
    schema_migrate_parser.add_argument("--no-backup", action="store_true")

    solve_parser = subparsers.add_parser("solve", help="Generate a seating snapshot.")
    solve_parser.add_argument("--students", required=True)
    solve_parser.add_argument("--layout", required=True)
    solve_parser.add_argument("--rules", default=None)
    solve_parser.add_argument("--preset", default=None)
    solve_parser.add_argument("--output", "-o", default="outputs/latest.snapshot.json")
    solve_parser.add_argument("--history", action="append", default=[])
    solve_parser.add_argument("--history-dir", default=None)
    solve_parser.add_argument("--time-limit", type=float, default=3.0)
    solve_parser.add_argument("--backend", choices=SOLVER_BACKENDS, default="auto")
    solve_parser.add_argument("--candidates", type=int, default=1)
    solve_parser.add_argument("--seed", type=int, default=None)
    solve_parser.add_argument("--report", default=None)

    rotation_parser = subparsers.add_parser(
        "rotation-plan", help="Generate several future seating periods."
    )
    rotation_parser.add_argument("--students", required=True)
    rotation_parser.add_argument("--layout", required=True)
    rotation_parser.add_argument("--rules", default=None)
    rotation_parser.add_argument("--preset", default=None)
    rotation_parser.add_argument("--history", action="append", default=[])
    rotation_parser.add_argument("--history-dir", default=None)
    rotation_parser.add_argument("--periods", type=int, default=4)
    rotation_parser.add_argument("--label", action="append", default=[])
    rotation_parser.add_argument("--name", default="SeatTrellis Rotation Plan")
    rotation_parser.add_argument("--seed", type=int, default=None)
    rotation_parser.add_argument("--time-limit", type=float, default=3.0)
    rotation_parser.add_argument("--backend", choices=SOLVER_BACKENDS, default="auto")
    rotation_parser.add_argument("--output", "-o", default="outputs/rotation-plan.json")

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
    export_parser.add_argument(
        "--candidate-scope",
        choices=["selected", "all"],
        default="selected",
    )
    export_parser.add_argument(
        "--template",
        choices=["public", "teacher", "report"],
        default="public",
    )
    export_parser.add_argument("--hide-score", action="store_true")
    export_parser.add_argument("--hide-notes", action="store_true")
    export_parser.add_argument("--hide-special-needs", action="store_true")
    export_parser.add_argument("--hide-height", action="store_true")
    export_parser.add_argument("--hide-vision", action="store_true")
    export_parser.add_argument("--anonymize", action="store_true")
    export_parser.add_argument(
        "--orientation",
        choices=["portrait", "landscape"],
        default="portrait",
    )
    export_parser.add_argument("--page-scale", type=float, default=1.0)
    export_parser.add_argument("--locale", choices=["zh", "en"], default="zh")

    edit_parser = subparsers.add_parser(
        "edit",
        help="Apply manual edits to a snapshot or candidate set.",
    )
    edit_parser.add_argument("--snapshot", required=True)
    edit_parser.add_argument("--candidate", default=None)
    edit_parser.add_argument("--operation", "--op", dest="operations", action="append", default=[])
    edit_parser.add_argument("--operations-file", default=None)
    edit_parser.add_argument("--output", "-o", default="outputs/edited.snapshot.json")
    edit_parser.add_argument("--strict", action="store_true")

    repair_parser = subparsers.add_parser(
        "repair",
        help="Re-solve a seating draft while preserving locks or a local scope.",
    )
    repair_parser.add_argument("--snapshot", required=True)
    repair_parser.add_argument("--candidate", default=None)
    repair_parser.add_argument("--affected-student", action="append", default=[])
    repair_parser.add_argument("--lock-student", action="append", default=[])
    repair_parser.add_argument("--lock-seat", action="append", default=[])
    repair_parser.add_argument("--history", action="append", default=[])
    repair_parser.add_argument("--history-dir", default=None)
    repair_parser.add_argument("--ignore-saved-locks", action="store_true")
    repair_parser.add_argument("--seed", type=int, default=None)
    repair_parser.add_argument("--time-limit", type=float, default=3.0)
    repair_parser.add_argument("--backend", choices=SOLVER_BACKENDS, default="auto")
    repair_parser.add_argument("--output", "-o", default="outputs/repaired.snapshot.json")

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

    project_list_parser = subparsers.add_parser("project-list", help="List recent local projects.")
    project_list_parser.add_argument("--root", default=".")
    project_list_parser.add_argument("--limit", type=int, default=20)

    project_privacy_parser = subparsers.add_parser("project-privacy", help="Scan a project for sensitive fields.")
    project_privacy_parser.add_argument("--project", default="seattrellis.project.json")
    project_privacy_parser.add_argument("--no-include-outputs", action="store_true")

    project_pack_parser = subparsers.add_parser("project-pack", help="Create a project backup bundle.")
    project_pack_parser.add_argument("--project", default="seattrellis.project.json")
    project_pack_parser.add_argument("--output", "-o", default=None)
    project_pack_parser.add_argument("--no-include-outputs", action="store_true")
    project_pack_parser.add_argument("--force", action="store_true")

    project_restore_parser = subparsers.add_parser("project-restore", help="Restore a project backup bundle.")
    project_restore_parser.add_argument("--bundle", "-b", required=True)
    project_restore_parser.add_argument("--output-dir", "-o", required=True)
    project_restore_parser.add_argument("--force", action="store_true")

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
    project_solve_parser.add_argument("--backend", choices=SOLVER_BACKENDS, default="auto")
    project_solve_parser.add_argument("--output", "-o", default=None)
    project_solve_parser.add_argument("--report", default=None)

    project_rotate_parser = subparsers.add_parser(
        "project-rotate", help="Generate future seating periods from a project."
    )
    project_rotate_parser.add_argument("--project", default="seattrellis.project.json")
    project_rotate_parser.add_argument("--periods", type=int, default=4)
    project_rotate_parser.add_argument("--label", action="append", default=[])
    project_rotate_parser.add_argument("--seed", type=int, default=None)
    project_rotate_parser.add_argument("--time-limit", type=float, default=3.0)
    project_rotate_parser.add_argument("--backend", choices=SOLVER_BACKENDS, default="auto")
    project_rotate_parser.add_argument("--output", "-o", default=None)

    project_edit_parser = subparsers.add_parser("project-edit", help="Edit a project artifact.")
    project_edit_parser.add_argument("--project", default="seattrellis.project.json")
    project_edit_parser.add_argument("--snapshot", default=None)
    project_edit_parser.add_argument("--candidate", default=None)
    project_edit_parser.add_argument("--operation", "--op", dest="operations", action="append", default=[])
    project_edit_parser.add_argument("--operations-file", default=None)
    project_edit_parser.add_argument("--output", "-o", default=None)
    project_edit_parser.add_argument("--strict", action="store_true")

    project_repair_parser = subparsers.add_parser(
        "project-repair",
        help="Re-solve a project artifact while preserving locks or a local scope.",
    )
    project_repair_parser.add_argument("--project", default="seattrellis.project.json")
    project_repair_parser.add_argument("--snapshot", default=None)
    project_repair_parser.add_argument("--candidate", default=None)
    project_repair_parser.add_argument("--affected-student", action="append", default=[])
    project_repair_parser.add_argument("--lock-student", action="append", default=[])
    project_repair_parser.add_argument("--lock-seat", action="append", default=[])
    project_repair_parser.add_argument("--ignore-saved-locks", action="store_true")
    project_repair_parser.add_argument("--seed", type=int, default=None)
    project_repair_parser.add_argument("--time-limit", type=float, default=3.0)
    project_repair_parser.add_argument("--backend", choices=SOLVER_BACKENDS, default="auto")
    project_repair_parser.add_argument("--output", "-o", default=None)

    project_export_parser = subparsers.add_parser("project-export", help="Export a project artifact.")
    project_export_parser.add_argument("--project", default="seattrellis.project.json")
    project_export_parser.add_argument("--snapshot", default=None)
    project_export_parser.add_argument("--format", default=None)
    project_export_parser.add_argument("--candidate", default=None)
    project_export_parser.add_argument("--output", "-o", default=None)

    args = parser.parse_args()
    if args.command == "doctor":
        print(run_doctor())
    elif args.command == "workspace":
        from seattrellis.workspace_server import (
            WorkspaceServerOptions,
            run_workspace_server,
        )

        options = WorkspaceServerOptions(
            host=args.host,
            port=args.port,
            open_browser=not args.no_open_browser,
        )
        print(f"SeatTrellis workspace: {options.browser_url}")
        run_workspace_server(options=options)
    elif args.command == "desktop":
        from seattrellis.desktop import DesktopOptions, run_desktop_app

        print("Starting SeatTrellis desktop workbench on the local machine.")
        run_desktop_app(options=DesktopOptions(width=args.width, height=args.height))
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
    elif args.command == "schema":
        if args.schema_command == "list":
            print(format_json_schema_artifacts())
        elif args.schema_command == "export":
            paths = write_json_schema_files(args.output_dir)
            print("JSON Schema files written:")
            for path in paths:
                print(path)
        elif args.schema_command == "migrate":
            _print_schema_migration(
                migrate_json_file(
                    args.input,
                    output=args.output,
                    in_place=args.in_place,
                    dry_run=args.dry_run,
                    create_backup=not args.no_backup,
                )
            )
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
            backend=args.backend,
            candidate_count=args.candidates,
            seed=args.seed,
            report_path=args.report,
        )
        print(f"{_solve_output_label(summary)} written to {path}")
        if summary:
            print(summary)
    elif args.command == "rotation-plan":
        path, summary = generate_rotation_plan(
            students_path=args.students,
            layout_path=args.layout,
            rules_path=args.rules,
            preset_name=args.preset,
            history_paths=args.history,
            history_dir=args.history_dir,
            period_count=args.periods,
            period_labels=args.label,
            name=args.name,
            seed=args.seed,
            time_limit_seconds=args.time_limit,
            backend=args.backend,
            output_path=args.output,
        )
        print(f"Rotation plan written to {path}")
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
            request=_build_export_request(
                output_format=args.format,
                output_path=args.output,
                candidate_id=args.candidate,
                template=args.template,
                hide_score=args.hide_score,
                hide_notes=args.hide_notes,
                hide_special_needs=args.hide_special_needs,
                hide_height=args.hide_height,
                hide_vision=args.hide_vision,
                anonymize=args.anonymize,
                orientation=args.orientation,
                scale=args.page_scale,
                locale=args.locale,
                candidate_scope=args.candidate_scope,
            ),
        )
        print(f"Export written to {path}")
    elif args.command == "edit":
        path, summary = edit_snapshot(
            snapshot_path=args.snapshot,
            output_path=args.output,
            operations=_parse_edit_operations(
                args.operations,
                operations_file=args.operations_file,
            ),
            candidate_id=args.candidate,
            strict=args.strict,
        )
        print(f"Edited snapshot written to {path}")
        print(summary)
    elif args.command == "repair":
        path, summary = repair_snapshot(
            snapshot_path=args.snapshot,
            output_path=args.output,
            candidate_id=args.candidate,
            affected_students=args.affected_student,
            locked_students=args.lock_student,
            locked_seats=args.lock_seat,
            history_paths=args.history,
            history_dir=args.history_dir,
            reuse_saved_locks=not args.ignore_saved_locks,
            seed=args.seed,
            time_limit_seconds=args.time_limit,
            backend=args.backend,
        )
        print(f"Repaired snapshot written to {path}")
        print(summary)
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
    elif args.command == "project-list":
        projects = list_recent_projects(args.root, limit=args.limit)
        if not projects:
            print("No SeatTrellis projects found.")
        else:
            for item in projects:
                print(f"{item.name}\t{item.path}\t{item.modified_at.isoformat()}")
    elif args.command == "project-privacy":
        print(
            scan_project_privacy(
                args.project,
                include_outputs=not args.no_include_outputs,
            ).format()
        )
    elif args.command == "project-pack":
        result = pack_project(
            args.project,
            args.output,
            include_outputs=not args.no_include_outputs,
            overwrite=args.force,
        )
        print(f"Project bundle written to {result.path} ({result.file_count} files).")
    elif args.command == "project-restore":
        print(
            f"Project restored to {restore_project_bundle(args.bundle, args.output_dir, overwrite=args.force)}"
        )
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
            backend=args.backend,
            output_path=args.output,
            report_path=args.report,
        )
        print(f"{_solve_output_label(summary)} written to {path}")
        if summary:
            print(summary)
    elif args.command == "project-rotate":
        path, summary = project_rotate(
            project_path=args.project,
            period_count=args.periods,
            period_labels=args.label,
            seed=args.seed,
            time_limit_seconds=args.time_limit,
            backend=args.backend,
            output_path=args.output,
        )
        print(f"Rotation plan written to {path}")
        print(summary)
    elif args.command == "project-edit":
        path, summary = project_edit(
            project_path=args.project,
            snapshot_path=args.snapshot,
            candidate_id=args.candidate,
            operations=_parse_edit_operations(
                args.operations,
                operations_file=args.operations_file,
            ),
            output_path=args.output,
            strict=args.strict,
        )
        print(f"Edited snapshot written to {path}")
        print(summary)
    elif args.command == "project-repair":
        path, summary = project_repair(
            project_path=args.project,
            snapshot_path=args.snapshot,
            candidate_id=args.candidate,
            affected_students=args.affected_student,
            locked_students=args.lock_student,
            locked_seats=args.lock_seat,
            reuse_saved_locks=not args.ignore_saved_locks,
            seed=args.seed,
            time_limit_seconds=args.time_limit,
            backend=args.backend,
            output_path=args.output,
        )
        print(f"Repaired snapshot written to {path}")
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


def _print_edit_result(result: tuple[Path, str]) -> None:
    path, summary = result
    typer.echo(f"Edited snapshot written to {path}")
    typer.echo(summary)


def _print_repair_result(result: tuple[Path, str]) -> None:
    path, summary = result
    typer.echo(f"Repaired snapshot written to {path}")
    typer.echo(summary)


def _print_rotation_result(result: tuple[Path, str]) -> None:
    path, summary = result
    typer.echo(f"Rotation plan written to {path}")
    typer.echo(summary)


def _print_schema_migration(result) -> None:
    if result.dry_run:
        destination = (
            f"; target would be {result.output_path}"
            if result.output_path is not None
            else ""
        )
        message = (
            f"{result.artifact} schema_version {result.schema_version!r} is valid"
            f"{destination}; no files written"
        )
    else:
        message = (
            f"{result.artifact} schema_version {result.schema_version!r} "
            f"written to {result.output_path}"
        )
        if result.backup_path is not None:
            message += f"\nBackup: {result.backup_path}"
    if typer is not None:
        typer.echo(message)
    else:
        print(message)


def _parse_edit_operations(
    values: list[str],
    *,
    operations_file: str | Path | None = None,
) -> list[EditingOperation]:
    """Collect file-backed operations before inline operations.

    A saved operation log is deliberately applied first, then any inline
    operations are appended. This produces a deterministic order across both
    Typer and argparse entry points.
    """

    operations: list[EditingOperation] = []
    if operations_file is not None:
        operations.extend(_load_edit_operations_file(Path(operations_file)))
    operations.extend(_parse_edit_operation(value) for value in values)
    if not operations:
        raise ValueError(
            "Provide at least one --operation value or an --operations-file."
        )
    return operations


def _load_edit_operations_file(path: Path) -> list[EditingOperation]:
    """Read a portable JSON operation log used by the CLI and future UIs."""

    try:
        contents = path.read_text(encoding="utf-8")
    except FileNotFoundError as exc:
        raise ValueError(f"Editing operation file not found: {path}") from exc
    except OSError as exc:
        raise ValueError(f"Editing operation file could not be read: {path}") from exc
    try:
        data = json.loads(contents)
    except json.JSONDecodeError as exc:
        raise ValueError(f"Editing operation file is not valid JSON: {path}") from exc

    if isinstance(data, list):
        entries = data
    elif isinstance(data, dict):
        entries = data.get("operations")
        if entries is None:
            raise ValueError(
                f"Editing operation file {path} must contain an 'operations' list."
            )
    else:
        raise ValueError(
            f"Editing operation file {path} must be a JSON list or object."
        )
    if not isinstance(entries, list):
        raise ValueError(
            f"Editing operation file {path} must contain an 'operations' list."
        )

    operations: list[EditingOperation] = []
    for index, entry in enumerate(entries, start=1):
        operations.append(
            _parse_edit_operation_mapping(
                entry,
                source=f"Editing operation file {path}, item {index}",
            )
        )
    return operations


def _parse_edit_operation_mapping(
    entry: object,
    *,
    source: str,
) -> EditingOperation:
    if not isinstance(entry, dict):
        raise ValueError(f"{source} must be an object with kind and payload.")
    raw_kind = entry.get("kind")
    if not isinstance(raw_kind, str) or not raw_kind.strip():
        raise ValueError(f"{source} must contain a non-empty string kind.")
    payload = entry.get("payload")
    if not isinstance(payload, Mapping):
        raise ValueError(f"{source} must contain a payload object.")

    normalized_payload: dict[str, EditingPayloadValue] = {}
    for key, value in payload.items():
        if not isinstance(key, str):
            raise ValueError(f"{source} payload keys must be strings.")
        normalized_payload[key] = _normalize_edit_payload_value(
            value,
            source=f"{source} payload.{key}",
        )

    kind = _normalize_edit_operation_kind(raw_kind)
    return EditingOperation(
        kind=kind,
        payload=normalized_payload,
    )


def _parse_edit_operation(value: str) -> EditingOperation:
    text = str(value).strip()
    if not text:
        raise ValueError("Editing operation cannot be empty.")
    parts = [part.strip() for part in text.split(":")]
    kind = _normalize_edit_operation_kind(parts[0])
    if kind == "swap_students":
        _require_operation_parts(text, parts, 3)
        return EditingOperation(
            kind="swap_students",
            payload={"first_student": parts[1], "second_student": parts[2]},
        )
    if kind in {"move_student", "seat_student"}:
        _require_operation_parts(text, parts, 3)
        return EditingOperation(
            kind=kind,
            payload={"student_key": parts[1], "seat_id": parts[2]},
        )
    if kind == "batch_move":
        _require_operation_parts(text, parts, 2)
        moves: list[dict[str, str]] = []
        for index, item in enumerate(parts[1].split(","), start=1):
            pair = [part.strip() for part in item.split("=", 1)]
            if len(pair) != 2 or not pair[0] or not pair[1]:
                raise ValueError(
                    f"Invalid batch move item {index} in operation {text!r}. "
                    "Use STUDENT=SEAT pairs separated by commas."
                )
            moves.append({"student_key": pair[0], "seat_id": pair[1]})
        return EditingOperation(kind="batch_move", payload={"moves": moves})
    if kind in {"unseat_student", "lock_student", "unlock_student"}:
        _require_operation_parts(text, parts, 2)
        return EditingOperation(
            kind=kind,
            payload={"student_key": parts[1]},
        )
    _require_operation_parts(text, parts, 2)
    return EditingOperation(
        kind=kind,
        payload={"seat_id": parts[1]},
    )


def _normalize_edit_operation_kind(value: str) -> EditingOperationKind:
    name = str(value).replace("-", "_").strip().lower()
    aliases: dict[str, EditingOperationKind] = {
        "swap": "swap_students",
        "swap_students": "swap_students",
        "move": "move_student",
        "move_student": "move_student",
        "batch": "batch_move",
        "batch_move": "batch_move",
        "seat": "seat_student",
        "seat_student": "seat_student",
        "unseat": "unseat_student",
        "unseat_student": "unseat_student",
        "lock_student": "lock_student",
        "unlock_student": "unlock_student",
        "lock_seat": "lock_seat",
        "unlock_seat": "unlock_seat",
    }
    kind = aliases.get(name)
    if kind is None:
        raise ValueError(
            f"Unsupported editing operation {value!r}. "
            "Use swap, move, batch-move, seat, unseat, lock-student, "
            "unlock-student, "
            "lock-seat, or unlock-seat."
        )
    return kind


def _normalize_edit_payload_value(
    value: object,
    *,
    source: str,
) -> EditingPayloadValue:
    if value is None or isinstance(value, (str, bool)):
        return value
    if isinstance(value, list):
        return [
            _normalize_edit_payload_value(item, source=f"{source}[{index}]")
            for index, item in enumerate(value)
        ]
    if isinstance(value, Mapping):
        normalized: dict[str, EditingPayloadValue] = {}
        for key, item in value.items():
            if not isinstance(key, str):
                raise ValueError(f"{source} object keys must be strings.")
            normalized[key] = _normalize_edit_payload_value(
                item,
                source=f"{source}.{key}",
            )
        return normalized
    raise ValueError(
        f"{source} must contain only strings, booleans, null, lists, or objects."
    )


def _require_operation_parts(text: str, parts: list[str], expected: int) -> None:
    if len(parts) != expected or any(not part for part in parts[1:]):
        examples = (
            "swap:STU001:STU002, move:STU003:R2C2, unseat:STU004, "
            "lock-seat:R1C1"
        )
        raise ValueError(f"Invalid editing operation {text!r}. Examples: {examples}.")


if __name__ == "__main__":
    main()
