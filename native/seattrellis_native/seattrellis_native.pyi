__version__: str
NATIVE_API_VERSION: int

def assignment_is_unique(
    student_count: int,
    seat_count: int,
    assignments: list[tuple[int, int]],
) -> bool: ...

def seat_distance(
    first_x: float,
    first_y: float,
    second_x: float,
    second_y: float,
) -> float: ...
