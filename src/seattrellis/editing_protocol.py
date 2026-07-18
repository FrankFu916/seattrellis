"""Versioned DTOs shared by browser, desktop, and Python editing adapters."""

from __future__ import annotations

from typing import Annotated, Literal, Union

try:
    from pydantic.v1 import (
        BaseModel,
        Field,
        StrictBool,
        conint,
        constr,
        root_validator,
        validator,
    )
except ImportError:  # pragma: no cover - pydantic v1.
    from pydantic import (
        BaseModel,
        Field,
        StrictBool,
        conint,
        constr,
        root_validator,
        validator,
    )

from seattrellis.editing import EditingOperation
from seattrellis.schema import EDITOR_PROTOCOL_VERSION


ProtocolIdentifier = constr(
    strict=True,
    strip_whitespace=True,
    min_length=1,
    max_length=128,
)
EntityReference = constr(strict=True, strip_whitespace=True, min_length=1)
NonNegativeInteger = conint(strict=True, ge=0)
PositiveInteger = conint(strict=True, ge=1)


class _StrictProtocolModel(BaseModel):
    class Config:
        extra = "forbid"


class SwapStudentsPayload(_StrictProtocolModel):
    first_student: EntityReference
    second_student: EntityReference


class StudentSeatPayload(_StrictProtocolModel):
    student_key: EntityReference
    seat_id: EntityReference


class StudentPayload(_StrictProtocolModel):
    student_key: EntityReference


class SeatPayload(_StrictProtocolModel):
    seat_id: EntityReference


class BatchMoveItem(_StrictProtocolModel):
    student_key: EntityReference
    seat_id: EntityReference


class BatchMovePayload(_StrictProtocolModel):
    moves: list[BatchMoveItem] = Field(min_items=1, max_items=100)

    @root_validator(skip_on_failure=True)
    def unique_students_and_seats(cls, values: dict[str, object]) -> dict[str, object]:
        value = values.get("moves")
        moves = value if isinstance(value, list) else []
        students = [
            move.student_key for move in moves if isinstance(move, BatchMoveItem)
        ]
        seats = [move.seat_id for move in moves if isinstance(move, BatchMoveItem)]
        if len(students) != len(set(students)):
            raise ValueError("batch_move students must be unique")
        if len(seats) != len(set(seats)):
            raise ValueError("batch_move target seats must be unique")
        return values


class SwapStudentsOperation(_StrictProtocolModel):
    kind: Literal["swap_students"]
    payload: SwapStudentsPayload


class MoveStudentOperation(_StrictProtocolModel):
    kind: Literal["move_student"]
    payload: StudentSeatPayload


class BatchMoveOperation(_StrictProtocolModel):
    kind: Literal["batch_move"]
    payload: BatchMovePayload


class SeatStudentOperation(_StrictProtocolModel):
    kind: Literal["seat_student"]
    payload: StudentSeatPayload


class UnseatStudentOperation(_StrictProtocolModel):
    kind: Literal["unseat_student"]
    payload: StudentPayload


class LockStudentOperation(_StrictProtocolModel):
    kind: Literal["lock_student"]
    payload: StudentPayload


class UnlockStudentOperation(_StrictProtocolModel):
    kind: Literal["unlock_student"]
    payload: StudentPayload


class LockSeatOperation(_StrictProtocolModel):
    kind: Literal["lock_seat"]
    payload: SeatPayload


class UnlockSeatOperation(_StrictProtocolModel):
    kind: Literal["unlock_seat"]
    payload: SeatPayload


EditorOperationDTO = Annotated[
    Union[
        SwapStudentsOperation,
        MoveStudentOperation,
        BatchMoveOperation,
        SeatStudentOperation,
        UnseatStudentOperation,
        LockStudentOperation,
        UnlockStudentOperation,
        LockSeatOperation,
        UnlockSeatOperation,
    ],
    Field(discriminator="kind"),
]

_OPERATION_MODELS = {
    "swap_students": SwapStudentsOperation,
    "move_student": MoveStudentOperation,
    "batch_move": BatchMoveOperation,
    "seat_student": SeatStudentOperation,
    "unseat_student": UnseatStudentOperation,
    "lock_student": LockStudentOperation,
    "unlock_student": UnlockStudentOperation,
    "lock_seat": LockSeatOperation,
    "unlock_seat": UnlockSeatOperation,
}


