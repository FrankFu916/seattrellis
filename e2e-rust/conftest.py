"""Browser fixtures for the NO_PYTHON_RUNTIME workbench E2E (M2 §5.7 item 2).

The server under test is the compiled Rust binary (`seattrellis_web`); the web
root is the compiled React workbench. No Python process participates in
serving or solving. The job that runs these tests must not install the Python
package at all - that absence, plus the binary checks below, is the evidence
that the whole import -> solve/candidates -> edit/repair -> save/rotation ->
export -> reopen workflow runs on the Rust runtime only.
"""

from __future__ import annotations

import json
import os
import re
import signal
import socket
import subprocess
import time
import urllib.request
from dataclasses import dataclass
from pathlib import Path

import pytest

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class RustServer:
    """A running `seattrellis_web` process used by a browser test session."""

    url: str
    health_url: str
    process: subprocess.Popen[str]
    log_path: Path
    binary_path: Path

    def assert_healthy(self) -> None:
        """Fail when the server exited or stopped responding."""
        return_code = self.process.poll()
        if return_code is not None:
            pytest.fail(
                f"seattrellis_web exited with code {return_code}.\n"
                f"{_log_tail(self.log_path)}"
            )
        try:
            _health_check(self.health_url)
        except OSError as exc:
            pytest.fail(
                f"Rust server health check failed: {exc}\n"
                f"{_log_tail(self.log_path)}"
            )

    def assert_native_binary(self) -> None:
        """Prove the serving process is the Rust binary, not a Python script
        or interpreter (the NO_PYTHON_RUNTIME part of the gate)."""
        pid = self.process.pid
        exe = _process_executable(pid)
        exe_name = Path(exe).name if exe else ""
        assert exe, f"cannot resolve executable of pid {pid}"
        assert "seattrellis_web" in exe_name, (
            f"expected the Rust seattrellis_web binary, got {exe!r}"
        )
        assert "python" not in exe_name.lower(), (
            f"server process must not be a Python interpreter, got {exe!r}"
        )
        # The binary must be a native executable, not a script.
        assert _is_native_executable(self.binary_path), (
            f"server binary is not a native executable: {self.binary_path}"
        )


def _process_executable(pid: int) -> str | None:
    """Best-effort resolution of the executable path of a process."""
    if os.name == "posix":
        try:
            return os.readlink(f"/proc/{pid}/exe")
        except OSError:
            try:
                out = subprocess.run(
                    ["ps", "-p", str(pid), "-o", "comm="],
                    check=False,
                    capture_output=True,
                    text=True,
                )
                return out.stdout.strip() or None
            except OSError:
                return None
    return None


def _is_native_executable(path: Path) -> bool:
    """ELF or Mach-O header check, plus a sanity rejection of shebang text."""
    if not path.is_file():
        return False
    with path.open("rb") as handle:
        head = handle.read(4)
    if head == b"\x7fELF" or head[:4] == b"\xcf\xfa\xed\xfe":
        return True
    try:
        first_line = path.read_text(encoding="utf-8", errors="ignore").splitlines()[0]
        return not first_line.startswith("#!")
    except OSError:
        return False


def _open_without_proxy(url: str, *, timeout: float, headers: dict[str, str] | None = None) -> bytes:
    request = urllib.request.Request(url, headers=headers or {})
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(request, timeout=timeout) as response:
        return response.read()


def _health_check(health_url: str) -> None:
    """Bootstrap the Bearer token (session endpoint) and health-check with
    it, mirroring the workbench: `/api/*` requires the token (M1-05)."""
    session_url = health_url.replace("/api/v1/health", "/api/v1/session")
    body = _open_without_proxy(session_url, timeout=2)
    token = json.loads(body).get("session_token", "")
    if not token:
        raise OSError("session bootstrap issued no token")
    response = _open_without_proxy(
        health_url, timeout=2, headers={"Authorization": f"Bearer {token}"}
    )
    if b'"ok"' not in response:
        raise OSError(f"unexpected health body: {response[:120]!r}")


