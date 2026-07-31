"""Optional FastAPI transport for the local API contract."""

from __future__ import annotations

from typing import Any
from uuid import uuid4

from seattrellis.api.errors import ApiProblem, invalid_request_problem
from seattrellis.api.drafts import EditorDraftNotFoundError, EditorDraftStore
from seattrellis.api.handlers import (
    ExportArtifact,
    capabilities,
    catalogs,
    export_draft,
    generate_class,
    generate_rotation_plan,
    health,
    inspect_class_request,
    room_templates,
    teacher_goals,
)
from seattrellis.api.layouts import (
    LayoutCommandConflictError,
    LayoutDraftNotFoundError,
    LayoutDraftStore,
)
from seattrellis.api.rosters import RosterDraftNotFoundError, RosterDraftStore
from seattrellis.api.models import (
    API_PREFIX,
    ApiErrorDetail,
    CapabilitiesResponse,
    ErrorResponse,
    ExportDraftRequest,
    GenerateClassRequest,
    GenerateClassResponse,
    GenerateRotationPlanRequest,
    GenerateRotationPlanResponse,
    HealthResponse,
    InspectClassResponse,
    CompiledLayoutResponse,
    CreateLayoutDraftRequest,
    LayoutCommandRequest,
    LayoutStateResponse,
    RoomTemplatesResponse,
    RosterDraftResponse,
    RosterUpdatePreviewRequest,
    RosterUpdatePreviewResponse,
    TeacherGoalsResponse,
)
from seattrellis.editing import EditingError
from seattrellis.editing_protocol import (
    EditorCommandEnvelope,
    EditorProtocolConflictError,
    EditorStateEnvelope,
)
from seattrellis.api.security import LocalApiPolicy
from seattrellis.application.layout_editor import (
    LayoutDraft,
    LayoutEditingError,
    LayoutRevisionConflictError,
)
from seattrellis.application.room_templates import build_room_from_template
from seattrellis.io.json_files import InputFileError
from seattrellis.io.roster_table import (
    DEFAULT_MAX_ROSTER_FILE_BYTES,
    read_roster_table_bytes,
)
from seattrellis.optional import MissingOptionalDependencyError


