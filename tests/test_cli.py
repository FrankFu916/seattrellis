from __future__ import annotations

from seattrellis import cli
from seattrellis.io.json_files import load_snapshot


def test_cli_helpers_init_solve_and_export(tmp_path) -> None:
    paths = cli.init_demo(output_dir=tmp_path, overwrite=True)
    snapshot_path = cli.solve(
        students_path=paths["students_csv"],
        layout_path=paths["layout"],
        rules_path=paths["rules"],
        output_path=tmp_path / "outputs" / "latest.snapshot.json",
    )
    html_path = cli.export(snapshot_path=snapshot_path, output_format="html", output_path=tmp_path / "outputs" / "seating.html")

    snapshot = load_snapshot(snapshot_path)
    assert len(snapshot.assignments) == 8
    assert html_path.exists()
