"""Conflict-aware roster update previews with optimistic revision checks."""

from __future__ import annotations

import hashlib
import json
import unicodedata
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from typing import Any, Literal

from seattrellis.application.roster_mapping import ROSTER_FIELDS, RosterField
from seattrellis.models.student import Student


RosterUpdateMode = Literal["incremental", "replace"]
RosterChangeAction = Literal["add", "update", "unchanged", "remove", "conflict"]
RosterMatchMethod = Literal["student_id", "name", "new"]

_STUDENT_FIELDS = (*ROSTER_FIELDS, "attributes")


class RosterUpdateError(ValueError):
    """Base error for roster preview and apply operations."""


class RosterUpdateConflictError(RosterUpdateError):
    """Raised when an update with unresolved identity conflicts is applied."""


class StaleRosterRevisionError(RosterUpdateError):
    """Raised when an update preview no longer targets the current roster."""


@dataclass(frozen=True)
class RosterState:
    """An immutable roster revision used by browser and desktop adapters."""

    students: tuple[Student, ...]
    revision: int = 0

    def __post_init__(self) -> None:
        if (
            isinstance(self.revision, bool)
            or not isinstance(self.revision, int)
            or self.revision < 0
        ):
            raise ValueError("revision must be a non-negative integer")
        object.__setattr__(self, "students", tuple(self.students))
        if not all(isinstance(student, Student) for student in self.students):
            raise TypeError("students must contain Student objects")


@dataclass(frozen=True)
class RosterFieldChange:
    """One visible field difference for an existing student."""

    field: str
    before: Any
    after: Any


@dataclass(frozen=True)
class RosterConflict:
    """An identity ambiguity that must be resolved before applying an import."""

    code: str
    message: str
    incoming_index: int | None = None
    existing_indices: tuple[int, ...] = ()


@dataclass(frozen=True)
class RosterChange:
    """One row in an import difference preview."""

    action: RosterChangeAction
    before: Student | None
    after: Student | None
    match_method: RosterMatchMethod
    field_changes: tuple[RosterFieldChange, ...] = ()
    incoming_index: int | None = None
    existing_index: int | None = None


@dataclass(frozen=True)
class RosterUpdatePreview:
    """A complete, immutable update plan that can be applied once."""

    mode: RosterUpdateMode
    base_revision: int
    base_fingerprint: str
    changes: tuple[RosterChange, ...]
    conflicts: tuple[RosterConflict, ...]
    resulting_students: tuple[Student, ...] | None
    updated_fields: tuple[str, ...]

    @property
    def can_apply(self) -> bool:
        return not self.conflicts and self.resulting_students is not None

    def count(self, action: RosterChangeAction) -> int:
        return sum(change.action == action for change in self.changes)


def normalize_student_name(value: str | None) -> str | None:
    """Normalize for exact matching only; no fuzzy or phonetic matching is used."""

    if value is None:
        return None
    text = unicodedata.normalize("NFKC", value).strip().casefold()
    if not text:
        return None
    # Whitespace differences from copied spreadsheets are not meaningful, but
    # every remaining character must match exactly.
    return " ".join(text.split())


def roster_fingerprint(students: Sequence[Student]) -> str:
    """Return an order-sensitive digest used to detect stale same-revision data."""

    payload = [_student_data(student) for student in students]
    serialized = json.dumps(
        payload,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        default=_json_default,
    ).encode("utf-8")
    return hashlib.sha256(serialized).hexdigest()


