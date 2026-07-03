"""PDF export via WeasyPrint.

Depends on the ``pdf`` optional extra (WeasyPrint) and a native Pango
installation. It shares CSS layout with the HTML exporter and requires no
Node.js runtime.
"""

from __future__ import annotations

import os
import sys
import tempfile
from pathlib import Path

from seattrellis.exporters.print_html import (
    PrintPrivacyOptions,
    _default_privacy,
    _render_print_html,
    _validate_template,
)
from seattrellis.models.candidate import CandidatePlan
from seattrellis.models.snapshot import SeatingSnapshot


def _configure_macos_library_path(
    search_roots: tuple[Path, ...] | None = None,
) -> bool:
    """Make Homebrew or MacPorts Pango libraries discoverable on macOS."""
    if sys.platform != "darwin":
        return False

    if search_roots is None:
        roots: list[Path] = []
        homebrew_prefix = os.environ.get("HOMEBREW_PREFIX")
        if homebrew_prefix:
            roots.append(Path(homebrew_prefix) / "lib")
        roots.extend(
            [
                Path("/opt/homebrew/lib"),
                Path("/usr/local/lib"),
                Path("/opt/local/lib"),
            ]
        )
        search_roots = tuple(dict.fromkeys(roots))

    library_dir = next(
        (
            root
            for root in search_roots
            if (root / "libpango-1.0.dylib").exists()
        ),
        None,
    )
    if library_dir is None:
        return False

    variable = "DYLD_FALLBACK_LIBRARY_PATH"
    current = [
        item
        for item in os.environ.get(variable, "").split(os.pathsep)
        if item
    ]
    library_text = str(library_dir)
    if library_text not in current:
        os.environ[variable] = os.pathsep.join([library_text, *current])
    return True


def export_pdf(
    snapshot: SeatingSnapshot,
    output: str | Path,
    *,
    template: str = "public",
    privacy: PrintPrivacyOptions | None = None,
    candidate: CandidatePlan | None = None,
) -> Path:
    """Export a seating snapshot as PDF via WeasyPrint.

    Parameters
    ----------
    snapshot:
        The seating snapshot.
    output:
        Output ``.pdf`` path.
    template:
        Print template: ``"public"``, ``"teacher"``, or ``"report"``.
    privacy:
        Privacy options.
    candidate:
        Candidate plan for the ``"report"`` template.
    """
    template = _validate_template(template)
    if template == "report" and candidate is None:
        raise ValueError("The report template requires a candidate plan.")

    _configure_macos_library_path()
    try:
        from weasyprint import HTML  # type: ignore[import-untyped]
    except ImportError as exc:  # pragma: no cover
        from seattrellis.optional import MissingOptionalDependencyError

        raise MissingOptionalDependencyError("PDF export", "pdf") from exc
    except OSError as exc:  # pragma: no cover
        from seattrellis.optional import MissingOptionalDependencyError

        raise MissingOptionalDependencyError(
            "PDF export",
            "pdf",
            detail=(
                "WeasyPrint is installed, but a native rendering library could "
                "not be loaded. On macOS, install Pango with `brew install "
                "pango`. See docs/font-strategy.zh.md for other platforms and "
                "manual library-path configuration."
            ),
        ) from exc

    resolved_privacy = privacy if privacy is not None else _default_privacy(template)
    html_str = _render_print_html(
        snapshot,
        template=template,
        privacy=resolved_privacy,
        candidate=candidate,
    )

    path = Path(output)
    path.parent.mkdir(parents=True, exist_ok=True)

    # WeasyPrint can render from a string; use a temp file for reliability
    # with CJK fonts.
    with tempfile.NamedTemporaryFile(suffix=".html", delete=False) as tmp:
        tmp.write(html_str.encode("utf-8"))
        tmp_path = Path(tmp.name)

    try:
        HTML(filename=str(tmp_path)).write_pdf(str(path))
    finally:
        tmp_path.unlink(missing_ok=True)

    return path
