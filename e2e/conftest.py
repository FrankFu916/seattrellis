"""Shared fixtures for browser-level Web acceptance tests."""

from __future__ import annotations

import os
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from dataclasses import dataclass
from pathlib import Path

import pytest


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
APP_PATH = REPOSITORY_ROOT / "src" / "seattrellis" / "web" / "app.py"


@dataclass(frozen=True)
class WebServer:
    """A running Streamlit process used by a browser test session."""

    url: str
    health_url: str
    process: subprocess.Popen[str]
    log_path: Path

    def assert_healthy(self) -> None:
        """Fail when the server exited or stopped responding."""

        return_code = self.process.poll()
        if return_code is not None:
            pytest.fail(
                f"Streamlit exited with code {return_code}.\n"
                f"{_log_tail(self.log_path)}"
            )
        try:
            _open_without_proxy(self.health_url, timeout=2)
        except OSError as exc:
            pytest.fail(
                f"Streamlit health check failed: {exc}\n"
                f"{_log_tail(self.log_path)}"
            )


def _open_without_proxy(url: str, *, timeout: float) -> bytes:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(url, timeout=timeout) as response:
        return response.read()


def _reserve_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def _log_tail(path: Path, *, line_count: int = 80) -> str:
    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return "No Web server log is available."
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


@pytest.fixture(scope="session")
def browser_context_args(
    browser_context_args: dict[str, object],
) -> dict[str, object]:
    """Use a desktop viewport and permit real file downloads."""

    return {
        **browser_context_args,
        "viewport": {"width": 1440, "height": 1200},
        "accept_downloads": True,
    }


@pytest.fixture
def web_server(request: pytest.FixtureRequest) -> WebServer:
    """Start an isolated Streamlit process and retain its diagnostic log."""

    results_dir = Path(
        os.environ.get("SEATTRELLIS_E2E_RESULTS", "test-results")
    ).resolve()
    results_dir.mkdir(parents=True, exist_ok=True)
    runtime_dir = Path(
        tempfile.mkdtemp(prefix="web-runtime-", dir=results_dir)
    )
    test_name = "".join(
        character if character.isalnum() or character in {"-", "_"} else "-"
        for character in request.node.name
    ).strip("-")
    log_path = results_dir / f"web-server-{test_name}.log"
    port = _reserve_local_port()
    url = f"http://127.0.0.1:{port}"
    health_url = f"{url}/_stcore/health"

    environment = os.environ.copy()
    python_path = str(REPOSITORY_ROOT / "src")
    if environment.get("PYTHONPATH"):
        python_path = os.pathsep.join(
            [python_path, environment["PYTHONPATH"]]
        )
    environment.update(
        {
            "NO_PROXY": "127.0.0.1,localhost",
            "PYTHONDONTWRITEBYTECODE": "1",
            "PYTHONFAULTHANDLER": "1",
            "PYTHONPATH": python_path,
            "PYTHONUNBUFFERED": "1",
            "SEATTRELLIS_BACKEND": "fallback",
            "TMPDIR": str(runtime_dir),
            "TEMP": str(runtime_dir),
            "TMP": str(runtime_dir),
            "XDG_CACHE_HOME": str(runtime_dir / "cache"),
            "no_proxy": "127.0.0.1,localhost",
        }
    )
    command = [
        sys.executable,
        "-m",
        "streamlit",
        "run",
        str(APP_PATH),
        "--global.developmentMode=false",
        "--server.headless=true",
        "--server.address=127.0.0.1",
        f"--server.port={port}",
        "--server.fileWatcherType=none",
        "--browser.gatherUsageStats=false",
    ]
    process_options: dict[str, object] = {}
    if os.name == "posix":
        process_options["start_new_session"] = True
    elif hasattr(subprocess, "CREATE_NEW_PROCESS_GROUP"):
        process_options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP

    with log_path.open("w", encoding="utf-8") as log_file:
        process = subprocess.Popen(
            command,
            cwd=REPOSITORY_ROOT,
            env=environment,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            text=True,
            **process_options,
        )
        server = WebServer(
            url=url,
            health_url=health_url,
            process=process,
            log_path=log_path,
        )
        try:
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    break
                try:
                    if _open_without_proxy(health_url, timeout=1).strip() == b"ok":
                        break
                except OSError:
                    time.sleep(0.2)
            else:
                pytest.fail(
                    "Streamlit did not become healthy within 60 seconds.\n"
                    f"{_log_tail(log_path)}"
                )

            server.assert_healthy()
            yield server
        finally:
            unexpected_return_code = process.poll()
            if unexpected_return_code is not None:
                with log_path.open("a", encoding="utf-8") as diagnostics:
                    diagnostics.write(
                        "\nStreamlit exited unexpectedly with code "
                        f"{unexpected_return_code}.\n"
                    )
            _stop_process(process)
