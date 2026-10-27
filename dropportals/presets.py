# -*- coding: utf-8 -*-
"""
Готові набори розширень для швидкого створення порталів
"""

from typing import Dict, List

from .config import Portal, Rule

# назва -> (колір, розширення)
PRESETS: Dict[str, tuple] = {
    "Документи": ("#4F7CFF", ["pdf", "doc", "docx", "rtf", "txt", "md", "odt",
                              "xls", "xlsx", "csv", "ods", "ppt", "pptx", "odp", "key", "pages"]),
    "Відео": ("#E5484D", ["mp4", "mov", "avi", "mkv", "webm", "m4v", "wmv", "mxf", "mpg", "mpeg"]),
    "Зображення": ("#30A46C", ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff",
                               "heic", "svg", "exr", "tga", "dpx", "hdr"]),
    "Аудіо": ("#8E4EC6", ["mp3", "wav", "aif", "aiff", "flac", "ogg", "m4a", "aac", "wma"]),
    "Дизайн": ("#F76B15", ["psd", "psb", "ai", "eps", "indd", "fig", "sketch", "xd", "afdesign", "afphoto"]),
    "Моушн і 3D": ("#0090FF", ["aep", "aepx", "prproj", "blend", "c4d", "fbx", "obj", "glb", "gltf",
                               "abc", "usd", "usdz", "riv", "rev", "lottie"]),
    "Шрифти": ("#6E56CF", ["ttf", "otf", "woff", "woff2"]),
    "Архіви": ("#978365", ["zip", "rar", "7z", "tar", "gz"]),
    "Програми": ("#687076", ["exe", "msi", "dmg", "pkg", "zxp", "appx"]),
}

SORTER_NAME = "Сортувальник"


def preset_names() -> List[str]:
    return list(PRESETS)


def preset_rule(name: str, subfolder: str = "") -> Rule:
    return Rule(extensions=list(PRESETS[name][1]), subfolder=subfolder)


def make_preset_portal(name: str, folder: str = "") -> Portal:
    """Портал для одного типу: приймає лише свої файли"""
    color, _ = PRESETS[name]
    return Portal(name=name, folder=folder, color=color,
                  rules=[preset_rule(name)], unmatched="skip")


def make_sorter_portal(folder: str = "") -> Portal:
    """Як DSort: усе в одну папку, розкладаючи по підпапках за типом"""
    rules = [preset_rule(name, subfolder=name) for name in PRESETS]
    return Portal(name=SORTER_NAME, folder=folder, color="#12A594", rules=rules,
                  unmatched="root", width=200)
