"""Short-lived, data-minimized editing drafts for the local API."""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass, field
from threading import RLock
from time import monotonic
from uuid import uuid4

from seattrellis.editing import EditingError, EditingSession, lock_state_from_snapshot
from seattrellis.editing_protocol import (
    EDITOR_PROTOCOL_VERSION,
    EditorCommandEnvelope,
    EditorHardConstraintState,
    EditorProtocolConflictError,
    EditorSeatState,
    EditorStateEnvelope,
    EditorStudentState,
    operation_to_domain,
)
from seattrellis.models.candidate import CandidateSet
from seattrellis.models.snapshot import SeatingSnapshot


class EditorDraftNotFoundError(KeyError):
    """Raised when an editing draft has expired or has been deleted."""


@dataclass
class _StoredDraft:
    candidate_set: CandidateSet
    candidate_id: str
    session: EditingSession
    draft_id: str
    revision: int = 0
    undo_stack: list[EditingSession] = field(default_factory=list)
    redo_stack: list[EditingSession] = field(default_factory=list)
    applied_command_ids: set[str] = field(default_factory=set)
    command_log: list[dict[str, object]] = field(default_factory=list)
    touched_at: float = field(default_factory=monotonic)


class EditorDraftStore:
    """Bounded in-memory ownership for sensitive local editing state.

    Drafts are never written to a browser cache or a server-side path.  The
    store is deliberately process-local, bounded, and expiring so closing the
    workspace releases all student data without a cleanup migration.
    """

    def __init__(
        self,
        *,
        max_drafts: int = 20,
        ttl_seconds: float = 6 * 60 * 60,
    ) -> None:
        if isinstance(max_drafts, bool) or not isinstance(max_drafts, int):
            raise TypeError("max_drafts must be an integer")
        if max_drafts < 1:
            raise ValueError("max_drafts must be positive")
        if (
            isinstance(ttl_seconds, bool)
            or not isinstance(ttl_seconds, (int, float))
            or ttl_seconds <= 0
        ):
            raise ValueError("ttl_seconds must be positive")
        self.max_drafts = max_drafts
        self.ttl_seconds = float(ttl_seconds)
        self._drafts: dict[str, _StoredDraft] = {}
        self._lock = RLock()

    def create(self, candidate_set: CandidateSet) -> EditorStateEnvelope:
        """Own a defensive copy of the recommended candidate and return its view."""

        candidate_copy = _copy_candidate_set(candidate_set)
        candidate = candidate_copy.get_candidate("recommended")
        locks = lock_state_from_snapshot(candidate.snapshot)
        stored = _StoredDraft(
            candidate_set=candidate_copy,
            candidate_id=candidate.candidate_id,
            session=EditingSession.from_snapshot(
                candidate.snapshot,
                locked_students=locks.locked_students,
                locked_seats=locks.locked_seats,
            ),
            draft_id=uuid4().hex,
        )
        with self._lock:
            self._prune()
            while len(self._drafts) >= self.max_drafts:
                oldest_id = min(
                    self._drafts,
                    key=lambda item: self._drafts[item].touched_at,
                )
                del self._drafts[oldest_id]
            self._drafts[stored.draft_id] = stored
            return _build_state(stored)

    def state(self, draft_id: str) -> EditorStateEnvelope:
        with self._lock:
            stored = self._get(draft_id)
            stored.touched_at = monotonic()
            return _build_state(stored)

    def snapshot(self, draft_id: str) -> SeatingSnapshot:
        """Return the current plan as a defensive snapshot for export.

        The snapshot reflects every editing command applied to the draft, so
        an exported file matches what the teacher currently sees.  Callers
        receive a deep copy and may not mutate the stored draft through it.
        """

        with self._lock:
            stored = self._get(draft_id)
            stored.touched_at = monotonic()
            current = stored.session.current_snapshot()
            if stored.command_log:
                metadata = dict(current.metadata)
                metadata["manual_edit"] = {
                    "source": "web_editor",
                    "draft_id": stored.draft_id,
                    "operation_count": len(stored.command_log),
                    "commands": deepcopy(stored.command_log),
                }
                current = current.model_copy(update={"metadata": metadata})
            return current.model_copy(deep=True)

    def dispatch(
        self,
        draft_id: str,
        command: EditorCommandEnvelope,
    ) -> EditorStateEnvelope:
        """Apply one versioned command atomically and return minimized state."""

        with self._lock:
            stored = self._get(draft_id)
            _validate_command_target(stored, draft_id, command)
            if command.action == "apply":
                self._apply(stored, command)
            elif command.action == "undo":
                self._undo(stored)
            else:
                self._redo(stored)
            stored.revision += 1
            stored.applied_command_ids.add(command.command_id)
            stored.command_log.append(command.model_dump(mode="json", exclude_none=True))
            stored.touched_at = monotonic()
            return _build_state(stored)

    def delete(self, draft_id: str) -> bool:
        """Forget a draft immediately; repeated deletion is safe."""

        with self._lock:
            return self._drafts.pop(_clean_draft_id(draft_id), None) is not None

    def clear(self) -> None:
        with self._lock:
            self._drafts.clear()

    def _get(self, draft_id: str) -> _StoredDraft:
        self._prune()
        cleaned = _clean_draft_id(draft_id)
        try:
            return self._drafts[cleaned]
        except KeyError as exc:
            raise EditorDraftNotFoundError(cleaned) from exc

    def _apply(
        self,
        stored: _StoredDraft,
        command: EditorCommandEnvelope,
    ) -> None:
        before = deepcopy(stored.session)
        working = deepcopy(stored.session)
        try:
            for operation in command.operations:
                working.apply(operation_to_domain(operation))
        except (EditingError, TypeError, ValueError):
            # The shared protocol promises atomic command batches.  No partial
            # assignment or lock change becomes visible when one item fails.
            raise
        stored.undo_stack.append(before)
        stored.redo_stack.clear()
        stored.session = _clean_session(working)

    def _undo(self, stored: _StoredDraft) -> None:
        if not stored.undo_stack:
            raise EditingError("There is no editing command to undo.")
        stored.redo_stack.append(deepcopy(stored.session))
        stored.session = stored.undo_stack.pop()

    def _redo(self, stored: _StoredDraft) -> None:
        if not stored.redo_stack:
            raise EditingError("There is no editing command to redo.")
        stored.undo_stack.append(deepcopy(stored.session))
        stored.session = stored.redo_stack.pop()

    def _prune(self) -> None:
        deadline = monotonic() - self.ttl_seconds
        expired = [
            draft_id
            for draft_id, stored in self._drafts.items()
            if stored.touched_at < deadline
        ]
        for draft_id in expired:
            del self._drafts[draft_id]


