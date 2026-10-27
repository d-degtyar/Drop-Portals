#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Збирання DropPortals.exe (Windows):
    python -m pip install -r requirements.txt pyinstaller
    python build.py
Результат: dist\\DropPortals.exe
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def make_icon(target: Path) -> None:
    from PySide6.QtWidgets import QApplication
    from dropportals.app import app_icon

    app = QApplication.instance() or QApplication([])  # noqa: F841 - потрібен для QPixmap
    target.parent.mkdir(parents=True, exist_ok=True)
    app_icon().pixmap(256, 256).save(str(target), "ICO")


def main() -> int:
    sys.path.insert(0, str(ROOT))
    icon = ROOT / "build" / "icon.ico"
    make_icon(icon)
    command = [
        sys.executable, "-m", "PyInstaller", "--noconfirm", "--clean",
        "--onefile", "--windowed",
        "--name", "DropPortals",
        "--icon", str(icon),
        "--exclude-module", "PySide6.QtWebEngineCore",
        "--exclude-module", "PySide6.Qt3DCore",
        str(ROOT / "run.py"),
    ]
    return subprocess.call(command, cwd=ROOT)


if __name__ == "__main__":
    sys.exit(main())
