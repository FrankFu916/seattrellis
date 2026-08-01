from __future__ import annotations

import builtins

import pytest

from seattrellis.api import create_app
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
    GenerateClassRequest,
    GenerateRotationPlanRequest,
)
from seattrellis.optional import MissingOptionalDependencyError


def _request(
    *,
    seat_count: int = 2,
    goal_id: str = "quick-shuffle",
) -> GenerateClassRequest:
    seats = [
        {"seat_id": f"A{index}", "row": 1, "col": index}
        for index in range(1, seat_count + 1)
    ]
    return GenerateClassRequest.model_validate(
        {
            "draft": {
                "name": "Class A",
                "students": [
                    {"student_id": "PRIVATE-001", "name": "Private Name One"},
                    {"student_id": "PRIVATE-002", "name": "Private Name Two"},
                ],
                "room": {
                    "layout": {
                        "layout_id": "small-room",
                        "name": "Small room",
                        "seats": seats,
                    }
                },
                "goal": {"goal_id": goal_id},
            },
            "options": {
                "candidate_count": 1,
                "seed": 11,
                "time_limit_seconds": 0.2,
                "backend": "fallback",
            },
        }
    )


def test_system_and_catalog_contracts_are_versioned() -> None:
    health_response = health()
    capability_response = capabilities()
    rooms_response = room_templates()
    goals_response = teacher_goals()

    assert health_response.api_version == "1"
    assert health_response.local_only
    assert capability_response.api_version == "1"
    assert capability_response.features == [
        "class-inspection",
        "class-generation",
        "rotation-plans",
        "project-workspace",
        "layout-editing",
        "roster-mapping",
        "roster-update-preview",
        "room-templates",
        "teacher-goals",
    ]
    assert [backend.name for backend in capability_response.solver_backends] == [
        "fallback",
        "ortools",
        "native",
    ]
    assert capability_response.limits["candidate_count_max"] == 20
    assert [room.capacity for room in rooms_response.room_templates] == [30, 48, 60]
    assert [goal.goal_id for goal in goals_response.teacher_goals] == [
        "daily-rotation",
        "quick-shuffle",
        "fair-shuffle",
        "peer-support",
        "custom",
    ]


def test_inspect_class_request_reuses_teacher_goal_and_validation_layers() -> None:
    response = inspect_class_request(_request())

    assert response.api_version == "1"
    assert response.class_name == "Class A"
    assert response.goal.goal_id == "quick-shuffle"
    assert response.goal.preset_name == "random"
    assert response.validation.ready
    assert response.validation.students_count == 2
    assert response.validation.enabled_seats_count == 2
    assert response.warnings == []


def test_generate_request_can_combine_goal_with_common_hard_rules() -> None:
    payload = _request().model_dump(mode="json")
    payload["draft"]["goal"]["hard_rules"] = {
        "cannot_be_adjacent": [{"students": ["PRIVATE-001", "PRIVATE-002"]}]
    }
    payload["draft"]["goal"]["rules_overlay"] = {
        "soft": {"vision_front": {"enabled": False}}
    }
    request = GenerateClassRequest.model_validate(payload)

    response = inspect_class_request(request)

    assert response.validation.ready
    assert response.goal.goal_id == "quick-shuffle"


def test_generate_class_runs_through_existing_application_workflow() -> None:
    response = generate_class(_request())

    assert response.api_version == "1"
    assert response.class_name == "Class A"
    assert response.goal.goal_id == "quick-shuffle"
    assert len(response.candidates) == 1
    assert response.recommended_candidate_id == "candidate_01"
    assert response.candidates[0].candidate_id == "candidate_01"
    assert response.candidates[0].recommended
    assert {student.student_key for student in response.editor.students} == {
        "PRIVATE-001",
        "PRIVATE-002",
    }
    serialized = response.json()
    assert "score_balance" not in serialized
    assert "solver_backend" not in serialized


def test_generate_rotation_plan_returns_versioned_periods() -> None:
    request = GenerateRotationPlanRequest.model_validate(
        {
            **_request().model_dump(mode="json"),
            "period_count": 2,
            "period_labels": ["Monday", "Friday"],
        }
    )
    from seattrellis.api.handlers import generate_rotation_plan

    response = generate_rotation_plan(request)
    assert response.api_version == "1"
    assert [period.label for period in response.rotation_plan.periods] == [
        "Monday",
        "Friday",
    ]
    assert response.editor.candidate_id == "period-1"
    assert response.editor.students[0].student_key == "PRIVATE-001"
    assert [editor.candidate_id for editor in response.period_editors] == [
        "period-1",
        "period-2",
    ]


