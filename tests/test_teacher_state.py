from __future__ import annotations

from dataclasses import fields

import pytest

from seattrellis.web.teacher_state import (
    TeacherPrimaryActionId,
    TeacherWorkspaceStage,
    build_teacher_workspace_state,
    primary_action_for_stage,
    recovery_prompt_for_stage,
)


@pytest.mark.parametrize(
    ("facts", "expected"),
    [
        ({}, TeacherWorkspaceStage.EMPTY),
        ({"has_roster": True}, TeacherWorkspaceStage.ROSTER_READY),
        (
            {"has_roster": True, "has_room": True},
            TeacherWorkspaceStage.ROOM_READY,
        ),
        (
            {"has_roster": True, "has_room": True, "has_goal": True},
            TeacherWorkspaceStage.READY,
        ),
        (
            {
                "has_roster": True,
                "has_room": True,
                "has_goal": True,
                "has_plan": True,
            },
            TeacherWorkspaceStage.PLAN_READY,
        ),
        (
            {
                "has_roster": True,
                "has_room": True,
                "has_goal": True,
                "has_plan": True,
                "is_editing": True,
            },
            TeacherWorkspaceStage.EDITING,
        ),
        (
            {
                "has_roster": True,
                "has_room": True,
                "has_goal": True,
                "has_plan": True,
                "is_saved": True,
            },
            TeacherWorkspaceStage.SAVED,
        ),
    ],
)
def test_workspace_stage_follows_the_teacher_workflow(facts, expected) -> None:
    assert build_teacher_workspace_state(**facts).stage is expected


def test_each_stage_has_one_stable_primary_action() -> None:
    expected = {
        TeacherWorkspaceStage.EMPTY: TeacherPrimaryActionId.IMPORT_ROSTER,
        TeacherWorkspaceStage.ROSTER_READY: TeacherPrimaryActionId.CONFIGURE_ROOM,
        TeacherWorkspaceStage.ROOM_READY: TeacherPrimaryActionId.CHOOSE_GOAL,
        TeacherWorkspaceStage.READY: TeacherPrimaryActionId.GENERATE_PLAN,
        TeacherWorkspaceStage.PLAN_READY: TeacherPrimaryActionId.ADJUST_PLAN,
        TeacherWorkspaceStage.EDITING: TeacherPrimaryActionId.SAVE_PLAN,
        TeacherWorkspaceStage.SAVED: TeacherPrimaryActionId.EXPORT_PLAN,
    }

    assert {
        stage: primary_action_for_stage(stage).action_id
        for stage in TeacherWorkspaceStage
    } == expected


def test_action_and_recovery_copy_support_chinese_and_english() -> None:
    chinese = build_teacher_workspace_state(
        has_roster=True,
        has_room=True,
        locale="zh-CN",
    )
    english = build_teacher_workspace_state(
        has_roster=True,
        has_room=True,
        locale="en-US",
    )

    assert chinese.primary_action.label == "选择排座目标"
    assert "学生名单和教室" in (chinese.recovery_prompt or "")
    assert english.primary_action.label == "Choose seating goal"
    assert "student list and classroom" in (english.recovery_prompt or "")


def test_empty_workspace_has_no_misleading_recovery_prompt() -> None:
    state = build_teacher_workspace_state(locale="en")

    assert state.recovery_prompt is None
    assert recovery_prompt_for_stage("empty", locale="zh") is None


@pytest.mark.parametrize(
    ("facts", "message"),
    [
        ({"has_room": True}, "has_room requires has_roster"),
        ({"has_goal": True}, "has_goal requires has_room"),
        ({"has_plan": True}, "has_plan requires has_goal"),
        ({"is_editing": True}, "is_editing requires has_plan"),
        ({"is_saved": True}, "is_saved requires has_plan"),
        (
            {
                "has_roster": True,
                "has_room": True,
                "has_goal": True,
                "has_plan": True,
                "is_editing": True,
                "is_saved": True,
            },
            "cannot both be true",
        ),
    ],
)
def test_partial_or_ambiguous_restore_state_is_rejected(facts, message) -> None:
    with pytest.raises(ValueError, match=message):
        build_teacher_workspace_state(**facts)


def test_view_state_does_not_store_domain_objects_or_readiness_copies() -> None:
    state = build_teacher_workspace_state(has_roster=True, locale="en")

    assert [item.name for item in fields(state)] == [
        "stage",
        "primary_action",
        "recovery_prompt",
    ]
    assert state == build_teacher_workspace_state(has_roster=True, locale="en")


def test_stage_aliases_are_normalized_for_adapters() -> None:
    action = primary_action_for_stage("plan_ready", locale="en-GB")

    assert action.action_id is TeacherPrimaryActionId.ADJUST_PLAN


@pytest.mark.parametrize(
    ("call", "message"),
    [
        (
            lambda: build_teacher_workspace_state(has_roster=1),
            "has_roster must be a boolean",
        ),
        (
            lambda: primary_action_for_stage("missing"),
            "Unknown teacher workspace stage",
        ),
        (
            lambda: recovery_prompt_for_stage("saved", locale="fr"),
            "Unsupported teacher workflow locale",
        ),
    ],
)
def test_invalid_adapter_inputs_fail_with_context(call, message) -> None:
    with pytest.raises((TypeError, ValueError), match=message):
        call()
