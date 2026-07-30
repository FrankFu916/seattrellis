"""Process-level lifecycle for sensitive Web working directories.

Streamlit re-executes the application script on every interaction, so a
registry declared in ``app.py`` is not durable enough.  This imported module
owns the registry for the lifetime of the Python process and permits a page to
discard one registered directory as soon as its result is no longer needed.
"""

from __future__ import annotations

import atexit
import shutil
import tempfile
from pathlib import Path
from threading import RLock


_REGISTERED_DIRECTORIES: set[str] = set()
_REGISTRY_LOCK = RLock()


def make_persistent_tempdir() -> str:
    """Create and register a private working directory for Web results."""

    directory = tempfile.mkdtemp(prefix="seattrellis_")
    with _REGISTRY_LOCK:
        _REGISTERED_DIRECTORIES.add(directory)
    return directory


def discard_persistent_tempdir(directory: str | Path) -> bool:
    """Remove one registered directory and report whether it was owned here.

    Unknown paths are deliberately ignored.  This keeps a malformed session
    value from turning the cleanup helper into an arbitrary directory remover.
    """

    normalized = str(directory)
    with _REGISTRY_LOCK:
        if normalized not in _REGISTERED_DIRECTORIES:
            return False
        _REGISTERED_DIRECTORIES.remove(normalized)
    shutil.rmtree(normalized, ignore_errors=True)
    return True


def cleanup_persistent_tempdirs() -> None:
    """Remove every registered Web directory during process shutdown."""

    with _REGISTRY_LOCK:
        directories = tuple(_REGISTERED_DIRECTORIES)
        _REGISTERED_DIRECTORIES.clear()
    for directory in directories:
        shutil.rmtree(directory, ignore_errors=True)


atexit.register(cleanup_persistent_tempdirs)
