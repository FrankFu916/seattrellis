"""Optional pywebview desktop shell for the bundled React workbench.

This is intentionally a thin launcher. The Python service, API contract, and
compiled client remain shared with the browser workbench so the desktop
prototype cannot grow a second implementation of classroom logic.
"""

from __future__ import annotations

import secrets
import socket
from dataclasses import dataclass
from threading import Event, Thread
from time import monotonic, sleep
from urllib.parse import urlencode
from typing import Any

from seattrellis.api.security import LocalApiPolicy
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.workspace_server import create_workspace_app, resolve_workspace_assets


@dataclass(frozen=True, slots=True)
class DesktopOptions:
    """Validated settings for the local desktop prototype."""

    host: str = "127.0.0.1"
    port: int = 0
    width: int = 1280
    height: int = 900
    title: str = "SeatTrellis"
    startup_timeout_seconds: float = 10.0

    def __post_init__(self) -> None:
        if self.host.strip().lower() not in {"127.0.0.1", "localhost", "::1"}:
            raise ValueError("The desktop app only accepts a loopback host.")
        if not 0 <= self.port <= 65535:
            raise ValueError("Desktop port must be between 0 and 65535.")
        if not 640 <= self.width <= 4096 or not 480 <= self.height <= 4096:
            raise ValueError("Desktop window dimensions are outside the supported range.")
        if not self.title.strip():
            raise ValueError("Desktop window title cannot be empty.")
        if self.startup_timeout_seconds < 1:
            raise ValueError("Desktop startup timeout must be at least one second.")


class DesktopSession:
    """Own the local API process and its unpredictable browser credential."""

    def __init__(
        self,
        options: DesktopOptions | None = None,
        *,
        static_dir: str | None = None,
    ) -> None:
        self.options = options or DesktopOptions()
        self.static_dir = static_dir
        self.session_token = secrets.token_urlsafe(32)
        self.port: int | None = None
        self._server: Any = None
        self._thread: Thread | None = None
        self._started = Event()

    @property
    def url(self) -> str:
        if self.port is None:
            raise RuntimeError("Desktop session has not started.")
        host = f"[{self.options.host}]" if self.options.host == "::1" else self.options.host
        return f"http://{host}:{self.port}/?{urlencode({'session': self.session_token})}"

    def start(self) -> str:
        """Start the loopback service and return the authenticated workbench URL."""

        try:
            import uvicorn
        except ImportError as exc:  # pragma: no cover - optional installation.
            raise MissingOptionalDependencyError(
                "Desktop workbench",
                "desktop",
                detail="Install SeatTrellis with the desktop extra to use pywebview.",
            ) from exc

        assets = resolve_workspace_assets(self.static_dir)
        self.port = self.options.port or _find_free_port(self.options.host)
        policy = LocalApiPolicy(session_token=self.session_token)
        app = create_workspace_app(static_dir=assets, policy=policy)
        self._server = uvicorn.Server(
            uvicorn.Config(
                app,
                host=self.options.host,
                port=self.port,
                log_level="warning",
                access_log=False,
                server_header=False,
            )
        )
        self._thread = Thread(target=self._run_server, name="seattrellis-desktop-api", daemon=True)
        self._thread.start()
        deadline = monotonic() + self.options.startup_timeout_seconds
        while not getattr(self._server, "started", False):
            if not self._thread.is_alive():
                raise RuntimeError("The local desktop API stopped during startup.")
            if monotonic() >= deadline:
                self.stop()
                raise RuntimeError("The local desktop API did not start in time.")
            sleep(0.02)
        self._started.set()
        return self.url

    def stop(self) -> None:
        """Request a clean server shutdown and wait for its thread."""

        if self._server is not None:
            self._server.should_exit = True
        if self._thread is not None and self._thread.is_alive():
            self._thread.join(timeout=5)
            if self._thread.is_alive() and self._server is not None:
                self._server.force_exit = True
                self._thread.join(timeout=2)
        self._started.clear()
        self._thread = None
        self._server = None
        self.port = None

    def _run_server(self) -> None:
        self._server.run()


def run_desktop_app(
    *,
    options: DesktopOptions | None = None,
    static_dir: str | None = None,
) -> None:
    """Open the optional desktop shell and always clean up the local service."""

    try:
        import webview
    except ImportError as exc:  # pragma: no cover - optional installation.
        raise MissingOptionalDependencyError(
            "Desktop workbench",
            "desktop",
            detail="Install SeatTrellis with the desktop extra to use pywebview.",
        ) from exc

    resolved = options or DesktopOptions()
    session = DesktopSession(resolved, static_dir=static_dir)
    session.start()
    try:
        webview.create_window(
            resolved.title,
            session.url,
            width=resolved.width,
            height=resolved.height,
        )
        webview.start()
    finally:
        session.stop()


def _find_free_port(host: str) -> int:
    family = socket.AF_INET6 if ":" in host else socket.AF_INET
    with socket.socket(family, socket.SOCK_STREAM) as listener:
        listener.bind((host, 0))
        return int(listener.getsockname()[1])
