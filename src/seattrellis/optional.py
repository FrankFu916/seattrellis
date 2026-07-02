from __future__ import annotations


class MissingOptionalDependencyError(RuntimeError):
    """Raised when an optional feature is used without its extra installed."""

    def __init__(
        self,
        feature: str,
        extra: str,
        *,
        detail: str | None = None,
    ) -> None:
        self.feature = feature
        self.extra = extra
        message = (
            f"{feature} requires the {extra} extra.\n"
            "Please install it with:\n"
            f'  python -m pip install "seattrellis[{extra}]"\n'
            "or, for local development:\n"
            f'  python -m pip install -e ".[{extra}]"'
        )
        if detail:
            message = f"{message}\n\n{detail}"
        super().__init__(message)