def _reserve_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def _log_tail(path: Path, *, line_count: int = 80) -> str:
    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return "No server log is available."
    return "\n".join(lines[-line_count:])


def _stop_process(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    try:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGTERM)
        else:
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T"],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        process.wait(timeout=5)


def _locate_server_binary() -> Path:
    override = os.environ.get("SEATTRELLIS_E2E_RUST_SERVER")
    if override:
        path = Path(override)
        if path.is_file():
            return path
        pytest.fail(f"SEATTRELLIS_E2E_RUST_SERVER does not exist: {override}")
    for candidate in (
        REPOSITORY_ROOT / "target" / "debug" / "seattrellis_web",
        REPOSITORY_ROOT / "target" / "release" / "seattrellis_web",
    ):
        if candidate.is_file():
            return candidate
    pytest.fail(
        "no seattrellis_web binary found; build it with "
        "`cargo build -p seattrellis_web` or set SEATTRELLIS_E2E_RUST_SERVER"
    )


def _locate_web_root() -> Path:
    override = os.environ.get("SEATTRELLIS_WEB_STATIC")
    candidates = [
        Path(override) if override else None,
        REPOSITORY_ROOT / "clients" / "web" / "dist",
    ]
    for candidate in candidates:
        if candidate is not None and (candidate / "index.html").is_file():
            return candidate
    pytest.fail(
        "no compiled workbench found; build the React client "
        "(`npm ci && npm run build` in clients/web) or set SEATTRELLIS_WEB_STATIC"
    )


@pytest.fixture(scope="session")
def browser_context_args(
    browser_context_args: dict[str, object],
) -> dict[str, object]:
    """Force an English UI (the default for non-zh browsers) and permit
    real file downloads."""

    return {
        **browser_context_args,
        "locale": "en-US",
        "viewport": {"width": 1440, "height": 1200},
        "accept_downloads": True,
    }


@pytest.fixture
def rust_server(request: pytest.FixtureRequest) -> RustServer:
    """Start an isolated `seattrellis_web` process and retain its log."""

    results_dir = Path(
        os.environ.get("SEATTRELLIS_E2E_RESULTS", "test-results-rust")
    ).resolve()
    results_dir.mkdir(parents=True, exist_ok=True)
    test_name = re.sub(r"[^A-Za-z0-9_-]", "-", request.node.name).strip("-")
    log_path = results_dir / f"rust-server-{test_name}.log"
    port = _reserve_local_port()
    url = f"http://127.0.0.1:{port}"
    binary = _locate_server_binary()
    web_root = _locate_web_root()

    environment = os.environ.copy()
    environment.update(
        {
            "NO_PROXY": "127.0.0.1,localhost",
            "SEATTRELLIS_WEB_STATIC": str(web_root),
            "no_proxy": "127.0.0.1,localhost",
        }
    )
    # The Rust runtime must never fall back to a Python backend; if the
    # environment still carries a Python-path variable, drop it so a stale
    # environment cannot mask a hidden Python dependency.
    environment.pop("PYTHONPATH", None)

    process_options: dict[str, object] = {}
    if os.name == "posix":
        process_options["start_new_session"] = True
    elif hasattr(subprocess, "CREATE_NEW_PROCESS_GROUP"):
        process_options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP

    with log_path.open("w", encoding="utf-8") as log_file:
        process = subprocess.Popen(
            [str(binary), "--port", str(port)],
            cwd=REPOSITORY_ROOT,
            env=environment,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            text=True,
            **process_options,
        )
        server = RustServer(
            url=url,
            health_url=f"{url}/api/v1/health",
            process=process,
            log_path=log_path,
            binary_path=binary,
        )
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    break
                try:
                    _health_check(server.health_url)
                    break
                except OSError:
                    time.sleep(0.2)
            server.assert_healthy()
            server.assert_native_binary()
            yield server
        finally:
            _stop_process(process)
