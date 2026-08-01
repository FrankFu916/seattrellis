"""Optional pywebview desktop shell for the bundled React workbench.

This is intentionally a thin launcher. The Python service, API contract, and
compiled client remain shared with the browser workbench so the desktop
prototype cannot grow a second implementation of classroom logic.
"""

from __future__ import annotations

import base64
import binascii
import json
import mimetypes
import os
import secrets
import socket
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from threading import Event, Thread
from time import monotonic, sleep
from urllib.parse import urlencode
from typing import Any

from seattrellis.api.security import LocalApiPolicy
from seattrellis.optional import MissingOptionalDependencyError
from seattrellis.workspace_server import create_workspace_app, resolve_workspace_assets


MAX_DESKTOP_ROSTER_BYTES = 20 * 1024 * 1024
MAX_DESKTOP_EXPORT_BYTES = 50 * 1024 * 1024
MAX_RECENT_FILES = 10
ROSTER_SUFFIXES = frozenset({".csv", ".xlsx", ".xls"})


class DesktopBridge:
    """Small, local-only API exposed to the React client by pywebview.

    The browser client still works without this object. Methods deliberately
    exchange base64 payloads instead of opening arbitrary paths in JavaScript,
    and recent-file metadata never contains student records or file contents.
    """

    def __init__(self, *, recent_file_path: Path | None = None) -> None:
        self._window: Any = None
        self._recent_file_path = recent_file_path or _default_recent_file_path()

    def attach_window(self, window: Any) -> None:
        """Attach the pywebview window after ``create_window`` returns."""

        self._window = window

    def open_roster_file(self) -> dict[str, str] | None:
        """Open a roster with the native picker and return a safe file payload."""

        selected = self._choose_file(
            dialog="OPEN",
            file_types=(
                "Roster files (*.csv;*.xlsx;*.xls)",
                "CSV files (*.csv)",
                "Excel files (*.xlsx;*.xls)",
            ),
        )
        if selected is None:
            return None
        path = _validated_roster_path(selected)
        data = _read_limited(path, MAX_DESKTOP_ROSTER_BYTES)
        self._remember_file(path)
        return {
            "name": path.name,
            "content_base64": base64.b64encode(data).decode("ascii"),
            "content_type": mimetypes.guess_type(path.name)[0] or "application/octet-stream",
        }

    def open_recent_file(self, path: str) -> dict[str, str] | None:
        """Open a previously selected roster without accepting arbitrary paths."""

        candidate = Path(path).expanduser()
        recent = {item.resolve() for item in self._load_recent_paths()}
        try:
            resolved = candidate.resolve(strict=True)
        except OSError:
            return None
        if resolved not in recent or resolved.suffix.casefold() not in ROSTER_SUFFIXES:
            return None
        data = _read_limited(resolved, MAX_DESKTOP_ROSTER_BYTES)
        self._remember_file(resolved)
        return {
            "name": resolved.name,
            "content_base64": base64.b64encode(data).decode("ascii"),
            "content_type": mimetypes.guess_type(resolved.name)[0] or "application/octet-stream",
        }

    def list_recent_files(self) -> list[dict[str, str]]:
        """Return recent roster names and paths, newest first."""

        result: list[dict[str, str]] = []
        for path in self._load_recent_paths():
            try:
                resolved = path.resolve(strict=True)
            except OSError:
                continue
            if resolved.suffix.casefold() in ROSTER_SUFFIXES:
                result.append({"name": resolved.name, "path": str(resolved)})
        return result

    def save_export_file(self, filename: str, content_base64: str) -> dict[str, Any]:
        """Save an export through the native picker and report the result."""

        safe_name = _safe_filename(filename, fallback="seating.html")
        data = _decode_limited(content_base64, MAX_DESKTOP_EXPORT_BYTES)
        selected = self._choose_file(
            dialog="SAVE",
            save_filename=safe_name,
            file_types=("SeatTrellis export (*.*)",),
        )
        if selected is None:
            return {"saved": False, "name": safe_name}
        destination = Path(selected).expanduser()
        if destination.exists() and destination.is_dir():
            raise ValueError("The selected export path is a directory.")
        destination.parent.mkdir(parents=True, exist_ok=True)
        _atomic_write_bytes(destination, data)
        return {"saved": True, "name": destination.name}

    def choose_project_folder(self) -> str | None:
        """Return a folder selected by the native picker for future project UI."""

        selected = self._choose_file(dialog="FOLDER")
        return str(Path(selected).expanduser()) if selected else None

    def _choose_file(
        self,
        *,
        dialog: str,
        file_types: tuple[str, ...] = (),
        save_filename: str = "",
    ) -> str | None:
        if self._window is None:
            raise RuntimeError("The desktop window is not ready.")
        try:
            import webview

            dialog_type = getattr(webview.FileDialog, dialog)
        except (ImportError, AttributeError) as exc:  # pragma: no cover - desktop extra.
            raise RuntimeError("Native file dialogs are not available.") from exc
        selected = self._window.create_file_dialog(
            dialog_type,
            allow_multiple=False,
            save_filename=save_filename,
            file_types=list(file_types),
        )
        if isinstance(selected, (list, tuple)):
            return str(selected[0]) if selected else None
        return str(selected) if selected else None

    def _remember_file(self, path: Path) -> None:
        resolved = path.resolve()
        paths = [item for item in self._load_recent_paths() if item.resolve() != resolved]
        paths.insert(0, resolved)
        self._write_recent_paths(paths[:MAX_RECENT_FILES])

    def _load_recent_paths(self) -> list[Path]:
        try:
            raw = json.loads(self._recent_file_path.read_text(encoding="utf-8"))
        except (FileNotFoundError, OSError, json.JSONDecodeError):
            return []
        if not isinstance(raw, list):
            return []
        return [Path(item) for item in raw if isinstance(item, str)][:MAX_RECENT_FILES]

    def _write_recent_paths(self, paths: list[Path]) -> None:
        try:
            self._recent_file_path.parent.mkdir(parents=True, exist_ok=True)
            payload = json.dumps([str(path) for path in paths], ensure_ascii=False, indent=2)
            _atomic_write_text(self._recent_file_path, payload + "\n")
        except OSError:
            # Recent files are a convenience; inability to persist them must
            # never prevent a roster from being opened or an export saved.
            return


