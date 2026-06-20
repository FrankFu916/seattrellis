from __future__ import annotations

from seattrellis.models.layout import ClassroomLayout
from seattrellis.models.student import Student


def validate_capacity(students: list[Student], layout: ClassroomLayout) -> None:
    enabled_count = len(layout.enabled_seats)
    if len(students) > enabled_count:
        raise ValueError(f"{len(students)} students cannot fit into {enabled_count} enabled seats.")