def preview_roster_update(
    current: RosterState | Sequence[Student],
    incoming: Iterable[Student],
    *,
    mode: RosterUpdateMode | Literal["full"] = "incremental",
    base_revision: int | None = None,
    updated_fields: Iterable[RosterField | Literal["attributes"]] | None = None,
) -> RosterUpdatePreview:
    """Build an incremental or full-replacement difference preview.

    Identity resolution follows a strict order:

    1. exact ``student_id``;
    2. a unique exact normalized name when no ID match exists;
    3. otherwise a new student or an explicit conflict.

    An incoming ID is never silently used to replace a different existing ID,
    even when the names match. This turns a likely ID typo into a reviewable
    conflict instead of corrupting the stable identity used by seat history.
    """

    state = _coerce_state(current, revision=base_revision)
    imported = tuple(incoming)
    if not all(isinstance(student, Student) for student in imported):
        raise TypeError("incoming must contain Student objects")
    resolved_mode = _normalize_mode(mode)
    fields = _normalize_updated_fields(updated_fields)

    conflicts: list[RosterConflict] = []
    changes: list[RosterChange] = []
    existing_ids = _index_values(
        state.students,
        lambda student: student.student_id,
    )
    existing_names = _index_values(
        state.students,
        lambda student: normalize_student_name(student.name),
    )
    incoming_ids = _index_values(imported, lambda student: student.student_id)
    incoming_name_only = _index_values(
        imported,
        lambda student: (
            normalize_student_name(student.name)
            if student.student_id is None
            else None
        ),
    )

    duplicate_existing_ids = {
        value: indices for value, indices in existing_ids.items() if len(indices) > 1
    }
    for student_id, indices in sorted(duplicate_existing_ids.items()):
        conflicts.append(
            RosterConflict(
                code="duplicate_existing_student_id",
                message=(
                    f"The current roster contains student_id {student_id!r} more "
                    "than once. Resolve it before importing."
                ),
                existing_indices=indices,
            )
        )

    blocked_incoming: set[int] = set()
    for student_id, indices in sorted(incoming_ids.items()):
        if len(indices) <= 1:
            continue
        blocked_incoming.update(indices)
        conflicts.append(
            RosterConflict(
                code="duplicate_incoming_student_id",
                message=f"The import contains student_id {student_id!r} more than once.",
                incoming_index=indices[0],
            )
        )
    for name, indices in sorted(incoming_name_only.items()):
        if len(indices) <= 1:
            continue
        blocked_incoming.update(indices)
        conflicts.append(
            RosterConflict(
                code="duplicate_incoming_name",
                message=(
                    f"The import contains the name {name!r} more than once without "
                    "student IDs."
                ),
                incoming_index=indices[0],
            )
        )

    matched_existing: set[int] = set()
    replacements: dict[int, Student] = {}
    additions: list[Student] = []

    for incoming_index, student in enumerate(imported):
        if incoming_index in blocked_incoming:
            changes.append(
                RosterChange(
                    action="conflict",
                    before=None,
                    after=student,
                    match_method="new",
                    incoming_index=incoming_index,
                )
            )
            continue

        match, method, conflict = _match_student(
            student,
            existing=state.students,
            existing_ids=existing_ids,
            existing_names=existing_names,
            incoming_index=incoming_index,
        )
        if conflict is not None:
            conflicts.append(conflict)
            changes.append(
                RosterChange(
                    action="conflict",
                    before=None,
                    after=student,
                    match_method=method,
                    incoming_index=incoming_index,
                )
            )
            continue
        if match is None:
            additions.append(student)
            changes.append(
                RosterChange(
                    action="add",
                    before=None,
                    after=student,
                    match_method="new",
                    incoming_index=incoming_index,
                )
            )
            continue
        if match in matched_existing:
            conflict = RosterConflict(
                code="existing_student_matched_twice",
                message=(
                    "Two imported rows resolve to the same current student. "
                    "Add or correct student IDs before applying."
                ),
                incoming_index=incoming_index,
                existing_indices=(match,),
            )
            conflicts.append(conflict)
            changes.append(
                RosterChange(
                    action="conflict",
                    before=state.students[match],
                    after=student,
                    match_method=method,
                    incoming_index=incoming_index,
                    existing_index=match,
                )
            )
            continue

        matched_existing.add(match)
        previous = state.students[match]
        replacement = (
            student
            if resolved_mode == "replace"
            else _merge_student(previous, student, fields, method=method)
        )
        replacements[match] = replacement
        field_changes = _field_changes(previous, replacement)
        changes.append(
            RosterChange(
                action="update" if field_changes else "unchanged",
                before=previous,
                after=replacement,
                match_method=method,
                field_changes=field_changes,
                incoming_index=incoming_index,
                existing_index=match,
            )
        )

    if resolved_mode == "replace":
        for existing_index, student in enumerate(state.students):
            if existing_index not in matched_existing:
                changes.append(
                    RosterChange(
                        action="remove",
                        before=student,
                        after=None,
                        match_method="new",
                        existing_index=existing_index,
                    )
                )

    if conflicts:
        result: tuple[Student, ...] | None = None
    elif resolved_mode == "replace":
        # Full replacement follows the imported row order exactly.
        result = imported
    else:
        merged = [
            replacements.get(index, student)
            for index, student in enumerate(state.students)
        ]
        merged.extend(additions)
        result = tuple(merged)

    if result is not None:
        duplicate_keys = _duplicate_student_keys(result)
        if duplicate_keys:
            conflicts.append(
                RosterConflict(
                    code="duplicate_resulting_identifier",
                    message=(
                        "The imported roster would create duplicate student "
                        "identifiers: " + ", ".join(duplicate_keys)
                    ),
                )
            )
            result = None

    return RosterUpdatePreview(
        mode=resolved_mode,
        base_revision=state.revision,
        base_fingerprint=roster_fingerprint(state.students),
        changes=tuple(changes),
        conflicts=tuple(conflicts),
        resulting_students=result,
        updated_fields=fields,
    )


