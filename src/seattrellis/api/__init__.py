"""Local, versioned application API.

Importing this package never requires FastAPI.  ``create_app`` imports the
optional HTTP transport only when an ASGI application is actually requested.
"""

from __future__ import annotations

from typing import Any

from seattrellis.api.errors import ApiProblem
from seattrellis.api.drafts import EditorDraftNotFoundError, EditorDraftStore
from seattrellis.api.layouts import LayoutDraftNotFoundError, LayoutDraftStore
from seattrellis.api.handlers import (
    capabilities,
    generate_class,
    health,
    inspect_class_request,
    room_templates,
    teacher_goals,
)
from seattrellis.api.models import (
    API_PREFIX,
    API_VERSION,
    GenerateClassRequest,
    GenerateClassResponse,
    InspectClassResponse,
)
from seattrellis.api.rosters import RosterDraftNotFoundError, RosterDraftStore
from seattrellis.api.security import LocalApiPolicy


def create_app(
    *,
    policy: LocalApiPolicy | None = None,
    draft_store: EditorDraftStore | None = None,
    layout_store: LayoutDraftStore | None = None,
    roster_store: RosterDraftStore | None = None,
) -> Any:
    """Lazily construct the optional FastAPI transport."""

    from seattrellis.api.http import create_app as create_http_app

    return create_http_app(
        policy=policy,
        draft_store=draft_store,
        layout_store=layout_store,
        roster_store=roster_store,
    )


__all__ = [
    "API_PREFIX",
    "API_VERSION",
    "ApiProblem",
    "EditorDraftNotFoundError",
    "EditorDraftStore",
    "LayoutDraftNotFoundError",
    "LayoutDraftStore",
    "GenerateClassRequest",
    "GenerateClassResponse",
    "InspectClassResponse",
    "LocalApiPolicy",
    "RosterDraftNotFoundError",
    "RosterDraftStore",
    "capabilities",
    "create_app",
    "generate_class",
    "health",
    "inspect_class_request",
    "room_templates",
    "teacher_goals",
]
