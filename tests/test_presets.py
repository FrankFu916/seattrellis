from __future__ import annotations

import json
import sys

import pytest
from typer.testing import CliRunner

from seattrellis import cli
from seattrellis.io.json_files import InputFileError, load_candidate_set, load_rules
from seattrellis.io.students import read_students
from seattrellis.presets import (
    export_preset,
    get_preset,
    list_presets,
    load_rules_with_preset,
    preset_context_warnings,
)


def test_preset_catalog_contains_supported_scenarios() -> None:
    assert [preset.name for preset in list_presets()] == [
        "random",
        "exam",
        "daily",
        "fair-rotation",
        "neighbor-aware",
        "balanced",
        "height-aware",
        "vision-friendly",
    ]
    assert get_preset("fair_rotation").name == "fair-rotation"
    assert get_preset("daily").metadata()["requirements"] == [
        "history",
        "score",
        "height",
        "vision",
    ]
    with pytest.raises(InputFileError, match="Available presets"):
        get_preset("missing")


def test_rules_or_preset_is_required() -> None:
    with pytest.raises(InputFileError, match="Provide --rules, --preset, or both"):
        load_rules_with_preset()


def test_preset_export_is_a_standard_rules_json(tmp_path) -> None:
    path = export_preset("neighbor-aware", tmp_path / "neighbor.rules.json")
    rules = load_rules(path)
    raw = json.loads(path.read_text(encoding="utf-8"))

    assert rules.soft.avoid_recent_neighbors.enabled is True
    assert rules.soft.avoid_recent_neighbors.weight == 20
    assert raw.keys() == {"seed", "hard", "soft"}
    assert "preset" not in raw


def test_user_rules_overlay_preserves_hard_rules_and_overrides_nested_soft_fields(
    tmp_path,
) -> None:
    overlay = tmp_path / "overlay.json"
    overlay.write_text(
        json.dumps(
            {
                "seed": 99,
                "hard": {
                    "fixed_seats": [
                        {"student": "STU001", "seat_id": "R1C1"}
                    ]
                },
                "soft": {
                    "randomize": {"enabled": False, "weight": 0},
                    "fair_rotation": {"weight": 30},
                },
            }
        ),
        encoding="utf-8",
    )

    rules, preset = load_rules_with_preset(
        rules_path=overlay,
        preset_name="daily",
    )

    assert preset is not None and preset.name == "daily"
    assert rules.seed == 99
    assert rules.hard.fixed_seats[0].student == "STU001"
    assert rules.soft.randomize.enabled is False
    assert rules.soft.fair_rotation.enabled is True
    assert rules.soft.fair_rotation.weight == 30
    assert rules.soft.avoid_recent_neighbors.enabled is True


def test_preset_context_warnings_explain_graceful_degradation() -> None:
    students = read_students("tests/fixtures/students.csv")
    warnings = preset_context_warnings(
        get_preset("daily"),
        students,
        history_count=0,
    )

    assert any("preferred history data" in warning for warning in warnings)
    assert not any("preferred score data" in warning for warning in warnings)
    assert not any("preferred height data" in warning for warning in warnings)
    assert not any("preferred vision data" in warning for warning in warnings)


def test_user_overlay_can_disable_a_preset_data_requirement(tmp_path) -> None:
    overlay = tmp_path / "disable-history.json"
    overlay.write_text(
        json.dumps(
            {
                "soft": {
                    "fair_rotation": {"enabled": False, "weight": 0},
                    "avoid_recent_neighbors": {"enabled": False, "weight": 0},
                }
            }
        ),
        encoding="utf-8",
    )
    rules, preset = load_rules_with_preset(
        rules_path=overlay,
        preset_name="daily",
    )

    warnings = preset_context_warnings(
        preset,
        read_students("examples/students.csv"),
        history_count=0,
        rules=rules,
    )

    assert not any("preferred history data" in warning for warning in warnings)


def test_validate_preset_warns_without_history_and_strict_fails() -> None:
    output = cli.run_validate(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        preset_name="fair-rotation",
    )

    assert "Validation passed." in output
    assert 'Preset "fair-rotation" is missing preferred history data.' in output

    with pytest.raises(InputFileError, match="Warnings treated as errors"):
        cli.run_validate(
            students_path="examples/students.csv",
            layout_path="examples/classroom.json",
            preset_name="fair-rotation",
            strict=True,
        )


def test_solve_preset_with_rules_overlay_keeps_hard_constraints_for_all_candidates(
    tmp_path,
) -> None:
    overlay = tmp_path / "hard-rules.json"
    overlay.write_text(
        json.dumps(
            {
                "hard": {
                    "fixed_seats": [
                        {"student": "STU001", "seat_id": "R1C1"}
                    ],
                    "cannot_be_adjacent": [
                        {"students": ["STU004", "STU005"]}
                    ],
                }
            }
        ),
        encoding="utf-8",
    )

    output, summary = cli.solve_with_report(
        students_path="examples/students.csv",
        layout_path="examples/classroom.json",
        rules_path=overlay,
        preset_name="daily",
        history_dir="examples/history",
        candidate_count=3,
        output_path=tmp_path / "candidates.json",
    )
    candidate_set = load_candidate_set(output)

    assert candidate_set.metadata["preset"] == {
        "name": "daily",
        "user_rules_overlay": True,
    }
    assert not candidate_set.warnings
    assert "Generated 3 candidate seating plans." in (summary or "")
    for candidate in candidate_set.candidates:
        assignment = {
            item.student_key: item.seat_id
            for item in candidate.snapshot.assignments
        }
        assert assignment["STU001"] == "R1C1"
        assert candidate.hard_constraints_satisfied is True
        assert candidate.snapshot.metadata["preset"]["name"] == "daily"


def test_preset_cli_list_show_export_and_solve(tmp_path) -> None:
    runner = CliRunner()

    list_result = runner.invoke(cli.app, ["presets", "list"])
    show_result = runner.invoke(cli.app, ["presets", "show", "daily"])
    export_path = tmp_path / "daily.rules.json"
    export_result = runner.invoke(
        cli.app,
        ["presets", "export", "daily", "--output", str(export_path)],
    )
    solve_path = tmp_path / "daily.snapshot.json"
    solve_result = runner.invoke(
        cli.app,
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
            "--output",
            str(solve_path),
        ],
    )

    assert list_result.exit_code == 0
    assert "neighbor-aware" in list_result.output
    assert show_result.exit_code == 0
    assert "Rules JSON:" in show_result.output
    assert export_result.exit_code == 0
    assert load_rules(export_path).soft.fair_rotation.enabled is True
    assert solve_result.exit_code == 0, solve_result.output
    assert solve_path.exists()


def test_preset_commands_work_in_argparse_fallback(monkeypatch, capsys) -> None:
    monkeypatch.setattr(sys, "argv", ["seattrellis", "presets", "show", "random"])

    cli._run_argparse()

    output = capsys.readouterr().out
    assert "Preset: random" in output
    assert "Rules JSON:" in output
