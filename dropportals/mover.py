# -*- coding: utf-8 -*-
"""
Переміщення / копіювання файлів у портал і скасування останньої дії
"""

import os
import shutil
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable, List, Optional

from .config import Portal
from .rules import resolve_destination


@dataclass
class TransferResult:
    source: str
    target: str = ""
    action: str = ""   # moved | copied | skipped | error
    reason: str = ""

    @property
    def ok(self) -> bool:
        return self.action in ("moved", "copied")


def _same(a: str, b: str) -> bool:
    return os.path.normcase(os.path.abspath(a)) == os.path.normcase(os.path.abspath(b))


def _is_inside(child: str, parent: str) -> bool:
    child = os.path.normcase(os.path.abspath(child))
    parent = os.path.normcase(os.path.abspath(parent))
    return child == parent or child.startswith(parent.rstrip(os.sep) + os.sep)


def unique_path(path: str) -> str:
    """file.txt -> file (2).txt -> file (3).txt ..."""
    if not os.path.lexists(path):
        return path
    folder, name = os.path.split(path)
    stem, ext = os.path.splitext(name)
    if os.path.isdir(path):
        stem, ext = name, ""
    index = 2
    while True:
        candidate = os.path.join(folder, f"{stem} ({index}){ext}")
        if not os.path.lexists(candidate):
            return candidate
        index += 1


def _copy(src: str, dst: str) -> None:
    if os.path.isdir(src):
        shutil.copytree(src, dst)
    else:
        shutil.copy2(src, dst)


def _put(src: str, dst: str, mode: str) -> None:
    if mode == "copy":
        _copy(src, dst)
    else:
        shutil.move(src, dst)


def transfer(source: str, dest_dir: str, mode: str = "move", conflict: str = "rename") -> TransferResult:
    src = os.path.abspath(source)
    result = TransferResult(source=src)
    if not os.path.lexists(src):
        result.action, result.reason = "skipped", "файл не знайдено"
        return result
    dest_dir = os.path.abspath(dest_dir)
    target = os.path.join(dest_dir, os.path.basename(src.rstrip("\\/")))
    if _same(src, target):
        result.action, result.reason = "skipped", "вже в цій папці"
        return result
    if os.path.isdir(src) and _is_inside(dest_dir, src):
        result.action, result.reason = "error", "папку не можна покласти саму в себе"
        return result

    try:
        os.makedirs(dest_dir, exist_ok=True)
        if os.path.lexists(target):
            if conflict == "skip":
                result.action, result.reason = "skipped", "такий файл уже є"
                return result
            if conflict == "replace" and os.path.isfile(target) and os.path.isfile(src):
                # спочатку кладемо поруч під тимчасовим ім'ям, потім підміняємо -
                # якщо щось піде не так, старий файл не пропаде
                tmp = os.path.join(dest_dir, f".dropportals-{uuid.uuid4().hex[:8]}.tmp")
                _put(src, tmp, mode)
                os.replace(tmp, target)
                result.target, result.reason = target, "замінено"
                result.action = "copied" if mode == "copy" else "moved"
                return result
            target = unique_path(target)
        _put(src, target, mode)
        result.target = target
        result.action = "copied" if mode == "copy" else "moved"
    except OSError as error:
        result.action, result.reason = "error", str(error)
    return result


def transfer_many(portal: Portal, paths: Iterable[str], mode: Optional[str] = None,
                  progress: Optional[Callable[[int, int], None]] = None) -> List[TransferResult]:
    paths = list(paths)
    mode = mode or portal.mode
    results = []
    for index, path in enumerate(paths, 1):
        dest = resolve_destination(portal, path)
        if dest is None:
            results.append(TransferResult(source=os.path.abspath(path), action="skipped",
                                          reason="не підходить під правила порталу"))
        else:
            results.append(transfer(path, dest, mode, portal.conflict))
        if progress:
            progress(index, len(paths))
    return results


def undo(results: Iterable[TransferResult]) -> List[str]:
    """Повертає переміщене на місце і прибирає копії. Повертає список помилок"""
    errors = []
    for item in reversed(list(results)):
        if not item.ok or not item.target:
            continue
        try:
            if item.action == "moved":
                if os.path.lexists(item.source):
                    errors.append(f"{item.source}: на старому місці вже є файл")
                    continue
                os.makedirs(os.path.dirname(item.source), exist_ok=True)
                shutil.move(item.target, item.source)
            elif os.path.isdir(item.target) and not os.path.islink(item.target):
                shutil.rmtree(item.target)
            else:
                os.remove(item.target)
        except OSError as error:
            errors.append(f"{Path(item.target).name}: {error}")
    return errors
