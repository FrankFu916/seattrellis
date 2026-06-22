"""Pydantic data models used across SeatTrellis."""

from seattrellis.models.layout import AdjacencyConfig, ClassroomLayout, SeatNode
from seattrellis.models.history import (
    FairnessReport,
    SeatHistory,
    SeatHistoryRecord,
    SeatPositionCategory,
    StudentSeatHistory,
)
from seattrellis.models.rules import (
    FairRotationRule,
    FixedSeatRule,
    HardRules,
    MinDistanceRule,
    PairRule,
    RuleSet,
    SoftRules,
    WeightedRule,
)
from seattrellis.models.snapshot import SeatAssignment, SeatingSnapshot
from seattrellis.models.student import Student

__all__ = [
    "AdjacencyConfig",
    "ClassroomLayout",
    "FairRotationRule",
    "FairnessReport",
    "FixedSeatRule",
    "HardRules",
    "MinDistanceRule",
    "PairRule",
    "RuleSet",
    "SeatAssignment",
    "SeatHistory",
    "SeatHistoryRecord",
    "SeatNode",
    "SeatPositionCategory",
    "SeatingSnapshot",
    "SoftRules",
    "Student",
    "StudentSeatHistory",
    "WeightedRule",
]
