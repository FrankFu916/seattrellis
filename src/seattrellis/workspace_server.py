"""Local server for the browser workbench.

The React client is built before packaging and served from the same loopback
origin as the versioned API.  Keeping both sides on one origin avoids broad
CORS permissions and gives the future desktop shell a stable application
boundary to launch.
"""

from __future__ import annotations

import webbrowser
from dataclasses import dataclass
from pathlib import Path
from threading import Timer
from typing import Any

from seattrellis.api.http import create_app
from seattrellis.api.security import LocalApiPolicy
from seattrellis.optional import MissingOptionalDependencyError


DEFAULT_WORKSPACE_HOST = "127.0.0.1"
DEFAULT_WORKSPACE_PORT = 8765
_LOOPBACK_HOSTS = frozenset({"127.0.0.1", "localhost", "::1"})


@dataclass(frozen=True, slots=True)
class WorkspaceServerOptions:
    """Validated settings for the local browser workbench."""

    host: str = DEFAULT_WORKSPACE_HOST
    port: int = DEFAULT_WORKSPACE_PORT
    open_browser: bool = True

    def __post_init__(self) -> None:
        host = self.host.strip().lower()
        if host not in _LOOPBACK_HOSTS:
            raise ValueError("The workspace server only accepts a loopback host.")
        if not 1 <= self.port <= 65535:
            raise ValueError("Workspace port must be between 1 and 65535.")
        object.__setattr__(self, "host", host)

    @property
    def browser_url(self) -> str:
        host = f"[{self.host}]" if self.host == "::1" else self.host
        return f"http://{host}:{self.port}/"


def resolve_workspace_assets(static_dir: str | Path | None = None) -> Path:
    """Locate a complete workbench build without accepting partial assets."""

    candidates = []
    if static_dir is not None:
        candidates.append(Path(static_dir))
    else:
        package_root = Path(__file__).resolve().parent
        candidates.extend(
            [
                package_root / "web_static",
                package_root.parent.parent / "clients" / "web" / "dist",
            ]
        )

    for candidate in candidates:
        resolved = candidate.expanduser().resolve()
        if resolved.is_dir() and (resolved / "index.html").is_file():
            return resolved

    if static_dir is not None:
        raise ValueError(
            "The selected Web asset directory does not contain index.html."
        )
    raise MissingOptionalDependencyError(
        "Browser workbench",
        "web",
        detail=(
            "This source checkout does not contain a built Web client. "
            "Run the Web client build or install an official SeatTrellis package."
        ),
    )


def create_workspace_app(
    *,
    static_dir: str | Path | None = None,
    policy: LocalApiPolicy | None = None,
) -> Any:
    """Create the same-origin API and static workbench application."""

    try:
        from fastapi.staticfiles import StaticFiles
    except ImportError as exc:  # pragma: no cover - optional installation.
        raise MissingOptionalDependencyError(
            "Browser workbench",
            "web",
            detail="Install the Web optional dependencies to start the workbench.",
        ) from exc

    assets = resolve_workspace_assets(static_dir)
    app = create_app(policy=policy)
    # Register after the API routes so the static application cannot shadow
    # the versioned contract.
    app.mount("/", StaticFiles(directory=str(assets), html=True), name="workbench")
    return app


def run_workspace_server(
    *,
    options: WorkspaceServerOptions | None = None,
    static_dir: str | Path | None = None,
) -> None:
    """Start the local workbench and optionally open the default browser."""

    resolved = options or WorkspaceServerOptions()
    try:
        import uvicorn
    except ImportError as exc:  # pragma: no cover - optional installation.
        raise MissingOptionalDependencyError(
            "Browser workbench",
            "web",
            detail="Install the Web optional dependencies to start the workbench.",
        ) from exc

    app = create_workspace_app(static_dir=static_dir)
    if resolved.open_browser:
        # Uvicorn owns the foreground thread.  A short daemon timer lets the
        # listener start before the browser requests the first page.
        timer = Timer(0.6, webbrowser.open, args=(resolved.browser_url,))
        timer.daemon = True
        timer.start()
    uvicorn.run(
        app,
        host=resolved.host,
        port=resolved.port,
        access_log=False,
        server_header=False,
    )
