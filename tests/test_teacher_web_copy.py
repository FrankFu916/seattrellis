from __future__ import annotations

from seattrellis.web import keys as web_keys
from seattrellis.web.i18n import available_translation_keys, translate


TEACHER_TRANSLATION_KEYS = {
    "workspace_choice",
    "workspace_teacher",
    "workspace_advanced",
    "teacher_home_title",
    "teacher_home_caption",
    "teacher_home_status_ready",
    "teacher_class_name",
    "teacher_class_name_placeholder",
    "teacher_roster_title",
    "teacher_roster_upload",
    "teacher_roster_help",
    "teacher_roster_summary",
    "teacher_roster_name_only",
    "teacher_roster_ready",
    "teacher_room_title",
    "teacher_room_template",
    "teacher_room_recommended",
    "teacher_room_summary",
    "teacher_room_too_small",
    "teacher_goal_title",
    "teacher_goal_help",
    "teacher_goal_daily_title",
    "teacher_goal_daily_description",
    "teacher_goal_fair_title",
    "teacher_goal_fair_description",
    "teacher_goal_peer_title",
    "teacher_goal_peer_description",
    "teacher_generate",
    "teacher_generating",
    "teacher_generate_success",
    "teacher_generate_failed",
    "teacher_results_title",
    "teacher_results_summary",
    "teacher_other_candidates",
    "teacher_candidate_choice",
    "teacher_export_title",
    "teacher_public_print",
    "teacher_public_print_help",
    "teacher_internal_print",
    "teacher_internal_print_help",
    "teacher_export_ready",
    "teacher_restore_notice",
    "teacher_restore_failed",
    "teacher_error_title",
    "teacher_error_detail",
}

FORMAT_VALUES = {
    "students": 42,
    "scores": 40,
    "heights": 38,
    "front_needs": 3,
    "special_needs": 2,
    "count": 3,
    "capacity": 48,
    "rows": 6,
    "label": "PDF",
    "error": "example",
}


def test_stable_widget_keys_are_unique() -> None:
    constants = {
        name: value
        for name, value in vars(web_keys).items()
        if name.isupper() and isinstance(value, str)
    }

    assert len(constants.values()) == len(set(constants.values()))


def test_teacher_workspace_copy_is_available_in_both_languages() -> None:
    assert TEACHER_TRANSLATION_KEYS <= available_translation_keys()

    for key in TEACHER_TRANSLATION_KEYS:
        chinese = translate(key, "zh", **FORMAT_VALUES)
        english = translate(key, "en", **FORMAT_VALUES)
        assert chinese.strip()
        assert english.strip()
        assert chinese != english


def test_default_teacher_copy_uses_plain_product_language() -> None:
    copy = " ".join(
        translate(key, locale, **FORMAT_VALUES)
        for key in TEACHER_TRANSLATION_KEYS
        for locale in ("zh", "en")
    ).lower()

    for technical_term in ("backend", "preset", "seed", "schema"):
        assert technical_term not in copy

    peer_copy = " ".join(
        translate("teacher_goal_peer_description", locale)
        for locale in ("zh", "en")
    ).lower()
    assert "邻座" in peer_copy
    assert "neighboring seats" in peer_copy
    assert "小组均衡" not in peer_copy
    assert "group balance" not in peer_copy
