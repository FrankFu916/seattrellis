from __future__ import annotations

from dataclasses import dataclass, field
from typing import Iterable, Literal, Mapping, Sequence, TypeAlias

from seattrellis.models.candidate import HardConstraintSummary
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student
from seattrellis.scoring import evaluate_hard_constraints


EditingOperationKind = Literal[
    "swap_students",
    "move_student",
    "batch_move",
    "unseat_student",
    "seat_student",
    "lock_student",
    "unlock_student",
    "lock_seat",
    "unlock_seat",
]

LOCK_STATE_METADATA_KEY = "lock_state"

EditingPayloadValue: TypeAlias = (
    str
    | bool
    | None
    | list["EditingPayloadValue"]
    | dict[str, "EditingPayloadValue"]
)


class EditingError(ValueError):
    """Raised when a manual editing command would make the draft inconsistent."""


@dataclass(frozen=True)
class EditingOperation:
    """A UI-neutral command that changes a seating draft."""

    kind: EditingOperationKind
    payload: Mapping[str, EditingPayloadValue] = field(default_factory=dict)


@dataclass(frozen=True)
class EditingLockState:
    """Portable current locks shared by edit and constrained re-solve flows."""

    locked_students: tuple[str, ...] = ()
    locked_seats: tuple[str, ...] = ()

    @classmethod
    def from_values(
        cls,
        *,
        locked_students: Sequence[str] = (),
        locked_seats: Sequence[str] = (),
    ) -> "EditingLockState":
        return cls(
            locked_students=tuple(_normalized_identifiers(locked_students)),
            locked_seats=tuple(_normalized_identifiers(locked_seats)),
        )


@dataclass(frozen=True)
class EditingState:
    """A compact snapshot of the mutable editing state for undo and redo."""

    assignments: tuple[SeatAssignment, ...]
    locked_students: frozenset[str]
    locked_seats: frozenset[str]


@dataclass(frozen=True)
class EditingRecord:
    """An applied command together with enough state to reverse it safely."""

    operation: EditingOperation
    before: EditingState
    after: EditingState
    hard_summary: HardConstraintSummary


