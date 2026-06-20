"""Export seating snapshots."""

from pathlib import Path

from seattrellis.exporters.excel import export_excel
from seattrellis.exporters.html import export_html
from seattrellis.exporters.png import export_png
from seattrellis.models.snapshot import SeatingSnapshot

__all__ = ["export_excel", "export_html", "export_png", "export_snapshot"]


def export_snapshot(snapshot: SeatingSnapshot, output_format: str, output: str | Path | None = None) -> Path:
    output_format = output_format.lower()
    if output is None:
        output = Path("outputs") / f"seating.{_extension_for_format(output_format)}"
    if output_format in {"excel", "xlsx"}:
        return export_excel(snapshot, output)
    if output_format == "png":
        return export_png(snapshot, output)
    if output_format == "html":
        return export_html(snapshot, output)
    raise ValueError(f"Unsupported export format: {output_format}")


def _extension_for_format(output_format: str) -> str:
    if output_format in {"excel", "xlsx"}:
        return "xlsx"
    if output_format in {"png", "html"}:
        return output_format
    return output_format
