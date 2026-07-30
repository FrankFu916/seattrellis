"""Optional FastAPI transport for the local API contract."""

from __future__ import annotations

from typing import Any

from seattrellis.api.errors import ApiProblem, invalid_request_problem
from seattrellis.api.drafts import EditorDraftNotFoundError, EditorDraftStore
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
    ApiErrorDetail,
    CapabilitiesResponse,
    ErrorResponse,
    GenerateClassRequest,
    GenerateClassResponse,
    HealthResponse,
    InspectClassResponse,
    RoomTemplatesResponse,
    TeacherGoalsResponse,
)
from seattrellis.editing import EditingError
from seattrellis.editing_protocol import (
    EditorCommandEnvelope,
    EditorProtocolConflictError,
    EditorStateEnvelope,
)
from seattrellis.api.security import LocalApiPolicy
from seattrellis.optional import MissingOptionalDependencyError


def create_app(
    *,
    policy: LocalApiPolicy | None = None,
    draft_store: EditorDraftStore | None = None,
) -> Any:
    """Create the optional ASGI application without enabling broad CORS."""

    try:
        from fastapi import FastAPI, Request
        from fastapi.exceptions import RequestValidationError
        from fastapi.middleware.cors import CORSMiddleware
        from fastapi.responses import JSONResponse
    except ImportError as exc:  # pragma: no cover - depends on installation.
        raise MissingOptionalDependencyError(
            "Local Web API",
            None,
            detail="Install FastAPI and a compatible ASGI server to use this transport.",
        ) from exc

    resolved_policy = policy or LocalApiPolicy()
    resolved_store = draft_store or EditorDraftStore()
    app = FastAPI(
        title="SeatTrellis Local API",
        version="1",
        docs_url=None,
        redoc_url=None,
        openapi_url=f"{API_PREFIX}/openapi.json",
    )

    # CORS is unnecessary when the built Web client is served from this app.
    # Development clients must opt into exact origins; wildcards are rejected
    # by LocalApiPolicy before middleware is configured.
    if resolved_policy.allowed_origins:
        app.add_middleware(
            CORSMiddleware,
            allow_origins=list(resolved_policy.allowed_origins),
            allow_credentials=False,
            allow_methods=["GET", "POST", "DELETE", "OPTIONS"],
            allow_headers=["Authorization", "Content-Type"],
        )

    @app.middleware("http")
    async def enforce_local_transport(request: Request, call_next: Any) -> Any:
        try:
            resolved_policy.validate(
                host_header=request.headers.get("host"),
                origin_header=request.headers.get("origin"),
                authorization_header=request.headers.get("authorization"),
                request_scheme=request.url.scheme,
                require_session=request.method != "OPTIONS",
            )
        except ApiProblem as problem:
            return JSONResponse(
                status_code=problem.status_code,
                content=_model_data(problem.response()),
            )
        return await call_next(request)

    @app.exception_handler(ApiProblem)
    async def handle_api_problem(_request: Request, problem: ApiProblem) -> Any:
        return JSONResponse(
            status_code=problem.status_code,
            content=_model_data(problem.response()),
        )

    @app.exception_handler(RequestValidationError)
    async def handle_request_validation(
        _request: Request,
        error: RequestValidationError,
    ) -> Any:
        details: list[ApiErrorDetail] = []
        for item in error.errors():
            location = ".".join(
                str(part) for part in item.get("loc", ()) if part != "body"
            )
            details.append(
                ApiErrorDetail(
                    code="invalid_field",
                    field=location or None,
                    message="This field is missing or has an invalid value.",
                )
            )
        problem = invalid_request_problem(details)
        return JSONResponse(
            status_code=problem.status_code,
            content=_model_data(problem.response()),
        )

    @app.exception_handler(Exception)
    async def handle_unexpected_error(_request: Request, _error: Exception) -> Any:
        response = ErrorResponse(
            error={
                "code": "internal_error",
                "message": (
                    "The local service could not complete this request. "
                    "No student data was included in this error response."
                ),
            }
        )
        return JSONResponse(status_code=500, content=_model_data(response))

    def generate_with_store(request: GenerateClassRequest) -> GenerateClassResponse:
        return generate_class(request, draft_store=resolved_store)

    def get_editor_state(draft_id: str) -> EditorStateEnvelope:
        try:
            return resolved_store.state(draft_id)
        except EditorDraftNotFoundError as exc:
            raise ApiProblem(
                status_code=404,
                code="editor_draft_not_found",
                message="This editing draft has expired or was already closed.",
            ) from exc

    def dispatch_editor_command(
        draft_id: str,
        command: EditorCommandEnvelope,
    ) -> EditorStateEnvelope:
        try:
            return resolved_store.dispatch(draft_id, command)
        except EditorDraftNotFoundError as exc:
            raise ApiProblem(
                status_code=404,
                code="editor_draft_not_found",
                message="This editing draft has expired or was already closed.",
            ) from exc
        except EditorProtocolConflictError as exc:
            raise ApiProblem(
                status_code=409,
                code="editor_revision_conflict",
                message=(
                    "The seating plan changed after this action started. "
                    "Refresh the plan and try the action again."
                ),
            ) from exc
        except (EditingError, TypeError, ValueError) as exc:
            raise ApiProblem(
                status_code=422,
                code="editor_command_rejected",
                message=(
                    "That change cannot be applied because a seat or student "
                    "is locked, unavailable, or outside the current plan."
                ),
            ) from exc

    def delete_editor_draft(draft_id: str) -> Any:
        from fastapi import Response

        resolved_store.delete(draft_id)
        return Response(status_code=204)

    app.add_api_route(
        f"{API_PREFIX}/health",
        health,
        methods=["GET"],
        response_model=HealthResponse,
        tags=["system"],
    )
    app.add_api_route(
        f"{API_PREFIX}/capabilities",
        capabilities,
        methods=["GET"],
        response_model=CapabilitiesResponse,
        tags=["system"],
    )
    app.add_api_route(
        f"{API_PREFIX}/room-templates",
        room_templates,
        methods=["GET"],
        response_model=RoomTemplatesResponse,
        tags=["catalogs"],
    )
    app.add_api_route(
        f"{API_PREFIX}/teacher-goals",
        teacher_goals,
        methods=["GET"],
        response_model=TeacherGoalsResponse,
        tags=["catalogs"],
    )
    app.add_api_route(
        f"{API_PREFIX}/classes/inspect",
        inspect_class_request,
        methods=["POST"],
        response_model=InspectClassResponse,
        responses={422: {"model": ErrorResponse}},
        tags=["classes"],
    )
    app.add_api_route(
        f"{API_PREFIX}/classes/generate",
        generate_with_store,
        methods=["POST"],
        response_model=GenerateClassResponse,
        responses={
            409: {"model": ErrorResponse},
            422: {"model": ErrorResponse},
            503: {"model": ErrorResponse},
        },
        tags=["classes"],
    )
    app.add_api_route(
        f"{API_PREFIX}/editing/drafts/{{draft_id}}",
        get_editor_state,
        methods=["GET"],
        response_model=EditorStateEnvelope,
        responses={404: {"model": ErrorResponse}},
        tags=["editing"],
    )
    app.add_api_route(
        f"{API_PREFIX}/editing/drafts/{{draft_id}}/commands",
        dispatch_editor_command,
        methods=["POST"],
        response_model=EditorStateEnvelope,
        responses={
            404: {"model": ErrorResponse},
            409: {"model": ErrorResponse},
            422: {"model": ErrorResponse},
        },
        tags=["editing"],
    )
    app.add_api_route(
        f"{API_PREFIX}/editing/drafts/{{draft_id}}",
        delete_editor_draft,
        methods=["DELETE"],
        status_code=204,
        tags=["editing"],
    )
    app.state.editor_draft_store = resolved_store
    return app


def _model_data(model: Any) -> dict[str, Any]:
    if hasattr(model, "model_dump"):
        return model.model_dump(mode="json")  # type: ignore[no-any-return]
    return model.dict()
