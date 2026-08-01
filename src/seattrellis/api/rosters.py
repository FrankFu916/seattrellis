"""Bounded roster-upload drafts and safe import previews."""

from __future__ import annotations

from dataclasses import dataclass, field
from math import isfinite
from threading import RLock
from time import monotonic
from typing import Any
from uuid import uuid4

from seattrellis.api.models import (
    RosterChangeItem,
    RosterColumnItem,
    RosterConflictItem,
    RosterDraftResponse,
    RosterFieldChangeItem,
    RosterMappingIssueItem,
    RosterMappingItem,
    RosterPreviewRow,
    RosterUpdatePreviewRequest,
    RosterUpdatePreviewResponse,
)
from seattrellis.application.roster_mapping import (
    ColumnMapping,
    create_roster_mapping,
    students_from_roster_mapping,
    suggest_roster_mapping,
)
from seattrellis.application.roster_update import (
    RosterState,
    preview_roster_update,
)
from seattrellis.io.roster_table import RosterTable


class RosterDraftNotFoundError(KeyError):
    """Raised when a roster upload has expired or has been closed."""


@dataclass
class _StoredRoster:
    table: RosterTable
    draft_id: str = field(default_factory=lambda: uuid4().hex)
    touched_at: float = field(default_factory=monotonic)


class RosterDraftStore:
    """Keep uploaded classroom data in memory for one local browser session."""

    def __init__(
        self,
        *,
        max_drafts: int = 10,
        ttl_seconds: float = 2 * 60 * 60,
    ) -> None:
        if isinstance(max_drafts, bool) or not isinstance(max_drafts, int):
            raise TypeError("max_drafts must be an integer")
        if max_drafts < 1:
            raise ValueError("max_drafts must be positive")
        if (
            isinstance(ttl_seconds, bool)
            or not isinstance(ttl_seconds, (int, float))
            or not isfinite(float(ttl_seconds))
            or ttl_seconds <= 0
        ):
            raise ValueError("ttl_seconds must be a positive finite number")
        self.max_drafts = max_drafts
        self.ttl_seconds = float(ttl_seconds)
        self._drafts: dict[str, _StoredRoster] = {}
        self._lock = RLock()

    def create(self, table: RosterTable) -> RosterDraftResponse:
        stored = _StoredRoster(table=table)
        with self._lock:
            self._prune()
            while len(self._drafts) >= self.max_drafts:
                oldest = min(
                    self._drafts,
                    key=lambda key: self._drafts[key].touched_at,
                )
                del self._drafts[oldest]
            self._drafts[stored.draft_id] = stored
            return _draft_response(stored)

    def state(self, draft_id: str) -> RosterDraftResponse:
        with self._lock:
            stored = self._get(draft_id)
            stored.touched_at = monotonic()
            return _draft_response(stored)

    def preview_update(
        self,
        draft_id: str,
        request: RosterUpdatePreviewRequest,
    ) -> RosterUpdatePreviewResponse:
        with self._lock:
            stored = self._get(draft_id)
            mapping = create_roster_mapping(
                stored.table,
                (
                    ColumnMapping(
                        field=item.field,
                        column_index=item.column_index,
                    )
                    for item in request.mapping
                ),
            )
            incoming = students_from_roster_mapping(stored.table, mapping)
            preview = preview_roster_update(
                RosterState(
                    students=tuple(request.current_students),
                    revision=request.current_revision,
                ),
                incoming,
                mode=request.mode,
                updated_fields=request.updated_fields,
            )
            stored.touched_at = monotonic()
            return RosterUpdatePreviewResponse(
                draft_id=stored.draft_id,
                base_revision=preview.base_revision,
                mode=preview.mode,
                can_apply=preview.can_apply,
                action_counts={
                    action: preview.count(action)
                    for action in (
                        "add",
                        "update",
                        "unchanged",
                        "remove",
                        "conflict",
                    )
                },
                changes=[
                    RosterChangeItem(
                        action=change.action,
                        match_method=change.match_method,
                        before=change.before,
                        after=change.after,
                        field_changes=[
                            RosterFieldChangeItem(
                                field=field_change.field,
                                before=field_change.before,
                                after=field_change.after,
                            )
                            for field_change in change.field_changes
                        ],
                        incoming_index=change.incoming_index,
                        existing_index=change.existing_index,
                    )
                    for change in preview.changes
                ],
                conflicts=[
                    RosterConflictItem(
                        code=conflict.code,
                        message=conflict.message,
                        incoming_index=conflict.incoming_index,
                        existing_indices=list(conflict.existing_indices),
                    )
                    for conflict in preview.conflicts
                ],
                resulting_students=(
                    list(preview.resulting_students)
                    if preview.resulting_students is not None
                    else None
                ),
            )

    def delete(self, draft_id: str) -> bool:
        with self._lock:
            return self._drafts.pop(_clean_id(draft_id), None) is not None

    def clear(self) -> None:
        with self._lock:
            self._drafts.clear()

    def _get(self, draft_id: str) -> _StoredRoster:
        self._prune()
        cleaned = _clean_id(draft_id)
        try:
            return self._drafts[cleaned]
        except KeyError as exc:
            raise RosterDraftNotFoundError(cleaned) from exc

    def _prune(self) -> None:
        deadline = monotonic() - self.ttl_seconds
        expired = [
            draft_id
            for draft_id, stored in self._drafts.items()
            if stored.touched_at < deadline
        ]
        for draft_id in expired:
            del self._drafts[draft_id]


def _draft_response(stored: _StoredRoster) -> RosterDraftResponse:
    suggestion = suggest_roster_mapping(stored.table)
    return RosterDraftResponse(
        draft_id=stored.draft_id,
        source_format=stored.table.source_format,
        headerless=stored.table.headerless,
        row_count=stored.table.row_count,
        column_count=stored.table.column_count,
        columns=[
            RosterColumnItem(
                index=column.index,
                header=_short_text(column.header),
            )
            for column in stored.table.columns
        ],
        preview_rows=[
            RosterPreviewRow(
                row_number=row.row_number,
                cells=[
                    _preview_cell(row.cell(column.index))
                    for column in stored.table.columns
                ],
            )
            for row in stored.table.rows[:5]
        ],
        suggested_mapping=[
            RosterMappingItem(
                field=assignment.field,
                column_index=assignment.column_index,
            )
            for assignment in suggestion.mapping.assignments
        ],
        mapping_issues=[
            RosterMappingIssueItem(
                code=issue.code,
                message=issue.message,
                field=issue.field,
                column_indices=list(issue.column_indices),
            )
            for issue in suggestion.issues
        ],
    )


def _preview_cell(value: Any) -> str | int | float | bool | None:
    if value is None or isinstance(value, (bool, int)):
        return value
    if isinstance(value, float):
        return value if isfinite(value) else str(value)
    return _short_text(value)


def _short_text(value: Any, *, limit: int = 160) -> str:
    text = str(value)
    return text if len(text) <= limit else text[: limit - 1] + "…"


def _clean_id(value: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise RosterDraftNotFoundError("")
    return value.strip()


__all__ = [
    "RosterDraftNotFoundError",
    "RosterDraftStore",
]
