from __future__ import annotations

from seattrellis import cli
from seattrellis.io.json_files import load_rotation_plan
from seattrellis.service import compute_rotation_plan
from seattrellis.service_types import RotationInput
from seattrellis.io.json_files import load_layout, load_rules
from seattrellis.io.students import read_students


def test_rotation_plan_generates_ordered_periods_and_repeat_summary(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    output, summary = cli.generate_rotation_plan(
        students_path=paths["students_csv"],
        layout_path=paths["layout"],
        rules_path=paths["rules"],
        history_dir=paths["history"],
        period_count=3,
        period_labels=["Monday", "Wednesday", "Friday"],
        seed=17,
        time_limit_seconds=0.1,
        backend="fallback",
        output_path=tmp_path / "rotation-plan.json",
    )

    plan = load_rotation_plan(output)
    assert [period.period for period in plan.periods] == [1, 2, 3]
    assert [period.label for period in plan.periods] == ["Monday", "Wednesday", "Friday"]
    assert plan.base_history_count == 3
    assert plan.pair_repeat_summary["history_count"] == 3
    assert "Generated 3 rotation periods." in summary


def test_rotation_plan_rejects_mismatched_labels(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    try:
        compute_rotation_plan(
            RotationInput(
                students=read_students(paths["students_csv"]),
                layout=load_layout(paths["layout"]),
                rules=load_rules(paths["rules"]),
                period_count=2,
                period_labels=["only one"],
            )
        )
    except ValueError as exc:
        assert "period_labels" in str(exc)
    else:  # pragma: no cover - defensive assertion
        raise AssertionError("mismatched period labels should fail")
