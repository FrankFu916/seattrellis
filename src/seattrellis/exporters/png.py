from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from seattrellis.models.snapshot import SeatingSnapshot


def export_png(snapshot: SeatingSnapshot, output: str | Path) -> Path:
    path = Path(output)
    path.parent.mkdir(parents=True, exist_ok=True)

    min_row, max_row, min_col, max_col = _bounds(snapshot)
    cell_w, cell_h = 150, 88
    margin = 40
    width = (max_col - min_col + 1) * cell_w + margin * 2
    height = (max_row - min_row + 1) * cell_h + margin * 2 + 30
    image = Image.new("RGB", (width, height), "white")
    draw = ImageDraw.Draw(image)
    font = ImageFont.load_default()
    assignment_by_seat = {assignment.seat_id: assignment for assignment in snapshot.assignments}

    draw.text((margin, 12), snapshot.layout.name, fill="#111111", font=font)
    for seat in snapshot.layout.seats:
        x0 = margin + (seat.col - min_col) * cell_w
        y0 = margin + 30 + (seat.row - min_row) * cell_h
        x1 = x0 + cell_w - 8
        y1 = y0 + cell_h - 8
        fill = "#f2f2f2" if not seat.enabled else "#eaf4ff"
        outline = "#999999" if not seat.enabled else "#3b82f6"
        draw.rounded_rectangle((x0, y0, x1, y1), radius=6, fill=fill, outline=outline, width=2)
        assignment = assignment_by_seat.get(seat.seat_id)
        name = assignment.student_name if assignment else ""
        text = f"{seat.seat_id}\n{name}" if seat.enabled else f"{seat.seat_id}\n--"
        draw.multiline_text((x0 + 10, y0 + 16), text, fill="#111111", font=font, spacing=6)

    image.save(path)
    return path


def _bounds(snapshot: SeatingSnapshot) -> tuple[int, int, int, int]:
    rows = [seat.row for seat in snapshot.layout.seats]
    cols = [seat.col for seat in snapshot.layout.seats]
    return min(rows), max(rows), min(cols), max(cols)
