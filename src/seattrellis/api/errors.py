"""Structured, privacy-conscious errors for local API adapters."""

from __future__ import annotations

from collections.abc import Iterable

from seattrellis.api.models import ApiErrorDetail, ApiErrorPayload, ErrorResponse


class ApiProblem(Exception):
    """An expected application failure safe to return to a local client.

    Messages carried by this exception must describe how to recover without
    echoing student records, rule references, file paths, or solver internals.
    """

    def __init__(
        self,
        *,
        status_code: int,
        code: str,
        message: str,
        details: Iterable[ApiErrorDetail] = (),
    ) -> None:
        super().__init__(message)
        self.status_code = status_code
        self.code = code
        self.message = message
        self.details = tuple(details)

    def response(self) -> ErrorResponse:
        """Build the versioned response envelope for this problem."""

        return ErrorResponse(
            error=ApiErrorPayload(
                code=self.code,
                message=self.message,
                details=list(self.details),
            )
        )


def invalid_request_problem(
    details: Iterable[ApiErrorDetail] = (),
) -> ApiProblem:
    """Return a generic request error that never includes submitted values."""

    return ApiProblem(
        status_code=422,
        code="invalid_request",
        message="Some request fields are invalid. Review the highlighted fields.",
        details=details,
    )

