"""Pydantic data models used across SeatTrellis."""

from seattrellis.models.layout import AdjacencyConfig, ClassroomLayout, SeatNode
from seattrellis.models.history import (
    FairnessReport,
    NeighborRelationType,
    PairHistory,
    PairHistoryRecord,
    PairHistoryReport,
    SeatHistory,
    SeatHistoryRecord,
    SeatPositionCategory,
    StudentPairHistory,
    StudentSeatHistory,
)
from seattrellis.models.rules import (
    AvoidRecentNeighborsRule,
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
    "AvoidRecentNeighborsRule",
    "FairRotationRule",
    "FairnessReport",
    "FixedSeatRule",
    "HardRules",
    "MinDistanceRule",
    "NeighborRelationType",
    "PairHistory",
    "PairHistoryRecord",
    "PairHistoryReport",
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
    "StudentPairHistory",
    "StudentSeatHistory",
    "WeightedRule",
]