@dataclass
class EditingSession:
    """Mutable draft editor for manual seating adjustments.

    The session intentionally lives below any web or desktop UI. It allows
    temporary drafts with unseated students, but it never allows duplicate
    students, duplicate seats, unknown students, or disabled seats.
    """

    snapshot: SeatingSnapshot
    locked_students: set[str] = field(default_factory=set)
    locked_seats: set[str] = field(default_factory=set)
    undo_stack: list[EditingRecord] = field(default_factory=list)
    redo_stack: list[EditingRecord] = field(default_factory=list)

    def __post_init__(self) -> None:
        self.snapshot = _copy_snapshot(self.snapshot)
        self.locked_students = {
            str(item).strip()
            for item in self.locked_students
            if str(item).strip()
        }
        self.locked_seats = {
            str(item).strip()
            for item in self.locked_seats
            if str(item).strip()
        }
        self._validate_known_locks()
        self._validate_assignments(self.snapshot.assignments)

    @classmethod
    def from_snapshot(
        cls,
        snapshot: SeatingSnapshot,
        *,
        locked_students: set[str] | list[str] | tuple[str, ...] = (),
        locked_seats: set[str] | list[str] | tuple[str, ...] = (),
    ) -> EditingSession:
        return cls(
            snapshot=snapshot,
            locked_students=set(locked_students),
            locked_seats=set(locked_seats),
        )

    @property
    def operation_log(self) -> tuple[EditingRecord, ...]:
        """Commands that are currently applied to the draft."""

        return tuple(self.undo_stack)

    @property
    def lock_state(self) -> EditingLockState:
        """Return the current locks in a portable UI-neutral form."""

        return EditingLockState.from_values(
            locked_students=tuple(self.locked_students),
            locked_seats=tuple(self.locked_seats),
        )

    def current_snapshot(self) -> SeatingSnapshot:
        """Return a defensive copy of the current draft snapshot."""

        return _copy_snapshot(self.snapshot)

    def assignment_by_student(self) -> dict[str, SeatAssignment]:
        return {
            assignment.student_key: _copy_assignment(assignment)
            for assignment in self.snapshot.assignments
        }

    def assignment_by_seat(self) -> dict[str, SeatAssignment]:
        return {
            assignment.seat_id: _copy_assignment(assignment)
            for assignment in self.snapshot.assignments
        }

    def unseated_students(self) -> list[str]:
        assigned = {assignment.student_key for assignment in self.snapshot.assignments}
        return [student.key for student in self.snapshot.students if student.key not in assigned]

    def hard_constraint_summary(self) -> HardConstraintSummary:
        return evaluate_hard_constraints(
            self.snapshot.assignments,
            self.snapshot.students,
            self.snapshot.layout,
            self.snapshot.rules,
        )

    def apply(self, operation: EditingOperation) -> HardConstraintSummary:
        before = self._capture_state()
        assignments = list(self.snapshot.assignments)

        match operation.kind:
            case "swap_students":
                assignments = self._swap_students(
                    assignments,
                    _required_payload(operation, "first_student"),
                    _required_payload(operation, "second_student"),
                )
            case "move_student":
                assignments = self._move_student(
                    assignments,
                    _required_payload(operation, "student_key"),
                    _required_payload(operation, "seat_id"),
                )
            case "batch_move":
                assignments = self._batch_move(
                    assignments,
                    _required_batch_moves(operation),
                )
            case "seat_student":
                assignments = self._move_student(
                    assignments,
                    _required_payload(operation, "student_key"),
                    _required_payload(operation, "seat_id"),
                )
            case "unseat_student":
                assignments = self._unseat_student(
                    assignments,
                    _required_payload(operation, "student_key"),
                )
            case "lock_student":
                student_key = self._require_known_student(
                    _required_payload(operation, "student_key")
                )
                self.locked_students.add(student_key)
            case "unlock_student":
                student_key = self._require_known_student(
                    _required_payload(operation, "student_key")
                )
                self.locked_students.discard(student_key)
            case "lock_seat":
                seat_id = self._require_enabled_seat(_required_payload(operation, "seat_id"))
                self.locked_seats.add(seat_id)
            case "unlock_seat":
                seat_id = self._require_enabled_seat(_required_payload(operation, "seat_id"))
                self.locked_seats.discard(seat_id)
            case _:
                raise EditingError(f"Unsupported editing operation: {operation.kind}.")

        self._replace_assignments(assignments)
        after = self._capture_state()
        summary = self.hard_constraint_summary()
        if after != before:
            self.undo_stack.append(
                EditingRecord(
                    operation=operation,
                    before=before,
                    after=after,
                    hard_summary=summary,
                )
            )
            self.redo_stack.clear()
        return summary

    def swap_students(self, first_student: str, second_student: str) -> HardConstraintSummary:
        return self.apply(
            EditingOperation(
                kind="swap_students",
                payload={"first_student": first_student, "second_student": second_student},
            )
        )

    def move_student(self, student_key: str, seat_id: str) -> HardConstraintSummary:
        return self.apply(
            EditingOperation(
                kind="move_student",
                payload={"student_key": student_key, "seat_id": seat_id},
            )
        )

    def batch_move(
        self,
        moves: Mapping[str, str] | Sequence[tuple[str, str]],
    ) -> HardConstraintSummary:
        """Apply multiple destinations as one atomic, undoable command."""
        entries = moves.items() if isinstance(moves, Mapping) else moves
        return self.apply(
            EditingOperation(
                kind="batch_move",
                payload={
                    "moves": [
                        {"student_key": student_key, "seat_id": seat_id}
                        for student_key, seat_id in entries
                    ]
                },
            )
        )

    def seat_student(self, student_key: str, seat_id: str) -> HardConstraintSummary:
        return self.apply(
            EditingOperation(
                kind="seat_student",
                payload={"student_key": student_key, "seat_id": seat_id},
            )
        )

    def unseat_student(self, student_key: str) -> HardConstraintSummary:
        return self.apply(
            EditingOperation(kind="unseat_student", payload={"student_key": student_key})
        )

    def lock_student(self, student_key: str) -> HardConstraintSummary:
        return self.apply(
            EditingOperation(kind="lock_student", payload={"student_key": student_key})
        )

    def unlock_student(self, student_key: str) -> HardConstraintSummary:
        return self.apply(
            EditingOperation(kind="unlock_student", payload={"student_key": student_key})
        )

    def lock_seat(self, seat_id: str) -> HardConstraintSummary:
        return self.apply(EditingOperation(kind="lock_seat", payload={"seat_id": seat_id}))

    def unlock_seat(self, seat_id: str) -> HardConstraintSummary:
        return self.apply(EditingOperation(kind="unlock_seat", payload={"seat_id": seat_id}))

    def undo(self) -> HardConstraintSummary:
        if not self.undo_stack:
            raise EditingError("There is no editing operation to undo.")
        record = self.undo_stack.pop()
        self._restore_state(record.before)
        self.redo_stack.append(record)
        return self.hard_constraint_summary()

    def redo(self) -> HardConstraintSummary:
        if not self.redo_stack:
            raise EditingError("There is no editing operation to redo.")
        record = self.redo_stack.pop()
        self._restore_state(record.after)
        self.undo_stack.append(record)
        return self.hard_constraint_summary()

    def _swap_students(
        self,
        assignments: list[SeatAssignment],
        first_student: str,
        second_student: str,
    ) -> list[SeatAssignment]:
        first_student = self._require_known_student(first_student)
        second_student = self._require_known_student(second_student)
        if first_student == second_student:
            return assignments

        by_student = _assignment_by_student(assignments)
        first_assignment = by_student.get(first_student)
        second_assignment = by_student.get(second_student)
        if first_assignment is None or second_assignment is None:
            raise EditingError("Both students must be seated before they can be swapped.")

        self._ensure_student_can_move(first_student)
        self._ensure_student_can_move(second_student)
        self._ensure_seat_can_change(first_assignment.seat_id)
        self._ensure_seat_can_change(second_assignment.seat_id)

        by_student[first_student] = self._make_assignment(first_student, second_assignment.seat_id)
        by_student[second_student] = self._make_assignment(second_student, first_assignment.seat_id)
        return self._ordered_assignments(by_student.values())

    def _move_student(
        self,
        assignments: list[SeatAssignment],
        student_key: str,
        seat_id: str,
    ) -> list[SeatAssignment]:
        student_key = self._require_known_student(student_key)
        seat_id = self._require_enabled_seat(seat_id)
        by_student = _assignment_by_student(assignments)
        current = by_student.get(student_key)
        if current is not None and current.seat_id == seat_id:
            return assignments

        self._ensure_student_can_move(student_key)
        if current is not None:
            self._ensure_seat_can_change(current.seat_id)
        self._ensure_seat_can_change(seat_id)

        occupant = _assignment_by_seat(by_student.values()).get(seat_id)
        if occupant is not None and occupant.student_key != student_key:
            self._ensure_student_can_move(occupant.student_key)
            by_student.pop(occupant.student_key)

        by_student[student_key] = self._make_assignment(student_key, seat_id)
        return self._ordered_assignments(by_student.values())

    def _batch_move(
        self,
        assignments: list[SeatAssignment],
        moves: Sequence[tuple[str, str]],
    ) -> list[SeatAssignment]:
        normalized = [
            (
                self._require_known_student(student_key),
                self._require_enabled_seat(seat_id),
            )
            for student_key, seat_id in moves
        ]
        students = [student_key for student_key, _seat_id in normalized]
        seats = [seat_id for _student_key, seat_id in normalized]
        duplicate_students = _duplicates(students)
        duplicate_seats = _duplicates(seats)
        if duplicate_students:
            raise EditingError(
                "Batch move contains duplicate students: "
                + ", ".join(duplicate_students)
                + "."
            )
        if duplicate_seats:
            raise EditingError(
                "Batch move contains duplicate target seats: "
                + ", ".join(duplicate_seats)
                + "."
            )

        by_student = _assignment_by_student(assignments)
        by_seat = _assignment_by_seat(assignments)
        active_moves = [
            (student_key, seat_id)
            for student_key, seat_id in normalized
            if student_key not in by_student
            or by_student[student_key].seat_id != seat_id
        ]
        moving_students = {student_key for student_key, _seat_id in active_moves}
        for student_key, target_seat in active_moves:
            current = by_student.get(student_key)
            self._ensure_student_can_move(student_key)
            if current is not None:
                self._ensure_seat_can_change(current.seat_id)
            self._ensure_seat_can_change(target_seat)
            occupant = by_seat.get(target_seat)
            if (
                occupant is not None
                and occupant.student_key != student_key
                and occupant.student_key not in moving_students
            ):
                raise EditingError(
                    "Batch move target is occupied by a student outside the batch: "
                    f"{target_seat} ({occupant.student_key})."
                )

        for student_key in moving_students:
            by_student.pop(student_key, None)
        for student_key, seat_id in active_moves:
            by_student[student_key] = self._make_assignment(student_key, seat_id)
        return self._ordered_assignments(by_student.values())

    def _unseat_student(
        self,
        assignments: list[SeatAssignment],
        student_key: str,
    ) -> list[SeatAssignment]:
        student_key = self._require_known_student(student_key)
        by_student = _assignment_by_student(assignments)
        current = by_student.get(student_key)
        if current is None:
            return assignments

        self._ensure_student_can_move(student_key)
        self._ensure_seat_can_change(current.seat_id)
        by_student.pop(student_key)
        return self._ordered_assignments(by_student.values())

    def _capture_state(self) -> EditingState:
        return EditingState(
            assignments=tuple(
                _copy_assignment(assignment) for assignment in self.snapshot.assignments
            ),
            locked_students=frozenset(self.locked_students),
            locked_seats=frozenset(self.locked_seats),
        )

    def _restore_state(self, state: EditingState) -> None:
        self.locked_students = set(state.locked_students)
        self.locked_seats = set(state.locked_seats)
        self._replace_assignments(list(state.assignments))

    def _replace_assignments(self, assignments: list[SeatAssignment]) -> None:
        ordered = self._ordered_assignments(assignments)
        self._validate_assignments(ordered)
        self.snapshot = _copy_snapshot_with_assignments(self.snapshot, ordered)

    def _ordered_assignments(self, assignments: Iterable[SeatAssignment]) -> list[SeatAssignment]:
        student_order = {student.key: index for index, student in enumerate(self.snapshot.students)}
        copied = [_copy_assignment(assignment) for assignment in assignments]
        return sorted(
            copied,
            key=lambda assignment: (
                student_order.get(assignment.student_key, len(student_order)),
                assignment.student_key,
            ),
        )

    def _make_assignment(self, student_key: str, seat_id: str) -> SeatAssignment:
        student = self._student_by_key()[student_key]
        return SeatAssignment(
            student_key=student.key,
            student_name=student.display_name,
            seat_id=seat_id,
        )

    def _validate_known_locks(self) -> None:
        for student_key in self.locked_students:
            self._require_known_student(student_key)
        for seat_id in self.locked_seats:
            self._require_enabled_seat(seat_id)

    def _validate_assignments(self, assignments: list[SeatAssignment]) -> None:
        student_keys = [assignment.student_key for assignment in assignments]
        seat_ids = [assignment.seat_id for assignment in assignments]
        duplicate_students = sorted(
            {student_key for student_key in student_keys if student_keys.count(student_key) > 1}
        )
        duplicate_seats = sorted({seat_id for seat_id in seat_ids if seat_ids.count(seat_id) > 1})
        if duplicate_students:
            raise EditingError(f"Duplicate student assignments: {', '.join(duplicate_students)}.")
        if duplicate_seats:
            raise EditingError(f"Duplicate seat assignments: {', '.join(duplicate_seats)}.")

        known_students = set(self._student_by_key())
        unknown_students = sorted(set(student_keys) - known_students)
        if unknown_students:
            raise EditingError(
                f"Assignments reference unknown students: {', '.join(unknown_students)}."
            )

        enabled_seats = self._enabled_seat_ids()
        unknown_seats = sorted(set(seat_ids) - enabled_seats)
        if unknown_seats:
            raise EditingError(
                f"Assignments reference unknown or disabled seats: {', '.join(unknown_seats)}."
            )

    def _student_by_key(self) -> dict[str, Student]:
        keys = [student.key for student in self.snapshot.students]
        duplicates = sorted({student_key for student_key in keys if keys.count(student_key) > 1})
        if duplicates:
            raise EditingError(f"Student keys must be unique: {', '.join(duplicates)}.")
        return {student.key: student for student in self.snapshot.students}

    def _enabled_seat_ids(self) -> set[str]:
        return {seat.seat_id for seat in self.snapshot.layout.enabled_seats}

    def _require_known_student(self, student_key: str) -> str:
        student_key = str(student_key).strip()
        if not student_key:
            raise EditingError("Student key cannot be empty.")
        if student_key not in self._student_by_key():
            raise EditingError(f"Unknown student: {student_key}.")
        return student_key

    def _require_enabled_seat(self, seat_id: str) -> str:
        seat_id = str(seat_id).strip()
        if not seat_id:
            raise EditingError("Seat id cannot be empty.")
        if seat_id not in self._enabled_seat_ids():
            raise EditingError(f"Unknown or disabled seat: {seat_id}.")
        return seat_id

    def _ensure_student_can_move(self, student_key: str) -> None:
        if student_key in self.locked_students:
            raise EditingError(f"Student is locked and cannot be moved: {student_key}.")

    def _ensure_seat_can_change(self, seat_id: str) -> None:
        if seat_id in self.locked_seats:
            raise EditingError(f"Seat is locked and cannot be changed: {seat_id}.")


