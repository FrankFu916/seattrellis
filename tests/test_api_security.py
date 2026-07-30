from __future__ import annotations

import pytest

from seattrellis.api.errors import ApiProblem
from seattrellis.api.security import LocalApiPolicy


@pytest.mark.parametrize(
    "host_header",
    ["localhost:8765", "127.0.0.1:8765", "[::1]:8765"],
)
def test_default_policy_accepts_loopback_hosts(host_header: str) -> None:
    LocalApiPolicy().validate(host_header=host_header)


@pytest.mark.parametrize(
    "host_header",
    [
        None,
        "",
        "192.0.2.10:8765",
        "example.com",
        "user@localhost:8765",
        "localhost:8765/path",
    ],
)
def test_default_policy_rejects_non_local_hosts(host_header: str | None) -> None:
    with pytest.raises(ApiProblem) as captured:
        LocalApiPolicy().validate(host_header=host_header)

    assert captured.value.status_code == 403
    assert captured.value.code == "local_access_required"


def test_policy_accepts_same_origin_and_rejects_cross_origin_browser_requests() -> None:
    policy = LocalApiPolicy()

    policy.validate(
        host_header="localhost:8765",
        origin_header="http://localhost:8765",
    )
    with pytest.raises(ApiProblem) as captured:
        policy.validate(
            host_header="localhost:8765",
            origin_header="https://example.com",
        )

    assert captured.value.code == "origin_not_allowed"


def test_policy_rejects_a_malformed_origin_port_without_crashing() -> None:
    with pytest.raises(ApiProblem) as captured:
        LocalApiPolicy().validate(
            host_header="localhost:8765",
            origin_header="http://localhost:not-a-port",
        )

    assert captured.value.code == "origin_not_allowed"


def test_same_host_with_a_different_scheme_is_not_same_origin() -> None:
    with pytest.raises(ApiProblem) as captured:
        LocalApiPolicy().validate(
            host_header="localhost:8765",
            origin_header="https://localhost:8765",
            request_scheme="http",
        )

    assert captured.value.code == "origin_not_allowed"


def test_development_origin_requires_an_explicit_exact_allowlist() -> None:
    policy = LocalApiPolicy(allowed_origins=("http://localhost:5173/",))

    policy.validate(
        host_header="127.0.0.1:8765",
        origin_header="http://localhost:5173",
    )
    with pytest.raises(ApiProblem, match="browser origin"):
        policy.validate(
            host_header="127.0.0.1:8765",
            origin_header="http://localhost:5174",
        )


def test_policy_rejects_wildcards() -> None:
    with pytest.raises(ValueError, match="explicit local host"):
        LocalApiPolicy(allowed_hosts=("*",))
    with pytest.raises(ValueError, match="wildcard"):
        LocalApiPolicy(allowed_origins=("*",))
    with pytest.raises(ValueError, match="exact HTTP or HTTPS"):
        LocalApiPolicy(allowed_origins=("localhost:5173",))
    with pytest.raises(ValueError, match="exact HTTP or HTTPS"):
        LocalApiPolicy(allowed_origins=("http://localhost:5173/path",))


def test_optional_session_token_boundary_uses_bearer_authentication() -> None:
    policy = LocalApiPolicy(session_token="private-session-token")

    with pytest.raises(ApiProblem) as missing:
        policy.validate(host_header="localhost:8765")
    with pytest.raises(ApiProblem) as wrong:
        policy.validate(
            host_header="localhost:8765",
            authorization_header="Bearer wrong-token",
        )
    policy.validate(
        host_header="localhost:8765",
        authorization_header="Bearer private-session-token",
    )

    assert missing.value.status_code == 401
    assert missing.value.code == "session_required"
    assert wrong.value.code == "session_required"
    assert "private-session-token" not in wrong.value.response().json()


def test_browser_preflight_can_validate_origin_before_session_authentication() -> None:
    policy = LocalApiPolicy(
        allowed_origins=("http://localhost:5173",),
        session_token="private-session-token",
    )

    policy.validate(
        host_header="127.0.0.1:8765",
        origin_header="http://localhost:5173",
        require_session=False,
    )