class EditorCommandEnvelope(_StrictProtocolModel):
    kind: Literal["seattrellis_editor_command"]
    protocol_version: Literal[EDITOR_PROTOCOL_VERSION]
    command_id: ProtocolIdentifier
    draft_id: ProtocolIdentifier
    base_revision: NonNegativeInteger
    action: Literal["apply", "undo", "redo"]
    operations: list[EditorOperationDTO] = Field(default_factory=list, max_items=100)

    class Config(_StrictProtocolModel.Config):
        schema_extra = {
            "allOf": [
                {
                    "if": {
                        "properties": {"action": {"const": "apply"}},
                        "required": ["action"],
                    },
                    "then": {
                        "properties": {"operations": {"minItems": 1}},
                        "required": ["operations"],
                    },
                },
                {
                    "if": {
                        "properties": {
                            "action": {"enum": ["undo", "redo"]},
                        },
                        "required": ["action"],
                    },
                    "then": {
                        "properties": {"operations": {"maxItems": 0}},
                    },
                },
            ]
        }

    @validator("operations", pre=True)
    def parse_operations_by_kind(cls, value: object) -> object:
        if not isinstance(value, (list, tuple)):
            return value
        parsed: list[_StrictProtocolModel] = []
        operation_types = tuple(_OPERATION_MODELS.values())
        for item in value:
            if isinstance(item, operation_types):
                parsed.append(item)
                continue
            if not isinstance(item, dict):
                raise TypeError("each editor operation must be an object")
            kind = item.get("kind")
            if not isinstance(kind, str) or kind not in _OPERATION_MODELS:
                allowed = ", ".join(sorted(_OPERATION_MODELS))
                raise ValueError(
                    f"unknown editor operation kind {kind!r}; expected one of: {allowed}"
                )
            parsed.append(_OPERATION_MODELS[kind].parse_obj(item))
        return parsed

    @root_validator(skip_on_failure=True)
    def action_matches_operations(cls, values: dict[str, object]) -> dict[str, object]:
        action = values.get("action")
        operations = values.get("operations") or []
        if action == "apply" and not operations:
            raise ValueError("apply commands require at least one operation")
        if action in {"undo", "redo"} and operations:
            raise ValueError(f"{action} commands must not contain operations")
        expanded_operation_count = sum(
            len(operation.payload.moves)
            if isinstance(operation, BatchMoveOperation)
            else 1
            for operation in operations
        )
        if expanded_operation_count > 100:
            raise ValueError(
                "editor commands may contain at most 100 expanded operations"
            )
        return values


class EditorStudentState(_StrictProtocolModel):
    student_key: EntityReference
    display_name: str
    seat_id: EntityReference | None = None
    locked: StrictBool = False


class EditorSeatState(_StrictProtocolModel):
    seat_id: EntityReference
    row: PositiveInteger
    col: PositiveInteger
    enabled: StrictBool
    student_key: EntityReference | None = None
    locked: StrictBool = False


class EditorHardConstraintState(_StrictProtocolModel):
    satisfied: StrictBool
    checked_rule_count: NonNegativeInteger
    violation_count: NonNegativeInteger
    violations: list[str] = Field(default_factory=list)


class EditorStateEnvelope(_StrictProtocolModel):
    kind: Literal["seattrellis_editor_state"]
    protocol_version: Literal[EDITOR_PROTOCOL_VERSION]
    draft_id: ProtocolIdentifier
    revision: NonNegativeInteger
    candidate_id: ProtocolIdentifier | None = None
    undo_depth: NonNegativeInteger
    redo_depth: NonNegativeInteger
    students: list[EditorStudentState]
    seats: list[EditorSeatState]
    hard_constraints: EditorHardConstraintState


class EditorProtocolConflictError(ValueError):
    """Raised when a command targets a stale or different editing draft."""


def operation_to_domain(operation: EditorOperationDTO) -> EditingOperation:
    """Convert a validated transport operation into the domain command."""
    if hasattr(operation.payload, "model_dump"):
        payload = operation.payload.model_dump(mode="json")  # type: ignore[attr-defined]
    else:
        payload = operation.payload.dict()
    return EditingOperation(kind=operation.kind, payload=payload)