def apply_roster_update(
    current: RosterState | Sequence[Student],
    preview: RosterUpdatePreview,
    *,
    current_revision: int | None = None,
) -> RosterState:
    """Apply a valid preview once, rejecting conflicts and stale base data."""

    if isinstance(current, RosterState):
        if current_revision is not None and current_revision != current.revision:
            raise StaleRosterRevisionError(
                "current_revision does not match the supplied RosterState"
            )
        state = current
    else:
        if current_revision is None:
            raise TypeError(
                "current_revision is required when current is not a RosterState"
            )
        state = RosterState(students=tuple(current), revision=current_revision)

    if state.revision != preview.base_revision:
        raise StaleRosterRevisionError(
            "Roster update preview is stale: "
            f"base revision {preview.base_revision}, current revision {state.revision}."
        )
    if roster_fingerprint(state.students) != preview.base_fingerprint:
        raise StaleRosterRevisionError(
            "Roster data changed after this update preview was created."
        )
    if preview.conflicts or preview.resulting_students is None:
        raise RosterUpdateConflictError(
            f"Roster update has {len(preview.conflicts)} unresolved conflict(s)."
        )
    return RosterState(
        students=preview.resulting_students,
        revision=state.revision + 1,
    )


def _coerce_state(
    current: RosterState | Sequence[Student],
    *,
    revision: int | None,
) -> RosterState:
    if isinstance(current, RosterState):
        if revision is not None and revision != current.revision:
            raise StaleRosterRevisionError(
                f"base revision {revision} does not match current revision "
                f"{current.revision}."
            )
        return current
    return RosterState(
        students=tuple(current),
        revision=0 if revision is None else revision,
    )


def _normalize_mode(value: str) -> RosterUpdateMode:
    if value == "full":
        return "replace"
    if value not in {"incremental", "replace"}:
        raise ValueError("mode must be incremental, replace, or full")
    return value  # type: ignore[return-value]


def _normalize_updated_fields(
    values: Iterable[RosterField | Literal["attributes"]] | None,
) -> tuple[str, ...]:
    if values is None:
        return _STUDENT_FIELDS
    requested = tuple(values)
    unknown = sorted({str(value) for value in requested if value not in _STUDENT_FIELDS})
    if unknown:
        raise ValueError("Unknown student update fields: " + ", ".join(unknown))
    requested_set = set(requested)
    return tuple(field for field in _STUDENT_FIELDS if field in requested_set)


