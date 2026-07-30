"""UI-neutral classroom layout editing commands.

The visual editor works with a permissive draft instead of mutating
``ClassroomLayout`` directly.  A draft may temporarily contain no usable
seats while a teacher reshapes the room; conversion back to the solver model
performs the strict validation at the workflow boundary.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal, Mapping, TypeAlias
from uuid import uuid4

from seattrellis.models.layout import AdjacencyConfig, ClassroomLayout, SeatNode


LayoutCellKind = Literal["seat", "aisle", "platform", "empty"]
LayoutCommandKind = Literal[
    "set_cell",
    "insert_row",
    "delete_row",
    "insert_column",
    "delete_column",
    "translate",
    "mirror_horizontal",
    "flip_vertical",
]
LayoutPayloadValue: TypeAlias = str | int | None

MAX_LAYOUT_ROWS = 50
MAX_LAYOUT_COLUMNS = 50
MAX_LAYOUT_CELLS = 1_000


class LayoutEditingError(ValueError):
    """Raised when a layout command cannot be applied safely."""


class LayoutRevisionConflictError(LayoutEditingError):
    """Raised when an editor command targets a stale draft revision."""


@dataclass(frozen=True, slots=True)
class LayoutCell:
    """One visible cell in the layout editor grid."""

    row: int
    column: int
    kind: LayoutCellKind
    seat_id: str | None = None

    def __post_init__(self) -> None:
        if self.row < 1 or self.column < 1:
            raise LayoutEditingError("Layout cell positions must be positive.")
        if self.kind not in {"seat", "aisle", "platform", "empty"}:
            raise LayoutEditingError(f"Unsupported layout cell kind: {self.kind!r}.")
        cleaned_id = self.seat_id.strip() if isinstance(self.seat_id, str) else None
        if self.kind == "seat" and not cleaned_id:
            raise LayoutEditingError("Seat cells require a seat_id.")
        if self.kind != "seat" and cleaned_id is not None:
            raise LayoutEditingError("Only seat cells may have a seat_id.")
        object.__setattr__(self, "seat_id", cleaned_id)


@dataclass(frozen=True, slots=True)
class LayoutCommand:
    """A serializable request to change a layout draft."""

    kind: LayoutCommandKind
    payload: Mapping[str, LayoutPayloadValue] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class _LayoutState:
    rows: int
    columns: int
    cells: tuple[LayoutCell, ...]


@dataclass
class LayoutDraft:
    """Mutable classroom grid with atomic commands and undo/redo history."""

    rows: int
    columns: int
    cells: dict[tuple[int, int], LayoutCell] = field(default_factory=dict)
    name: str = "Classroom"
    draft_id: str = field(default_factory=lambda: uuid4().hex)
    revision: int = 0
    undo_stack: list[_LayoutState] = field(default_factory=list, repr=False)
    redo_stack: list[_LayoutState] = field(default_factory=list, repr=False)

    def __post_init__(self) -> None:
        self.name = self.name.strip()
        self.draft_id = self.draft_id.strip()
        if not self.name:
            raise LayoutEditingError("Layout name cannot be empty.")
        if not self.draft_id:
            raise LayoutEditingError("Layout draft_id cannot be empty.")
        self._validate_dimensions(self.rows, self.columns)
        normalized: dict[tuple[int, int], LayoutCell] = {}
        for cell in self.cells.values():
            self._validate_cell_position(cell)
            normalized[(cell.row, cell.column)] = cell
        self.cells = normalized
        self._validate_unique_seat_ids()

    @classmethod
    def rectangular(
        cls,
        rows: int,
        columns: int,
        *,
        name: str = "Classroom",
    ) -> "LayoutDraft":
        """Create a draft filled with usable seats."""

        return cls(
            rows=rows,
            columns=columns,
            name=name,
            cells={
                (row, column): LayoutCell(
                    row=row,
                    column=column,
                    kind="seat",
                    seat_id=f"R{row}C{column}",
                )
                for row in range(1, rows + 1)
                for column in range(1, columns + 1)
            },
        )

    @classmethod
    def from_layout(cls, layout: ClassroomLayout) -> "LayoutDraft":
        """Create an editable draft from the strict solver layout."""

        rows = max(seat.row for seat in layout.seats)
        columns = max(seat.col for seat in layout.seats)
        cells: dict[tuple[int, int], LayoutCell] = {}
        for seat in layout.seats:
            cell_type = str(seat.attributes.get("cell_type", "")).strip().lower()
            if seat.enabled:
                kind: LayoutCellKind = "seat"
                seat_id = seat.seat_id
            elif cell_type == "platform" or seat.zone == "platform":
                kind = "platform"
                seat_id = None
            elif cell_type == "aisle" or seat.zone == "aisle":
                kind = "aisle"
                seat_id = None
            else:
                kind = "empty"
                seat_id = None
            cells[(seat.row, seat.col)] = LayoutCell(
                row=seat.row,
                column=seat.col,
                kind=kind,
                seat_id=seat_id,
            )
        return cls(
            rows=rows,
            columns=columns,
            cells=cells,
            name=layout.name,
            draft_id=layout.layout_id,
        )

    def ordered_cells(self, *, include_empty: bool = True) -> tuple[LayoutCell, ...]:
        """Return a stable row-major view for transport and rendering."""

        result: list[LayoutCell] = []
        for row in range(1, self.rows + 1):
            for column in range(1, self.columns + 1):
                cell = self.cells.get((row, column))
                if cell is None:
                    cell = LayoutCell(row=row, column=column, kind="empty")
                if include_empty or cell.kind != "empty":
                    result.append(cell)
        return tuple(result)

    def apply(
        self,
        command: LayoutCommand,
        *,
        base_revision: int | None = None,
    ) -> int:
        """Apply one command atomically and return the new revision."""

        if base_revision is not None and base_revision != self.revision:
            raise LayoutRevisionConflictError(
                "The layout command is stale: "
                f"base revision {base_revision}, current revision {self.revision}."
            )
        before = self._capture_state()
        try:
            self._dispatch(command)
            self._validate_dimensions(self.rows, self.columns)
            self._validate_unique_seat_ids()
        except Exception:
            self._restore_state(before)
            raise
        after = self._capture_state()
        if after != before:
            self.undo_stack.append(before)
            self.redo_stack.clear()
            self.revision += 1
        return self.revision

    def undo(self, *, base_revision: int | None = None) -> int:
        if base_revision is not None and base_revision != self.revision:
            raise LayoutRevisionConflictError(
                "The layout undo request targets a stale revision."
            )
        if not self.undo_stack:
            raise LayoutEditingError("There is no layout change to undo.")
        current = self._capture_state()
        previous = self.undo_stack.pop()
        self._restore_state(previous)
        self.redo_stack.append(current)
        self.revision += 1
        return self.revision

    def redo(self, *, base_revision: int | None = None) -> int:
        if base_revision is not None and base_revision != self.revision:
            raise LayoutRevisionConflictError(
                "The layout redo request targets a stale revision."
            )
        if not self.redo_stack:
            raise LayoutEditingError("There is no layout change to redo.")
        current = self._capture_state()
        following = self.redo_stack.pop()
        self._restore_state(following)
        self.undo_stack.append(current)
        self.revision += 1
        return self.revision

    def to_layout(self, *, layout_id: str | None = None) -> ClassroomLayout:
        """Validate and compile the draft for solving or project storage."""

        nodes: list[SeatNode] = []
        for cell in self.ordered_cells(include_empty=False):
            if cell.kind == "seat":
                nodes.append(
                    SeatNode(
                        seat_id=cell.seat_id or "",
                        row=cell.row,
                        col=cell.column,
                        enabled=True,
                        near_platform=self._has_platform_in_front(cell),
                        attributes={"cell_type": "seat"},
                    )
                )
            elif cell.kind in {"aisle", "platform"}:
                nodes.append(
                    SeatNode(
                        seat_id=f"{cell.kind.upper()}-R{cell.row}C{cell.column}",
                        row=cell.row,
                        col=cell.column,
                        enabled=False,
                        zone=cell.kind,
                        tags=[cell.kind],
                        attributes={"cell_type": cell.kind},
                    )
                )
        if not any(node.enabled for node in nodes):
            raise LayoutEditingError(
                "The classroom needs at least one seat before it can be used."
            )
        return ClassroomLayout(
            layout_id=(layout_id or self.draft_id).strip(),
            name=self.name,
            seats=nodes,
            adjacency=AdjacencyConfig(
                include_horizontal=True,
                include_vertical=False,
                include_diagonal=False,
            ),
            metadata={
                "room_type": "visual-editor",
                "rows": self.rows,
                "columns": self.columns,
                "front": "row-1",
            },
        )

    def _dispatch(self, command: LayoutCommand) -> None:
        match command.kind:
            case "set_cell":
                self._set_cell(command.payload)
            case "insert_row":
                self._insert_row(_required_int(command.payload, "index"))
            case "delete_row":
                self._delete_row(_required_int(command.payload, "index"))
            case "insert_column":
                self._insert_column(_required_int(command.payload, "index"))
            case "delete_column":
                self._delete_column(_required_int(command.payload, "index"))
            case "translate":
                self._translate(
                    _required_int(command.payload, "row_delta"),
                    _required_int(command.payload, "column_delta"),
                )
            case "mirror_horizontal":
                self.cells = {
                    (cell.row, self.columns + 1 - cell.column): LayoutCell(
                        row=cell.row,
                        column=self.columns + 1 - cell.column,
                        kind=cell.kind,
                        seat_id=cell.seat_id,
                    )
                    for cell in self.cells.values()
                }
            case "flip_vertical":
                self.cells = {
                    (self.rows + 1 - cell.row, cell.column): LayoutCell(
                        row=self.rows + 1 - cell.row,
                        column=cell.column,
                        kind=cell.kind,
                        seat_id=cell.seat_id,
                    )
                    for cell in self.cells.values()
                }
            case _:
                raise LayoutEditingError(
                    f"Unsupported layout command: {command.kind!r}."
                )

    def _set_cell(self, payload: Mapping[str, LayoutPayloadValue]) -> None:
        row = _required_int(payload, "row")
        column = _required_int(payload, "column")
        self._require_position(row, column)
        raw_kind = payload.get("kind")
        if not isinstance(raw_kind, str) or raw_kind not in {
            "seat",
            "aisle",
            "platform",
            "empty",
        }:
            raise LayoutEditingError(
                "Cell kind must be seat, aisle, platform, or empty."
            )
        kind: LayoutCellKind = raw_kind
        if kind == "seat":
            raw_seat_id = payload.get("seat_id")
            seat_id = (
                raw_seat_id.strip()
                if isinstance(raw_seat_id, str) and raw_seat_id.strip()
                else self._next_seat_id(row, column)
            )
        else:
            seat_id = None
        self.cells[(row, column)] = LayoutCell(
            row=row,
            column=column,
            kind=kind,
            seat_id=seat_id,
        )

    def _insert_row(self, index: int) -> None:
        if not 1 <= index <= self.rows + 1:
            raise LayoutEditingError("Inserted row index is outside the layout.")
        self._validate_dimensions(self.rows + 1, self.columns)
        self.cells = {
            (cell.row + (cell.row >= index), cell.column): LayoutCell(
                row=cell.row + (cell.row >= index),
                column=cell.column,
                kind=cell.kind,
                seat_id=cell.seat_id,
            )
            for cell in self.cells.values()
        }
        self.rows += 1

    def _delete_row(self, index: int) -> None:
        if self.rows == 1:
            raise LayoutEditingError("A layout must keep at least one row.")
        if not 1 <= index <= self.rows:
            raise LayoutEditingError("Deleted row index is outside the layout.")
        self.cells = {
            (cell.row - (cell.row > index), cell.column): LayoutCell(
                row=cell.row - (cell.row > index),
                column=cell.column,
                kind=cell.kind,
                seat_id=cell.seat_id,
            )
            for cell in self.cells.values()
            if cell.row != index
        }
        self.rows -= 1

    def _insert_column(self, index: int) -> None:
        if not 1 <= index <= self.columns + 1:
            raise LayoutEditingError("Inserted column index is outside the layout.")
        self._validate_dimensions(self.rows, self.columns + 1)
        self.cells = {
            (cell.row, cell.column + (cell.column >= index)): LayoutCell(
                row=cell.row,
                column=cell.column + (cell.column >= index),
                kind=cell.kind,
                seat_id=cell.seat_id,
            )
            for cell in self.cells.values()
        }
        self.columns += 1

    def _delete_column(self, index: int) -> None:
        if self.columns == 1:
            raise LayoutEditingError("A layout must keep at least one column.")
        if not 1 <= index <= self.columns:
            raise LayoutEditingError("Deleted column index is outside the layout.")
        self.cells = {
            (cell.row, cell.column - (cell.column > index)): LayoutCell(
                row=cell.row,
                column=cell.column - (cell.column > index),
                kind=cell.kind,
                seat_id=cell.seat_id,
            )
            for cell in self.cells.values()
            if cell.column != index
        }
        self.columns -= 1

    def _translate(self, row_delta: int, column_delta: int) -> None:
        moved: dict[tuple[int, int], LayoutCell] = {}
        for cell in self.cells.values():
            # Empty cells are the canvas background. Moving them would make a
            # useful one-cell shift impossible whenever the draft has an empty
            # border, so only physical classroom cells participate.
            if cell.kind == "empty":
                continue
            row = cell.row + row_delta
            column = cell.column + column_delta
            self._require_position(row, column)
            moved[(row, column)] = LayoutCell(
                row=row,
                column=column,
                kind=cell.kind,
                seat_id=cell.seat_id,
            )
        self.cells = moved

    def _next_seat_id(self, row: int, column: int) -> str:
        existing = {cell.seat_id for cell in self.cells.values() if cell.seat_id}
        preferred = f"R{row}C{column}"
        if preferred not in existing:
            return preferred
        suffix = 2
        while f"{preferred}-{suffix}" in existing:
            suffix += 1
        return f"{preferred}-{suffix}"

    def _has_platform_in_front(self, cell: LayoutCell) -> bool:
        return any(
            other.kind == "platform" and other.row < cell.row
            for other in self.cells.values()
        )

    def _capture_state(self) -> _LayoutState:
        return _LayoutState(self.rows, self.columns, self.ordered_cells())

    def _restore_state(self, state: _LayoutState) -> None:
        self.rows = state.rows
        self.columns = state.columns
        self.cells = {
            (cell.row, cell.column): cell
            for cell in state.cells
            if cell.kind != "empty"
        }

    def _validate_dimensions(self, rows: int, columns: int) -> None:
        if not 1 <= rows <= MAX_LAYOUT_ROWS:
            raise LayoutEditingError(
                f"Layout rows must be between 1 and {MAX_LAYOUT_ROWS}."
            )
        if not 1 <= columns <= MAX_LAYOUT_COLUMNS:
            raise LayoutEditingError(
                f"Layout columns must be between 1 and {MAX_LAYOUT_COLUMNS}."
            )
        if rows * columns > MAX_LAYOUT_CELLS:
            raise LayoutEditingError(
                f"Layout grids may contain at most {MAX_LAYOUT_CELLS} cells."
            )

    def _validate_cell_position(self, cell: LayoutCell) -> None:
        self._require_position(cell.row, cell.column)

    def _require_position(self, row: int, column: int) -> None:
        if not 1 <= row <= self.rows or not 1 <= column <= self.columns:
            raise LayoutEditingError(
                f"Cell position row {row}, column {column} is outside the layout."
            )

    def _validate_unique_seat_ids(self) -> None:
        seat_ids = [
            cell.seat_id
            for cell in self.cells.values()
            if cell.kind == "seat" and cell.seat_id is not None
        ]
        duplicates = sorted(
            seat_id for seat_id in set(seat_ids) if seat_ids.count(seat_id) > 1
        )
        if duplicates:
            raise LayoutEditingError(
                "Seat IDs must be unique: " + ", ".join(duplicates)
            )


def _required_int(
    payload: Mapping[str, LayoutPayloadValue],
    key: str,
) -> int:
    value = payload.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        raise LayoutEditingError(f"Layout command field {key!r} must be an integer.")
    return value
