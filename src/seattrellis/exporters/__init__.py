"""Export seating snapshots."""

from pathlib import Path

from seattrellis.exporters.html import export_html
from seattrellis.models.candidate import CandidatePlan
from seattrellis.models.snapshot import SeatingSnapshot
from seattrellis.service_types import (
    ExportRequest,
    PageOptions,
    PrivacyOptions,
    export_extension,
)

__all__ = [
    "export_docx",
    "export_excel",
    "export_html",
    "export_pdf",
    "export_png",
    "export_print_html",
    "export_snapshot",
]


def export_excel(snapshot: SeatingSnapshot, output: str | Path) -> Path:
    from seattrellis.exporters.excel import export_excel as loaded_export_excel

    return loaded_export_excel(snapshot, output)


def export_png(snapshot: SeatingSnapshot, output: str | Path) -> Path:
    from seattrellis.exporters.png import export_png as loaded_export_png

    return loaded_export_png(snapshot, output)


def export_pdf(
    snapshot: SeatingSnapshot,
    output: str | Path,
    *,
    template: str = "public",
    privacy: PrivacyOptions | None = None,
    candidate: CandidatePlan | None = None,
    page: PageOptions | None = None,
    locale: str = "zh",
) -> Path:
    from seattrellis.exporters.pdf import export_pdf as loaded_export_pdf

    return loaded_export_pdf(
        snapshot,
        output,
        template=template,
        privacy=privacy,
        candidate=candidate,
        page=page,
        locale=locale,
    )


def export_docx(
    snapshot: SeatingSnapshot,
    output: str | Path,
    *,
    template: str = "public",
    privacy: PrivacyOptions | None = None,
    candidate: CandidatePlan | None = None,
    page: PageOptions | None = None,
    locale: str = "zh",
) -> Path:
    from seattrellis.exporters.docx_export import export_docx as loaded_export_docx

    return loaded_export_docx(
        snapshot,
        output,
        template=template,
        privacy=privacy,
        candidate=candidate,
        page=page,
        locale=locale,
    )


def export_print_html(
    snapshot: SeatingSnapshot,
    output: str | Path,
    *,
    template: str = "public",
    privacy: PrivacyOptions | None = None,
    candidate: CandidatePlan | None = None,
    page: PageOptions | None = None,
    locale: str = "zh",
) -> Path:
    from seattrellis.exporters.print_html import export_print_html as loaded_export_print_html

    return loaded_export_print_html(
        snapshot,
        output,
        template=template,
        privacy=privacy,
        candidate=candidate,
        page=page,
        locale=locale,
    )


def export_snapshot(
    snapshot: SeatingSnapshot,
    output_format: str | None = None,
    output: str | Path | None = None,
    *,
    template: str = "public",
    privacy: PrivacyOptions | None = None,
    candidate: CandidatePlan | None = None,
    page: PageOptions | None = None,
    locale: str = "zh",
    request: ExportRequest | None = None,
) -> Path:
    if request is None:
        if output_format is None:
            raise ValueError("output_format is required when request is not provided.")
        request = ExportRequest(
            output_format=output_format,
            output_path=output,
            template=template,
            privacy=privacy,
            page=page or PageOptions(),
            locale=locale,
        )
    elif output_format is not None and output_format.lower() != request.output_format:
        raise ValueError("output_format conflicts with request.output_format.")

    output_format = request.output_format
    output = request.resolved_output_path
    privacy = request.resolved_privacy
    configurable_formats = {"pdf", "docx", "print-html"}
    if output_format not in configurable_formats and (
        request.privacy is not None
        or request.template != "public"
        or request.page != PageOptions()
        or request.locale != "zh"
    ):
        raise ValueError(
            f"Export format {output_format!r} does not yet support template, "
            "privacy, page, or locale options."
        )
    if output_format in {"excel", "xlsx"}:
        return export_excel(snapshot, output)
    if output_format == "png":
        return export_png(snapshot, output)
    if output_format == "html":
        return export_html(snapshot, output)
    if output_format == "pdf":
        return export_pdf(
            snapshot,
            output,
            template=request.template,
            privacy=privacy,
            candidate=candidate,
            page=request.page,
            locale=request.locale,
        )
    if output_format == "docx":
        return export_docx(
            snapshot,
            output,
            template=request.template,
            privacy=privacy,
            candidate=candidate,
            page=request.page,
            locale=request.locale,
        )
    if output_format == "print-html":
        return export_print_html(
            snapshot,
            output,
            template=request.template,
            privacy=privacy,
            candidate=candidate,
            page=request.page,
            locale=request.locale,
        )
    raise ValueError(f"Unsupported export format: {output_format}")
