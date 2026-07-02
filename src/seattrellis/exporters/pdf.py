"""PDF export via WeasyPrint.

Depends on the ``pdf`` optional extra (weasyprint).
WeasyPrint is pure Python, shares CSS layout with the HTML exporter,
and requires no Node.js runtime.
"""

from __future__ import annotations

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
                "not be loaded. Install the platform Pango/Cairo dependencies "
                "described in docs/font-strategy.zh.md."
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