def _index_values(
    students: Sequence[Student],
    getter: Any,
) -> dict[str, tuple[int, ...]]:
    mutable: dict[str, list[int]] = {}
    for index, student in enumerate(students):
        value = getter(student)
        if value is None:
            continue
        mutable.setdefault(str(value), []).append(index)
    return {key: tuple(indices) for key, indices in mutable.items()}


def _match_student(
    incoming: Student,
    *,
    existing: Sequence[Student],
    existing_ids: dict[str, tuple[int, ...]],
    existing_names: dict[str, tuple[int, ...]],
    incoming_index: int,
) -> tuple[int | None, RosterMatchMethod, RosterConflict | None]:
    if incoming.student_id is not None:
        id_matches = existing_ids.get(incoming.student_id, ())
        if len(id_matches) == 1:
            return id_matches[0], "student_id", None
        if len(id_matches) > 1:
            return (
                None,
                "student_id",
                RosterConflict(
                    code="ambiguous_student_id",
                    message=(
                        f"student_id {incoming.student_id!r} matches more than one "
                        "current student."
                    ),
                    incoming_index=incoming_index,
                    existing_indices=id_matches,
                ),
            )

    normalized_name = normalize_student_name(incoming.name)
    if normalized_name is None:
        return None, "new", None
    name_matches = existing_names.get(normalized_name, ())
    if not name_matches:
        return None, "new", None
    if len(name_matches) > 1:
        return (
            None,
            "name",
            RosterConflict(
                code="ambiguous_name",
                message=(
                    f"Name {incoming.name!r} matches more than one current student. "
                    "Use a student ID."
                ),
                incoming_index=incoming_index,
                existing_indices=name_matches,
            ),
        )

    existing_index = name_matches[0]
    current_id = existing[existing_index].student_id
    if (
        incoming.student_id is not None
        and current_id is not None
        and incoming.student_id != current_id
    ):
        return (
            None,
            "name",
            RosterConflict(
                code="student_id_name_mismatch",
                message=(
                    f"Name {incoming.name!r} already belongs to student_id "
                    f"{current_id!r}, not {incoming.student_id!r}."
                ),
                incoming_index=incoming_index,
                existing_indices=(existing_index,),
            ),
        )
    return existing_index, "name", None


def _merge_student(
    before: Student,
    incoming: Student,
    fields: tuple[str, ...],
    *,
    method: RosterMatchMethod,
) -> Student:
    merged = _student_data(before)
    imported = _student_data(incoming)
    for field in fields:
        # Empty IDs never erase the stable key used by history and assignments.
        if field == "student_id" and imported[field] is None:
            continue
        merged[field] = imported[field]
    # When a row matched a name-only record, preserve the newly supplied stable
    # ID even if an adapter omitted ``updated_fields`` accidentally.
    if method == "name" and before.student_id is None and incoming.student_id:
        merged["student_id"] = incoming.student_id
    return Student(**merged)


def _field_changes(before: Student, after: Student) -> tuple[RosterFieldChange, ...]:
    previous = _student_data(before)
    current = _student_data(after)
    return tuple(
        RosterFieldChange(field=field, before=previous[field], after=current[field])
        for field in _STUDENT_FIELDS
        if previous[field] != current[field]
    )


def _student_data(student: Student) -> dict[str, Any]:
    return student.model_dump(mode="python")


def _duplicate_student_keys(students: Sequence[Student]) -> tuple[str, ...]:
    keys = [student.key for student in students]
    return tuple(sorted({key for key in keys if keys.count(key) > 1}))


def _json_default(value: Any) -> str:
    if hasattr(value, "isoformat"):
        return str(value.isoformat())
    return str(value)


__all__ = [
    "RosterChange",
    "RosterChangeAction",
    "RosterConflict",
    "RosterFieldChange",
    "RosterMatchMethod",
    "RosterState",
    "RosterUpdateConflictError",
    "RosterUpdateError",
    "RosterUpdateMode",
    "RosterUpdatePreview",
    "StaleRosterRevisionError",
    "apply_roster_update",
    "normalize_student_name",
    "preview_roster_update",
    "roster_fingerprint",
]
