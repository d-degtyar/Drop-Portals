# -*- coding: utf-8 -*-
"""
Налаштування порталів: збереження та завантаження з JSON
"""

import json
import os
import sys
import uuid
from dataclasses import asdict, dataclass, field, fields
from pathlib import Path
from typing import List, Optional

from . import APP_ID

MODES = ("move", "copy")
CONFLICTS = ("rename", "skip", "replace")
UNMATCHED = ("root", "skip")


@dataclass
class Rule:
    """Умова сортування: якщо файл підходить - кладемо в підпапку"""
    extensions: List[str] = field(default_factory=list)  # без крапки, у нижньому регістрі
    pattern: str = ""        # маска імені, напр. "*invoice*"; порожньо - будь-яке ім'я
    min_mb: float = 0.0      # 0 - без обмеження
    max_mb: float = 0.0      # 0 - без обмеження
    subfolder: str = ""      # шаблон підпапки: "Video", "{ext}", "{year}-{month}"
    enabled: bool = True


@dataclass
class Portal:
    """Один портал = одне плаваюче вікно"""
    id: str = field(default_factory=lambda: uuid.uuid4().hex[:8])
    name: str = "Портал"
    folder: str = ""
    color: str = "#4F7CFF"
    mode: str = "move"           # move | copy (Ctrl під час кидання - навпаки)
    conflict: str = "rename"     # rename | skip | replace
    unmatched: str = "root"      # root - в корінь папки, skip - не приймати
    rules: List[Rule] = field(default_factory=list)
    x: int = 120
    y: int = 120
    width: int = 180
    height: int = 110
    opacity: float = 0.92
    visible: bool = True


@dataclass
class AppConfig:
    portals: List[Portal] = field(default_factory=list)


def config_dir() -> Path:
    """Папка налаштувань: %APPDATA%\\DropPortals (Windows) або ~/.dropportals"""
    override = os.environ.get("DROP_PORTALS_HOME")
    if override:
        return Path(override)
    if sys.platform == "win32" and os.environ.get("APPDATA"):
        return Path(os.environ["APPDATA"]) / APP_ID
    return Path.home() / ".dropportals"


def config_path() -> Path:
    return config_dir() / "config.json"


def _pick(cls, data: dict) -> dict:
    """Лише відомі поля - старий або пошкоджений конфіг не ламає програму"""
    names = {f.name for f in fields(cls)}
    return {k: v for k, v in data.items() if k in names}


def portal_from_dict(data: dict) -> Portal:
    data = _pick(Portal, data)
    rules = [Rule(**_pick(Rule, r)) for r in data.pop("rules", []) if isinstance(r, dict)]
    portal = Portal(**data, rules=rules)
    if portal.mode not in MODES:
        portal.mode = "move"
    if portal.conflict not in CONFLICTS:
        portal.conflict = "rename"
    if portal.unmatched not in UNMATCHED:
        portal.unmatched = "root"
    return portal


def load_config(path: Optional[Path] = None) -> AppConfig:
    path = path or config_path()
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return AppConfig()
    portals = []
    items = data.get("portals", []) if isinstance(data, dict) else []
    for item in items if isinstance(items, list) else []:
        if not isinstance(item, dict):
            continue
        try:
            portals.append(portal_from_dict(item))
        except (TypeError, ValueError):
            continue
    return AppConfig(portals=portals)


def save_config(config: AppConfig, path: Optional[Path] = None) -> None:
    path = path or config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(asdict(config), ensure_ascii=False, indent=2), encoding="utf-8")
    os.replace(tmp, path)
