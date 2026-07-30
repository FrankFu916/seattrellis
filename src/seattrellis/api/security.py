"""Local transport security boundary for the Web API.

This is intentionally independent of FastAPI so desktop and test adapters can
enforce the same Host, Origin, and future session-token rules.
"""

from __future__ import annotations

from dataclasses import dataclass
from hmac import compare_digest
from urllib.parse import urlsplit

from seattrellis.api.errors import ApiProblem


@dataclass(frozen=True, slots=True)
class LocalApiPolicy:
    """Allow loopback hosts and only explicitly trusted browser origins."""

    allowed_hosts: tuple[str, ...] = ("127.0.0.1", "localhost", "::1")
    allowed_origins: tuple[str, ...] = ()
    session_token: str | None = None

    def __post_init__(self) -> None:
        normalized_hosts = tuple(
            dict.fromkeys(host.strip().lower() for host in self.allowed_hosts if host.strip())
        )
        normalized_origins = tuple(
            dict.fromkeys(origin.rstrip("/") for origin in self.allowed_origins if origin)
        )
        if not normalized_hosts or "*" in normalized_hosts:
            raise ValueError("allowed_hosts must be explicit local host names.")
        if "*" in normalized_origins:
            raise ValueError("allowed_origins cannot contain a wildcard.")
        if any(not _valid_configured_origin(origin) for origin in normalized_origins):
            raise ValueError(
                "allowed_origins must contain exact HTTP or HTTPS origins."
            )
        if self.session_token is not None and not self.session_token:
            raise ValueError("session_token cannot be empty.")
        object.__setattr__(self, "allowed_hosts", normalized_hosts)
        object.__setattr__(self, "allowed_origins", normalized_origins)

    def validate(
        self,
        *,
        host_header: str | None,
        origin_header: str | None = None,
        authorization_header: str | None = None,
        request_scheme: str = "http",
        require_session: bool = True,
    ) -> None:
        """Validate request metadata without retaining headers or credentials."""

        hostname, _port = _parse_host_header(host_header)
        if hostname not in self.allowed_hosts:
            raise ApiProblem(
                status_code=403,
                code="local_access_required",
                message="This API only accepts requests through its local address.",
            )

        if origin_header and not self._origin_is_allowed(
            origin_header,
            host_header or "",
            request_scheme,
        ):
            raise ApiProblem(
                status_code=403,
                code="origin_not_allowed",
                message="This browser origin is not allowed to use the local API.",
            )

        if self.session_token is not None and require_session:
            expected = f"Bearer {self.session_token}"
            supplied = authorization_header or ""
            if not compare_digest(supplied, expected):
                raise ApiProblem(
                    status_code=401,
                    code="session_required",
                    message="A valid local application session is required.",
                )

    def _origin_is_allowed(
        self,
        origin: str,
        host_header: str,
        request_scheme: str,
    ) -> bool:
        normalized = origin.rstrip("/")
        if normalized in self.allowed_origins:
            return True
        try:
            parsed = urlsplit(normalized)
            if (
                parsed.scheme not in {"http", "https"}
                or parsed.scheme != request_scheme
                or not parsed.hostname
                or parsed.username is not None
                or parsed.password is not None
                or parsed.path not in {"", "/"}
                or parsed.query
                or parsed.fragment
            ):
                return False
            host_name, host_port = _parse_host_header(host_header)
            return (
                parsed.hostname.lower() == host_name
                and _origin_port(parsed.scheme, parsed.port) == host_port
            )
        except ValueError:
            return False


def _parse_host_header(value: str | None) -> tuple[str, int]:
    if not value:
        return "", -1
    if any(character in value for character in ("@", "/", "\\", "?", "#")):
        return "", -1
    if any(character.isspace() for character in value):
        return "", -1
    try:
        parsed = urlsplit(f"//{value}")
        if parsed.username is not None or parsed.password is not None or parsed.path:
            return "", -1
        return (parsed.hostname or "").lower(), parsed.port or 80
    except ValueError:
        return "", -1


def _origin_port(scheme: str, port: int | None) -> int:
    if port is not None:
        return port
    return 443 if scheme == "https" else 80


def _valid_configured_origin(origin: str) -> bool:
    try:
        parsed = urlsplit(origin)
        # Accessing ``port`` validates malformed values such as ``:abc``.
        parsed.port
    except ValueError:
        return False
    return bool(
        parsed.scheme in {"http", "https"}
        and parsed.hostname
        and parsed.username is None
        and parsed.password is None
        and parsed.path in {"", "/"}
        and not parsed.query
        and not parsed.fragment
    )
