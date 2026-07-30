"""Bounded layout-draft ownership for browser and desktop editors."""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass, field
from threading import RLock
from time import monotonic

from seattrellis.api.models import (
    CompiledLayoutResponse,
    LayoutCellState,
    LayoutCommandRequest,
    LayoutStateResponse,
)
from seattrellis.application.layout_editor import LayoutCommand, LayoutDraft


class LayoutDraftNotFoundError(KeyError):
    """Raised when a layout draft has expired or was closed."""


class LayoutCommandConflictError(ValueError):
    """Raised for a stale, duplicate, or cross-draft command."""


@dataclass
class _StoredLayout:
    draft: LayoutDraft
    command_ids: set[str] = field(default_factory=set)
    touched_at: float = field(default_factory=monotonic)


class LayoutDraftStore:
    def __init__(self, *, max_drafts: int = 20, ttl_seconds: float = 6 * 60 * 60) -> None:
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
        self._layouts: dict[str, _StoredLayout] = {}
        self._lock = RLock()

    def create(self, draft: LayoutDraft) -> LayoutStateResponse:
        with self._lock:
            self._prune()
            while len(self._layouts) >= self.max_drafts:
                oldest = min(
                    self._layouts,
                    key=lambda key: self._layouts[key].touched_at,
                )
                del self._layouts[oldest]
            owned = deepcopy(draft)
            self._layouts[owned.draft_id] = _StoredLayout(draft=owned)
            return _state(owned)

    def state(self, draft_id: str) -> LayoutStateResponse:
        with self._lock:
            stored = self._get(draft_id)
            stored.touched_at = monotonic()
            return _state(stored.draft)

    def dispatch(
        self,
        draft_id: str,
        command: LayoutCommandRequest,
    ) -> LayoutStateResponse:
        with self._lock:
            stored = self._get(draft_id)
            cleaned = _clean_id(draft_id)
            if command.draft_id != cleaned:
                raise LayoutCommandConflictError(
                    "The layout command targets a different draft."
                )
            if command.command_id in stored.command_ids:
                raise LayoutCommandConflictError(
                    "This layout command has already been applied."
                )
            if command.base_revision != stored.draft.revision:
                raise LayoutCommandConflictError(
                    "The layout command targets a stale revision."
                )
            if command.action == "apply":
                operation = command.operation
                if operation is None:  # pragma: no cover - model validation.
                    raise ValueError("Apply commands require an operation.")
                stored.draft.apply(
                    LayoutCommand(kind=operation.kind, payload=operation.payload),
                    base_revision=command.base_revision,
                )
            elif command.action == "undo":
                stored.draft.undo(base_revision=command.base_revision)
            else:
                stored.draft.redo(base_revision=command.base_revision)
            stored.command_ids.add(command.command_id)
            stored.touched_at = monotonic()
            return _state(stored.draft)

    def compile(self, draft_id: str) -> CompiledLayoutResponse:
        with self._lock:
            stored = self._get(draft_id)
            stored.touched_at = monotonic()
            return CompiledLayoutResponse(
                draft_id=stored.draft.draft_id,
                revision=stored.draft.revision,
                layout=stored.draft.to_layout(),
            )

    def delete(self, draft_id: str) -> bool:
        with self._lock:
            return self._layouts.pop(_clean_id(draft_id), None) is not None

    def clear(self) -> None:
        with self._lock:
            self._layouts.clear()

    def _get(self, draft_id: str) -> _StoredLayout:
        self._prune()
        cleaned = _clean_id(draft_id)
        try:
            return self._layouts[cleaned]
        except KeyError as exc:
            raise LayoutDraftNotFoundError(cleaned) from exc

    def _prune(self) -> None:
        deadline = monotonic() - self.ttl_seconds
        expired = [
            draft_id
            for draft_id, stored in self._layouts.items()
            if stored.touched_at < deadline
        ]
        for draft_id in expired:
            del self._layouts[draft_id]


def _state(draft: LayoutDraft) -> LayoutStateResponse:
    return LayoutStateResponse(
        draft_id=draft.draft_id,
        revision=draft.revision,
        name=draft.name,
        rows=draft.rows,
        columns=draft.columns,
        cells=[
            LayoutCellState(
                row=cell.row,
                column=cell.column,
                kind=cell.kind,
                seat_id=cell.seat_id,
            )
            for cell in draft.ordered_cells()
        ],
        undo_depth=len(draft.undo_stack),
        redo_depth=len(draft.redo_stack),
        usable_seat_count=sum(
            cell.kind == "seat" for cell in draft.ordered_cells()
        ),
    )


def _clean_id(value: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise LayoutDraftNotFoundError("")
    return value.strip()


__all__ = [
    "LayoutCommandConflictError",
    "LayoutDraftNotFoundError",
    "LayoutDraftStore",
]
