from __future__ import annotations

import sys
from types import SimpleNamespace

import pytest

from seattrellis.io.json_files import InputFileError
from seattrellis.io.roster_table import (
    RosterTableLimits,
    read_roster_table,
    read_roster_table_bytes,
)


def test_csv_table_preserves_duplicate_headers_positions_and_raw_cells(tmp_path) -> None:
    path = tmp_path / "roster.csv"
    path.write_bytes(
        "\ufeff姓名,姓名, 成绩 ,备注\r\n Alice ,备用名,091,  keep me  \r\nBob,,80\r\n".encode(
            "utf-8"
        )
    )

    table = read_roster_table(path)

    assert table.source_format == "csv"
    assert table.sheet_name is None
    assert table.headers == ("姓名", "姓名", "成绩", "备注")
    assert [column.index for column in table.columns] == [0, 1, 2, 3]
    assert table.columns[2].raw_header == " 成绩 "
    assert table.rows[0].row_number == 2
    assert table.rows[0].cells == (" Alice ", "备用名", "091", "  keep me  ")
    assert table.rows[1].cells == ("Bob", "", "80")
    assert table.rows[1].cell(3) is None


def test_csv_upload_bytes_uses_same_lossless_reader() -> None:
    table = read_roster_table_bytes(
        "学号,姓名\n001,小林\n".encode(),
        filename="class.csv",
    )

    assert table.headers == ("学号", "姓名")
    assert table.rows[0].cells == ("001", "小林")


@pytest.mark.parametrize(
    ("content", "limits", "message"),
    [
        (b"name\nAlice\n", RosterTableLimits(max_file_bytes=5), "limit is 5"),
        (
            b"name\nAlice\nBob\n",
            RosterTableLimits(max_rows=1),
            "more than the allowed 1 data rows",
        ),
        (
            b"a,b,c\n1,2,3\n",
            RosterTableLimits(max_columns=2),
            "3 columns; the limit is 2",
        ),
    ],
)
def test_csv_reader_enforces_resource_limits(
    content: bytes,
    limits: RosterTableLimits,
    message: str,
) -> None:
    with pytest.raises(InputFileError, match=message):
        read_roster_table_bytes(content, filename="roster.csv", limits=limits)


def test_csv_reader_rejects_rows_wider_than_header() -> None:
    with pytest.raises(InputFileError, match="row 2 has 2 cells"):
        read_roster_table_bytes(b"name\nAlice,extra\n", filename="roster.csv")


def test_xlsx_reader_preserves_cell_types_and_duplicate_headers(tmp_path) -> None:
    openpyxl = pytest.importorskip("openpyxl")
    path = tmp_path / "roster.xlsx"
    workbook = openpyxl.Workbook()
    sheet = workbook.active
    sheet.title = "Class A"
    sheet.append(["姓名", "成绩", "成绩"])
    sheet.append(["Alice", 91, 92])
    workbook.save(path)
    workbook.close()

    table = read_roster_table(path)

    assert table.source_format == "xlsx"
    assert table.sheet_name == "Class A"
    assert table.headers == ("姓名", "成绩", "成绩")
    assert table.rows[0].cells == ("Alice", 91, 92)


def test_xlsx_workbook_is_closed_when_a_limit_fails(monkeypatch) -> None:
    class FakeSheet:
        max_column = 1
        max_row = 5
        title = "Sheet"

    class FakeWorkbook:
        active = FakeSheet()

        def __init__(self) -> None:
            self.closed = False

        def close(self) -> None:
            self.closed = True

    workbook = FakeWorkbook()
    fake_openpyxl = SimpleNamespace(load_workbook=lambda *args, **kwargs: workbook)
    monkeypatch.setitem(sys.modules, "openpyxl", fake_openpyxl)

    with pytest.raises(InputFileError, match="more than the allowed 1 data rows"):
        read_roster_table_bytes(
            b"not-used-by-fake-loader",
            filename="roster.xlsx",
            limits=RosterTableLimits(max_rows=1),
        )

    assert workbook.closed is True


def test_unsupported_and_empty_inputs_fail_clearly() -> None:
    with pytest.raises(InputFileError, match="Unsupported roster file format"):
        read_roster_table_bytes(b"data", filename="roster.txt")
    with pytest.raises(InputFileError, match="header row is required"):
        read_roster_table_bytes(b"", filename="roster.csv")

