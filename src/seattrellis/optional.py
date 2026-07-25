from __future__ import annotations


class MissingOptionalDependencyError(RuntimeError):
    """Raised when an optional feature is unavailable in this installation."""

    def __init__(
        self,
        feature: str,
        extra: str | None,
        *,
        detail: str | None = None,
    ) -> None:
        self.feature = feature
        self.extra = extra
        if extra is None:
            message = f"{feature} is not available in this installation."
        else:
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