def _default_recent_file_path() -> Path:
    if sys.platform == "darwin":
        root = Path.home() / "Library" / "Application Support"
    elif os.name == "nt":
        root = Path(os.environ.get("APPDATA", Path.home() / "AppData/Roaming"))
    else:
        root = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
    return root / "SeatTrellis" / "recent-files.json"


def _validated_roster_path(value: str) -> Path:
    path = Path(value).expanduser()
    if path.suffix.casefold() not in ROSTER_SUFFIXES:
        raise ValueError("Choose a CSV or Excel roster file.")
    try:
        return path.resolve(strict=True)
    except OSError as exc:
        raise ValueError("The selected roster file could not be opened.") from exc


def _read_limited(path: Path, limit: int) -> bytes:
    if path.stat().st_size > limit:
        raise ValueError("The selected file is too large to open safely.")
    return path.read_bytes()


def _decode_limited(value: str, limit: int) -> bytes:
    if not isinstance(value, str):
        raise TypeError("Export content must be base64 text.")
    try:
        data = base64.b64decode(value, validate=True)
    except (binascii.Error, ValueError) as exc:
        raise ValueError("Export content is not valid base64.") from exc
    if len(data) > limit:
        raise ValueError("The selected export is too large to save safely.")
    return data


def _safe_filename(value: str, *, fallback: str) -> str:
    candidate = Path(str(value)).name.strip()
    if not candidate or candidate in {".", ".."}:
        return fallback
    return candidate[:180]


def _atomic_write_bytes(path: Path, data: bytes) -> None:
    with tempfile.NamedTemporaryFile(
        mode="wb",
        dir=path.parent,
        prefix=f".{path.name}.",
        suffix=".tmp",
        delete=False,
    ) as handle:
        temporary = Path(handle.name)
        handle.write(data)
    try:
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def _atomic_write_text(path: Path, text: str) -> None:
    _atomic_write_bytes(path, text.encode("utf-8"))


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
    bridge = DesktopBridge()
    session.start()
    try:
        window = webview.create_window(
            resolved.title,
            session.url,
            width=resolved.width,
            height=resolved.height,
            js_api=bridge,
        )
        bridge.attach_window(window)
        webview.start()
    finally:
        session.stop()


def _find_free_port(host: str) -> int:
    family = socket.AF_INET6 if ":" in host else socket.AF_INET
    with socket.socket(family, socket.SOCK_STREAM) as listener:
        listener.bind((host, 0))
        return int(listener.getsockname()[1])
