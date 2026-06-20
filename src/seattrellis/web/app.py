from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pandas as pd
from pydantic import ValidationError

try:
    import streamlit as st
except Exception as exc:  # pragma: no cover - depends on optional extra.
    raise RuntimeError("Install SeatTrellis with the web extra: pip install -e '.[web]'") from exc

from seattrellis.exporters import export_snapshot
from seattrellis.io.json_files import _parse_model, write_json_model
from seattrellis.io.students import students_from_dataframe
from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.rules import RuleSet
from seattrellis.solver import SeatTrellisSolveError, solve_seating


st.set_page_config(page_title="SeatTrellis", layout="wide")
st.title("SeatTrellis")

students_file = st.file_uploader("学生名单 CSV / Excel", type=["csv", "xlsx", "xls"])
layout_file = st.file_uploader("教室布局 JSON", type=["json"])
rules_file = st.file_uploader("规则 JSON", type=["json"])

if st.button("生成座位表", type="primary", disabled=not (students_file and layout_file and rules_file)):
    try:
        if students_file.name.lower().endswith(".csv"):
            students_frame = pd.read_csv(students_file, dtype=object)
        else:
            students_frame = pd.read_excel(students_file, dtype=object)
        students = students_from_dataframe(students_frame)
        layout = _parse_model(ClassroomLayout, json.loads(layout_file.getvalue().decode("utf-8")))
        rules = _parse_model(RuleSet, json.loads(rules_file.getvalue().decode("utf-8")))
        solution = solve_seating(students, layout, rules, seed=rules.seed)
        snapshot = solution.to_snapshot(students=students, layout=layout, rules=rules, seed=rules.seed)
    except (SeatTrellisSolveError, ValidationError, ValueError) as exc:
        st.error(str(exc))
    else:
        st.success(f"求解完成：{solution.solver_status}")
        st.dataframe([assignment.dict() for assignment in snapshot.assignments], use_container_width=True)
        with tempfile.TemporaryDirectory() as tmpdir:
            snapshot_path = Path(tmpdir) / "seattrellis.snapshot.json"
            write_json_model(snapshot, snapshot_path)
            st.download_button(
                "下载 snapshot JSON",
                data=snapshot_path.read_bytes(),
                file_name="seattrellis.snapshot.json",
                mime="application/json",
            )
            for output_format, mime in [
                ("excel", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                ("png", "image/png"),
                ("html", "text/html"),
            ]:
                output_path = export_snapshot(snapshot, output_format, Path(tmpdir) / f"seating.{_ext(output_format)}")
                st.download_button(
                    f"下载 {output_format.upper()}",
                    data=output_path.read_bytes(),
                    file_name=output_path.name,
                    mime=mime,
                )


def _ext(output_format: str) -> str:
    return "xlsx" if output_format == "excel" else output_format
