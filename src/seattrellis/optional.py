from __future__ import annotations


class MissingOptionalDependencyError(RuntimeError):
    """Raised when an optional feature is used without its extra installed."""

    def __init__(self, feature: str, extra: str) -> None:
        self.feature = feature
        self.extra = extra
        super().__init__(
            f"{feature} requires the {extra} extra.\n"
            "Please install it with:\n"
            f'  python -m pip install "seattrellis[{extra}]"\n'
            "or, for local development:\n"
            f'  python -m pip install -e ".[{extra}]"'
        )
