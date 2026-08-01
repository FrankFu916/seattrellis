# PyInstaller onedir recipe for the optional pywebview desktop application.
# Platform installers and signing are intentionally kept outside this spec.

import sys
from pathlib import Path

from PyInstaller.utils.hooks import collect_data_files


PROJECT_ROOT = Path(SPECPATH).resolve().parents[1]
DATA_FILES = collect_data_files("seattrellis", includes=["web_static/**/*"])
PLATFORM_IMPORTS = {
    "darwin": ["webview.platforms.cocoa"],
    "win32": ["webview.platforms.winforms", "webview.platforms.edgechromium"],
}.get(sys.platform, ["webview.platforms.gtk", "webview.platforms.qt"])

# Keep the desktop bundle small.  pyarrow is pulled in only by PIL's lazy
# ``import pyarrow`` hook, and ortools (plus its pandas/numpy dependency) is
# pulled in through the solver registry; the desktop ships the fallback solver
# and the png exporter does not use arrow arrays.  Both are already optional at
# runtime (guarded imports), so excluding them is safe and shrinks the archive
# by roughly 200 MB.
_EXCLUDED_OPTIONAL_MODULES = [
    "pyarrow",
    "ortools",
]

a = Analysis(
    [str(PROJECT_ROOT / "src" / "seattrellis" / "desktop_app.py")],
    pathex=[str(PROJECT_ROOT / "src")],
    binaries=[],
    datas=DATA_FILES,
    hiddenimports=PLATFORM_IMPORTS,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=_EXCLUDED_OPTIONAL_MODULES,
    noarchive=False,
)
pyz = PYZ(a.pure)
executable = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="SeatTrellis",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=False,
)
COLLECT(
    executable,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    name="SeatTrellis",
)