def lock_state_from_snapshot(snapshot: SeatingSnapshot) -> EditingLockState:
    """Return persisted locks from a snapshot, ignoring malformed metadata."""

    stored = snapshot.metadata.get(LOCK_STATE_METADATA_KEY)
    if not isinstance(stored, dict):
        return EditingLockState()
    return EditingLockState.from_values(
        locked_students=stored.get("locked_students", ()),
        locked_seats=stored.get("locked_seats", ()),
    )


def snapshot_with_lock_state(
    snapshot: SeatingSnapshot,
    lock_state: EditingLockState,
) -> SeatingSnapshot:
    """Persist portable locks without changing assignments or source rules."""

    metadata = dict(snapshot.metadata)
    metadata[LOCK_STATE_METADATA_KEY] = {
        "locked_students": list(lock_state.locked_students),
        "locked_seats": list(lock_state.locked_seats),
    }
    if hasattr(snapshot, "model_copy"):
        return snapshot.model_copy(  # type: ignore[attr-defined,return-value]
            update={"metadata": metadata}
        )
    return snapshot.copy(update={"metadata": metadata})


def _required_payload(operation: EditingOperation, key: str) -> str:
    value = operation.payload.get(key)
    text = "" if value is None else str(value).strip()
    if not text:
        raise EditingError(f"{operation.kind} requires payload field: {key}.")
    return text


