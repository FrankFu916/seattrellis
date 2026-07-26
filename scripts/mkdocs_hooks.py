"""Small build hooks used by the documentation site."""

from __future__ import annotations

import shutil
from pathlib import Path
from typing import Any


def on_post_build(config: Any, **_: object) -> None:
    """Publish committed JSON Schema files at the URLs used by their $id."""
    project_root = Path(config.config_file_path).resolve().parent
    source_dir = project_root / "schemas"
    destination_dir = Path(config.site_dir) / "schemas"
    destination_dir.mkdir(parents=True, exist_ok=True)
    for source in sorted(source_dir.glob("*.schema.json")):
        shutil.copy2(source, destination_dir / source.name)