def test_generation_error_is_structured_without_echoing_student_data() -> None:
    with pytest.raises(ApiProblem) as captured:
        generate_class(_request(seat_count=1))

    problem = captured.value
    response_text = problem.response().json()
    assert problem.status_code == 422
    assert problem.code == "class_not_ready"
    assert problem.details[0].code == "room_capacity"
    assert "PRIVATE-001" not in response_text
    assert "PRIVATE-002" not in response_text
    assert "Private Name" not in response_text


def test_unknown_template_and_goal_use_private_generic_errors() -> None:
    template_request = GenerateClassRequest.model_validate(
        {
            "draft": {
                "name": "Class A",
                "students": [{"student_id": "PRIVATE-001"}],
                "room": {"template_id": "private-room-name"},
                "goal": {"goal_id": "quick-shuffle"},
            }
        }
    )

    with pytest.raises(ApiProblem) as template_error:
        inspect_class_request(template_request)
    with pytest.raises(ApiProblem) as goal_error:
        inspect_class_request(_request(goal_id="private-goal-name"))

    assert template_error.value.code == "invalid_class_draft"
    assert "private-room-name" not in template_error.value.response().json()
    assert goal_error.value.code == "invalid_class_draft"
    assert "private-goal-name" not in goal_error.value.response().json()


def test_room_selection_requires_one_source() -> None:
    with pytest.raises(ValueError, match="either template_id or layout"):
        GenerateClassRequest.model_validate(
            {
                "draft": {
                    "name": "Class A",
                    "students": [{"student_id": "S1"}],
                    "room": {},
                }
            }
        )


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("candidate_count", True),
        ("seed", False),
        ("time_limit_seconds", True),
        ("time_limit_seconds", "0.2"),
        ("time_limit_seconds", float("inf")),
        ("time_limit_seconds", float("nan")),
        ("candidate_count", 1.5),
        ("seed", "11"),
    ],
)
def test_generate_options_reject_boolean_and_non_finite_numbers(
    field: str,
    value: object,
) -> None:
    payload = _request().model_dump()
    payload["options"][field] = value

    with pytest.raises(ValueError):
        GenerateClassRequest.model_validate(payload)


def test_fastapi_transport_is_loaded_only_when_requested(monkeypatch) -> None:
    original_import = builtins.__import__

    def guarded_import(name, globals=None, locals=None, fromlist=(), level=0):
        if name == "fastapi" or name.startswith("fastapi."):
            raise ImportError("FastAPI intentionally unavailable")
        return original_import(name, globals, locals, fromlist, level)

    # Core handlers and models have already imported successfully.  Only the
    # transport factory should require FastAPI.
    monkeypatch.setattr(builtins, "__import__", guarded_import)
    with pytest.raises(MissingOptionalDependencyError, match="Local Web API"):
        create_app()


def test_fastapi_routes_use_only_the_versioned_prefix_when_available() -> None:
    pytest.importorskip("fastapi")

    app = create_app()
    api_paths = {route.path for route in app.routes if route.path.startswith("/api/")}

    assert api_paths == {
        f"{API_PREFIX}/openapi.json",
        f"{API_PREFIX}/health",
        f"{API_PREFIX}/capabilities",
        f"{API_PREFIX}/room-templates",
        f"{API_PREFIX}/teacher-goals",
        f"{API_PREFIX}/catalogs",
        f"{API_PREFIX}/classes/inspect",
        f"{API_PREFIX}/classes/generate",
        f"{API_PREFIX}/classes/rotation",
            f"{API_PREFIX}/projects/recent",
            f"{API_PREFIX}/projects/history",
            f"{API_PREFIX}/projects/artifacts/compare",
            f"{API_PREFIX}/projects/artifacts/restore",
            f"{API_PREFIX}/projects/privacy",
        f"{API_PREFIX}/projects/bundle",
        f"{API_PREFIX}/projects/restore",
        f"{API_PREFIX}/editing/drafts/{{draft_id}}",
        f"{API_PREFIX}/editing/drafts/{{draft_id}}/commands",
        f"{API_PREFIX}/layouts/drafts",
        f"{API_PREFIX}/layouts/drafts/{{draft_id}}",
        f"{API_PREFIX}/layouts/drafts/{{draft_id}}/commands",
        f"{API_PREFIX}/layouts/drafts/{{draft_id}}/compiled",
        f"{API_PREFIX}/rosters/drafts",
        f"{API_PREFIX}/rosters/drafts/{{draft_id}}",
        f"{API_PREFIX}/rosters/drafts/{{draft_id}}/preview",
        f"{API_PREFIX}/exports",
    }
