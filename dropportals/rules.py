# -*- coding: utf-8 -*-
"""
Правила сортування: чи підходить файл і в яку папку його класти
"""

import fnmatch
import os
import re
from datetime import datetime
from pathlib import Path
from typing import Iterable, List, Optional

from .config import Portal, Rule

MB = 1024 * 1024
_INVALID = re.compile(r'[<>:"|?*\x00-\x1f]')


def normalize_extensions(text: str | Iterable[str]) -> List[str]:
    """"pdf, .DOCX; xls" -> ["pdf", "docx", "xls"]"""
    parts = re.split(r"[\s,;]+", text) if isinstance(text, str) else list(text)
    result = []
    for part in parts:
        ext = part.strip().lstrip("*").lstrip(".").lower()
        if ext and ext not in result:
            result.append(ext)
    return result


def file_extension(path: str) -> str:
    return Path(path).suffix.lstrip(".").lower()


def _item_size(path: str) -> int:
    try:
        return os.path.getsize(path)
    except OSError:
        return 0


def rule_matches(rule: Rule, path: str) -> bool:
    if not rule.enabled:
        return False
    is_dir = os.path.isdir(path)
    if rule.extensions:
        if is_dir or file_extension(path) not in rule.extensions:
            return False
    if rule.pattern and not fnmatch.fnmatch(Path(path).name.lower(), rule.pattern.lower()):
        return False
    if rule.min_mb > 0 or rule.max_mb > 0:
        if is_dir:
            return False
        size = _item_size(path)
        if rule.min_mb > 0 and size < rule.min_mb * MB:
            return False
        if rule.max_mb > 0 and size > rule.max_mb * MB:
            return False
    return True


def render_subfolder(template: str, path: str) -> str:
    """Підставляє {ext} {EXT} {name} {year} {month} {day} (дата зміни файлу).
    Результат - безпечний відносний шлях без '..'"""
    if not template:
        return ""
    try:
        stamp = datetime.fromtimestamp(os.path.getmtime(path))
    except OSError:
        stamp = datetime.now()
    ext = file_extension(path) or "no_ext"
    values = {
        "ext": ext, "EXT": ext.upper(), "name": Path(path).stem,
        "year": f"{stamp.year:04d}", "month": f"{stamp.month:02d}", "day": f"{stamp.day:02d}",
    }
    text = re.sub(r"\{(\w+)\}", lambda m: values.get(m.group(1), m.group(0)), template)
    parts = []
    for part in re.split(r"[\\/]+", text):
        part = _INVALID.sub("_", part).strip().rstrip(".")
        if part and part not in (".", ".."):
            parts.append(part)
    return os.path.join(*parts) if parts else ""


def matching_rule(portal: Portal, path: str) -> Optional[Rule]:
    for rule in portal.rules:
        if rule_matches(rule, path):
            return rule
    return None


def resolve_destination(portal: Portal, path: str) -> Optional[str]:
    """Папка призначення для файлу або None, якщо портал його не приймає.
    Перше правило, що підійшло, перемагає"""
    if not portal.folder:
        return None
    rule = matching_rule(portal, path)
    if rule is None:
        if portal.unmatched == "skip" and any(r.enabled for r in portal.rules):
            return None
        return portal.folder
    sub = render_subfolder(rule.subfolder, path)
    return os.path.join(portal.folder, sub) if sub else portal.folder


def rules_summary(portal: Portal, limit: int = 4) -> str:
    """Короткий підпис для вікна порталу: "pdf docx xls +12" """
    exts = []
    for rule in portal.rules:
        if rule.enabled:
            exts.extend(e for e in rule.extensions if e not in exts)
    if not exts:
        return "усі файли"
    shown = " ".join(exts[:limit])
    return f"{shown} +{len(exts) - limit}" if len(exts) > limit else shown
