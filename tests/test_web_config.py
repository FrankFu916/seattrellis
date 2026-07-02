from __future__ import annotations

import json

import pytest

from seattrellis.io.json_files import InputFileError
from seattrellis.web.config import (
    WebSessionConfig,
    dump_web_config,
    load_web_config,
)


def test_web_config_round_trip_preserves_settings_and_rules() -> None:
    config = WebSessionConfig(
        preset_name="daily",
        rules_overlay={
            "hard": {
                "fixed_seats": [{"student": "学生甲", "seat_id": "R1C1"}]
            }
        },
        candidate_count=5,
        seed=123,
        time_limit_seconds=4.5,
    )

    loaded = load_web_config(dump_web_config(config))

    assert loaded == config
    assert loaded.contains_student_references is True
    assert dump_web_config(config).endswith(b"\n")


def test_web_config_without_student_rules_is_not_marked_sensitive() -> None:
    config = WebSessionConfig(
        preset_name="daily",
        rules_overlay={"soft": {"randomize": {"enabled": False}}},
    )

    assert config.contains_student_references is False


@pytest.mark.parametrize(
    "payload",
    [
        b"not json",
        b"[]",
        json.dumps(
            {
                "kind": "seattrellis_web_config",
                "schema_version": 2,
            }
        ).encode(),
        json.dumps(
            {
                "kind": "seattrellis_web_config",
                "schema_version": 1,
                "candidate_count": 0,
            }
        ).encode(),
        json.dumps(
            {
                "kind": "seattrellis_web_config",
                "schema_version": 1,
                "students": ["private"],
            }
        ).encode(),
    ],
)
def test_web_config_rejects_invalid_or_private_fields(payload: bytes) -> None:
    with pytest.raises(InputFileError):
        load_web_config(payload)