def _validate_command_target(
    stored: _StoredDraft,
    path_draft_id: str,
    command: EditorCommandEnvelope,
) -> None:
    cleaned = _clean_draft_id(path_draft_id)
    if command.draft_id != cleaned or command.draft_id != stored.draft_id:
        raise EditorProtocolConflictError(
            "The editor command targets a different draft."
        )
    if command.command_id in stored.applied_command_ids:
        raise EditorProtocolConflictError(
            "This editor command has already been applied."
        )
    if command.base_revision != stored.revision:
        raise EditorProtocolConflictError(
            "The editor command targets a stale draft revision."
        )


def _build_state(stored: _StoredDraft) -> EditorStateEnvelope:
    snapshot = stored.session.current_snapshot()
    assignment_by_student = {
        assignment.student_key: assignment for assignment in snapshot.assignments
    }
    assignment_by_seat = {
        assignment.seat_id: assignment for assignment in snapshot.assignments
    }
    hard = stored.session.hard_constraint_summary()
    return EditorStateEnvelope(
        kind="seattrellis_editor_state",
        protocol_version=EDITOR_PROTOCOL_VERSION,
        draft_id=stored.draft_id,
        revision=stored.revision,
        candidate_id=stored.candidate_id,
        undo_depth=len(stored.undo_stack),
        redo_depth=len(stored.redo_stack),
        students=[
            EditorStudentState(
                student_key=student.key,
                display_name=student.display_name,
                seat_id=(
                    assignment_by_student[student.key].seat_id
                    if student.key in assignment_by_student
                    else None
                ),
                locked=student.key in stored.session.locked_students,
            )
            for student in snapshot.students
        ],
        seats=[
            EditorSeatState(
                seat_id=seat.seat_id,
                row=seat.row,
                col=seat.col,
                enabled=seat.enabled,
                student_key=(
                    assignment_by_seat[seat.seat_id].student_key
                    if seat.seat_id in assignment_by_seat
                    else None
                ),
                locked=seat.seat_id in stored.session.locked_seats,
            )
            for seat in snapshot.layout.seats
        ],
        hard_constraints=EditorHardConstraintState(
            satisfied=hard.satisfied,
            checked_rule_count=hard.checked_rule_count,
            violation_count=hard.violation_count,
            # Raw diagnostics can contain student or rule identifiers.  The
            # browser only needs an actionable aggregate until a teacher opens
            # an explicitly authorized diagnostic view.
            violations=(
                ["One or more required seating rules are not satisfied."]
                if hard.violation_count
                else []
            ),
        ),
    )


def _clean_session(session: EditingSession) -> EditingSession:
    """Drop operation-level history; the API owns command-level history."""

    locks = session.lock_state
    return EditingSession.from_snapshot(
        session.current_snapshot(),
        locked_students=locks.locked_students,
        locked_seats=locks.locked_seats,
    )


def _copy_candidate_set(candidate_set: CandidateSet) -> CandidateSet:
    return candidate_set.model_copy(deep=True)


def _clean_draft_id(value: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise EditorDraftNotFoundError("")
    return value.strip()


__all__ = [
    "EditorDraftNotFoundError",
    "EditorDraftStore",
]