def create_app(
    *,
    policy: LocalApiPolicy | None = None,
    draft_store: EditorDraftStore | None = None,
    layout_store: LayoutDraftStore | None = None,
    roster_store: RosterDraftStore | None = None,
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
    resolved_layout_store = layout_store or LayoutDraftStore()
    resolved_roster_store = roster_store or RosterDraftStore()
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

    def generate_rotation(request: GenerateRotationPlanRequest) -> GenerateRotationPlanResponse:
        return generate_rotation_plan(request)

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

    def create_layout_draft(request: CreateLayoutDraftRequest) -> LayoutStateResponse:
        try:
            if request.layout is not None:
                draft = LayoutDraft.from_layout(request.layout)
                draft.name = request.name
            elif request.template_id is not None:
                draft = LayoutDraft.from_layout(
                    build_room_from_template(request.template_id, name=request.name)
                )
            else:
                draft = LayoutDraft.rectangular(
                    request.rows or 1,
                    request.columns or 1,
                    name=request.name,
                )
            # Source layout IDs identify saved room definitions, not editing
            # sessions. Every browser draft receives an independent opaque ID.
            draft.draft_id = uuid4().hex
            return resolved_layout_store.create(draft)
        except (KeyError, LayoutEditingError, TypeError, ValueError) as exc:
            raise ApiProblem(
                status_code=422,
                code="invalid_layout_draft",
                message="The classroom layout could not be created from these settings.",
            ) from exc

    def get_layout_state(draft_id: str) -> LayoutStateResponse:
        try:
            return resolved_layout_store.state(draft_id)
        except LayoutDraftNotFoundError as exc:
            raise _layout_not_found_problem() from exc

    def dispatch_layout_command(
        draft_id: str,
        command: LayoutCommandRequest,
    ) -> LayoutStateResponse:
        try:
            return resolved_layout_store.dispatch(draft_id, command)
        except LayoutDraftNotFoundError as exc:
            raise _layout_not_found_problem() from exc
        except (LayoutCommandConflictError, LayoutRevisionConflictError) as exc:
            raise ApiProblem(
                status_code=409,
                code="layout_revision_conflict",
                message=(
                    "The classroom layout changed after this action started. "
                    "Refresh it and try again."
                ),
            ) from exc
        except (LayoutEditingError, TypeError, ValueError) as exc:
            raise ApiProblem(
                status_code=422,
                code="layout_command_rejected",
                message="That layout change does not fit the current classroom grid.",
            ) from exc

    def compile_layout_draft(draft_id: str) -> CompiledLayoutResponse:
        try:
            return resolved_layout_store.compile(draft_id)
        except LayoutDraftNotFoundError as exc:
            raise _layout_not_found_problem() from exc
        except LayoutEditingError as exc:
            raise ApiProblem(
                status_code=422,
                code="layout_not_ready",
                message="Add at least one usable seat before using this classroom.",
            ) from exc

    def delete_layout_draft(draft_id: str) -> Any:
        from fastapi import Response

        resolved_layout_store.delete(draft_id)
        return Response(status_code=204)

    async def create_roster_draft(request: Any) -> RosterDraftResponse:
        try:
            form = await request.form()
            upload = form.get("file")
            filename = getattr(upload, "filename", None)
            read = getattr(upload, "read", None)
            close = getattr(upload, "close", None)
            if not isinstance(filename, str) or not filename.strip() or read is None:
                raise ApiProblem(
                    status_code=422,
                    code="roster_file_required",
                    message="Choose one CSV or Excel roster file to continue.",
                )
            try:
                data = await read(DEFAULT_MAX_ROSTER_FILE_BYTES + 1)
            finally:
                if close is not None:
                    await close()
            if not isinstance(data, bytes):
                raise TypeError("Roster uploads must contain bytes.")
            if len(data) > DEFAULT_MAX_ROSTER_FILE_BYTES:
                raise ApiProblem(
                    status_code=413,
                    code="roster_file_too_large",
                    message="The roster file is larger than the 20 MB limit.",
                )
            table = read_roster_table_bytes(data, filename=filename)
            return resolved_roster_store.create(table)
        except ApiProblem:
            raise
        except MissingOptionalDependencyError as exc:
            raise ApiProblem(
                status_code=503,
                code="feature_unavailable",
                message="Excel roster preview is not available in this installation.",
            ) from exc
        except (InputFileError, TypeError, ValueError) as exc:
            raise ApiProblem(
                status_code=422,
                code="invalid_roster_file",
                message=(
                    "The roster could not be read. Use a UTF-8 CSV or a valid "
                    "Excel workbook with a header row."
                ),
            ) from exc

    # The transport remains optional, so Request is imported inside
    # ``create_app``. Give FastAPI the concrete runtime annotation instead of
    # a module-level dependency on Starlette.
    create_roster_draft.__annotations__["request"] = Request

    def get_roster_draft(draft_id: str) -> RosterDraftResponse:
        try:
            return resolved_roster_store.state(draft_id)
        except RosterDraftNotFoundError as exc:
            raise _roster_not_found_problem() from exc

    def preview_roster_update(
        draft_id: str,
        request: RosterUpdatePreviewRequest,
    ) -> RosterUpdatePreviewResponse:
        try:
            return resolved_roster_store.preview_update(draft_id, request)
        except RosterDraftNotFoundError as exc:
            raise _roster_not_found_problem() from exc
        except (InputFileError, TypeError, ValueError) as exc:
            raise ApiProblem(
                status_code=422,
                code="roster_mapping_rejected",
                message=(
                    "The selected columns could not be converted into a valid "
                    "student list. Review the identity and numeric columns."
                ),
            ) from exc

    def delete_roster_draft(draft_id: str) -> Any:
        from fastapi import Response

        resolved_roster_store.delete(draft_id)
        return Response(status_code=204)

    def export_with_store(request: ExportDraftRequest) -> Any:
        from fastapi.responses import Response

        artifact: ExportArtifact = export_draft(request, draft_store=resolved_store)
        return Response(
            content=artifact.data,
            media_type=artifact.content_type,
            headers={
                "Content-Disposition": f'attachment; filename="{artifact.filename}"'
            },
        )

    app.add_api_route(
        f"{API_PREFIX}/health",
        health,
        methods=["GET"],
        response_model=None,
        tags=["system"],
    )
    app.add_api_route(
        f"{API_PREFIX}/capabilities",
        capabilities,
        methods=["GET"],
        response_model=None,
        tags=["system"],
    )
    app.add_api_route(
        f"{API_PREFIX}/room-templates",
        room_templates,
        methods=["GET"],
        response_model=None,
        tags=["catalogs"],
    )
    app.add_api_route(
        f"{API_PREFIX}/teacher-goals",
        teacher_goals,
        methods=["GET"],
        response_model=None,
        tags=["catalogs"],
    )
    app.add_api_route(
        f"{API_PREFIX}/catalogs",
        catalogs,
        methods=["GET"],
        response_model=None,
        tags=["catalogs"],
    )
    app.add_api_route(
        f"{API_PREFIX}/classes/inspect",
        inspect_class_request,
        methods=["POST"],
        response_model=None,
        responses={422: {"model": ErrorResponse}},
        tags=["classes"],
    )
    app.add_api_route(
        f"{API_PREFIX}/classes/generate",
        generate_with_store,
        methods=["POST"],
        response_model=None,
        responses={
            409: {"model": ErrorResponse},
            422: {"model": ErrorResponse},
            503: {"model": ErrorResponse},
        },
        tags=["classes"],
    )
    app.add_api_route(
        f"{API_PREFIX}/classes/rotation",
        generate_rotation,
        methods=["POST"],
        response_model=None,
        responses={422: {"model": ErrorResponse}, 409: {"model": ErrorResponse}},
        tags=["classes"],
    )
    app.add_api_route(
        f"{API_PREFIX}/editing/drafts/{{draft_id}}",
        get_editor_state,
        methods=["GET"],
        response_model=None,
        responses={404: {"model": ErrorResponse}},
        tags=["editing"],
    )
    app.add_api_route(
        f"{API_PREFIX}/editing/drafts/{{draft_id}}/commands",
        dispatch_editor_command,
        methods=["POST"],
        response_model=None,
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
        response_model=None,
        tags=["editing"],
    )
    app.add_api_route(
        f"{API_PREFIX}/layouts/drafts",
        create_layout_draft,
        methods=["POST"],
        response_model=None,
        responses={422: {"model": ErrorResponse}},
        tags=["layouts"],
    )
    app.add_api_route(
        f"{API_PREFIX}/layouts/drafts/{{draft_id}}",
        get_layout_state,
        methods=["GET"],
        response_model=None,
        responses={404: {"model": ErrorResponse}},
        tags=["layouts"],
    )
    app.add_api_route(
        f"{API_PREFIX}/layouts/drafts/{{draft_id}}/commands",
        dispatch_layout_command,
        methods=["POST"],
        response_model=None,
        responses={
            404: {"model": ErrorResponse},
            409: {"model": ErrorResponse},
            422: {"model": ErrorResponse},
        },
        tags=["layouts"],
    )
    app.add_api_route(
        f"{API_PREFIX}/layouts/drafts/{{draft_id}}/compiled",
        compile_layout_draft,
        methods=["GET"],
        response_model=None,
        responses={404: {"model": ErrorResponse}, 422: {"model": ErrorResponse}},
        tags=["layouts"],
    )
    app.add_api_route(
        f"{API_PREFIX}/layouts/drafts/{{draft_id}}",
        delete_layout_draft,
        methods=["DELETE"],
        status_code=204,
        response_model=None,
        tags=["layouts"],
    )
    app.add_api_route(
        f"{API_PREFIX}/rosters/drafts",
        create_roster_draft,
        methods=["POST"],
        response_model=None,
        responses={
            413: {"model": ErrorResponse},
            422: {"model": ErrorResponse},
            503: {"model": ErrorResponse},
        },
        tags=["rosters"],
    )
    app.add_api_route(
        f"{API_PREFIX}/rosters/drafts/{{draft_id}}",
        get_roster_draft,
        methods=["GET"],
        response_model=None,
        responses={404: {"model": ErrorResponse}},
        tags=["rosters"],
    )
    app.add_api_route(
        f"{API_PREFIX}/rosters/drafts/{{draft_id}}/preview",
        preview_roster_update,
        methods=["POST"],
        response_model=None,
        responses={404: {"model": ErrorResponse}, 422: {"model": ErrorResponse}},
        tags=["rosters"],
    )
    app.add_api_route(
        f"{API_PREFIX}/rosters/drafts/{{draft_id}}",
        delete_roster_draft,
        methods=["DELETE"],
        status_code=204,
        response_model=None,
        tags=["rosters"],
    )
    app.add_api_route(
        f"{API_PREFIX}/exports",
        export_with_store,
        methods=["POST"],
        response_model=None,
        responses={
            404: {"model": ErrorResponse},
            422: {"model": ErrorResponse},
            503: {"model": ErrorResponse},
        },
        tags=["exports"],
    )
    app.state.editor_draft_store = resolved_store
    app.state.layout_draft_store = resolved_layout_store
    app.state.roster_draft_store = resolved_roster_store
    return app


def _layout_not_found_problem() -> ApiProblem:
    return ApiProblem(
        status_code=404,
        code="layout_draft_not_found",
        message="This classroom layout draft has expired or was already closed.",
    )


def _roster_not_found_problem() -> ApiProblem:
    return ApiProblem(
        status_code=404,
        code="roster_draft_not_found",
        message="This roster preview has expired or was already closed.",
    )


def _model_data(model: Any) -> dict[str, Any]:
    return model.model_dump(mode="json")