def _required_batch_moves(operation: EditingOperation) -> list[tuple[str, str]]:
    value = operation.payload.get("moves")
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise EditingError("batch_move requires a moves list.")
    if not value:
        raise EditingError("batch_move requires at least one move.")
    moves: list[tuple[str, str]] = []
    for index, entry in enumerate(value, start=1):
        if not isinstance(entry, Mapping):
            raise EditingError(f"batch_move item {index} must be an object.")
        student_key = str(entry.get("student_key") or "").strip()
        seat_id = str(entry.get("seat_id") or "").strip()
        if not student_key or not seat_id:
            raise EditingError(
                f"batch_move item {index} requires student_key and seat_id."
            )
        moves.append((student_key, seat_id))
    return moves


def _normalized_identifiers(values: object) -> list[str]:
    if isinstance(values, str):
        candidates = [values]
    elif isinstance(values, Iterable):
        candidates = values
    else:
        return []
    normalized: list[str] = []
    for value in candidates:
        text = str(value).strip()
        if text and text not in normalized:
            normalized.append(text)
    return sorted(normalized)


def _duplicates(values: Sequence[str]) -> list[str]:
    return sorted({value for value in values if values.count(value) > 1})


def _assignment_by_student(assignments: Iterable[SeatAssignment]) -> dict[str, SeatAssignment]:
    return {
        assignment.student_key: _copy_assignment(assignment)
        for assignment in assignments
    }


def _assignment_by_seat(assignments: Iterable[SeatAssignment]) -> dict[str, SeatAssignment]:
    return {
        assignment.seat_id: _copy_assignment(assignment)
        for assignment in assignments
    }


def _copy_assignment(assignment: SeatAssignment) -> SeatAssignment:
    if hasattr(assignment, "model_copy"):
        return assignment.model_copy(deep=True)
    return assignment.copy(deep=True)


def _copy_snapshot(snapshot: SeatingSnapshot) -> SeatingSnapshot:
    if hasattr(snapshot, "model_copy"):
        return snapshot.model_copy(deep=True)
    return snapshot.copy(deep=True)


def _copy_snapshot_with_assignments(
    snapshot: SeatingSnapshot,
    assignments: list[SeatAssignment],
) -> SeatingSnapshot:
    copied_assignments = [_copy_assignment(assignment) for assignment in assignments]
    if hasattr(snapshot, "model_copy"):
        return snapshot.model_copy(update={"assignments": copied_assignments}, deep=True)
    return snapshot.copy(update={"assignments": copied_assignments}, deep=True)
