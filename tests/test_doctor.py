from __future__ import annotations

from seattrellis import service
from seattrellis.demo import create_demo_files


def test_doctor_checks_demo_files_in_the_current_workspace(
    tmp_path,
    monkeypatch,
) -> None:
    create_demo_files(tmp_path, overwrite=True)
    monkeypatch.chdir(tmp_path)
    checked_packages: list[str] = []

    def installed_version(package_name: str) -> str:
        checked_packages.append(package_name)
        return "1.0"

    monkeypatch.setattr(service, "version", installed_version)

    report = service.run_doctor()

    assert f"Examples:     {tmp_path / 'examples'}" in report
    for filename in (
        "students.csv",
        "classroom.json",
        "rules.json",
        "project.seattrellis.json",
    ):
        assert f"✅ {filename}" in report
    assert {
        "ortools",
        "openpyxl",
        "Pillow",
        "streamlit",
        "weasyprint",
        "python-docx",
        "seattrellis-native",
    } <= set(checked_packages)
    assert (
        "Native extension: installed "
        "(1.0; compatibility is checked only when selected)"
    ) in report
