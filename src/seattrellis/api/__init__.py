"""Local, versioned application API.

Importing this package never requires FastAPI.  ``create_app`` imports the
optional HTTP transport only when an ASGI application is actually requested.
"""

from __future__ import annotations

from typing import Any

from seattrellis.api.errors import ApiProblem
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
from seattrellis.api.security import LocalApiPolicy


def create_app(*, policy: LocalApiPolicy | None = None) -> Any:
    """Lazily construct the optional FastAPI transport."""

    from seattrellis.api.http import create_app as create_http_app

    return create_http_app(policy=policy)


__all__ = [
    "API_PREFIX",
    "API_VERSION",
    "ApiProblem",
    "GenerateClassRequest",
    "GenerateClassResponse",
    "InspectClassResponse",
    "LocalApiPolicy",
    "capabilities",
    "create_app",
    "generate_class",
    "health",
    "inspect_class_request",
    "room_templates",
    "teacher_goals",
]
