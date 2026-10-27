# -*- coding: utf-8 -*-
"""
Автозапуск разом з Windows (HKCU\\...\\Run). На інших системах - вимкнено
"""

import os
import sys

from . import APP_ID

RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"


def supported() -> bool:
    return sys.platform == "win32"


def launch_command() -> str:
    if getattr(sys, "frozen", False):
        return f'"{sys.executable}"'
    # з вихідного коду - через pythonw, щоб не відкривалась консоль
    exe = sys.executable
    pythonw = os.path.join(os.path.dirname(exe), "pythonw.exe")
    if os.path.exists(pythonw):
        exe = pythonw
    script = os.path.abspath(sys.argv[0])
    return f'"{exe}" "{script}"'


def is_enabled() -> bool:
    if not supported():
        return False
    import winreg
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as key:
            winreg.QueryValueEx(key, APP_ID)
            return True
    except OSError:
        return False


def set_enabled(enabled: bool) -> None:
    if not supported():
        return
    import winreg
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
        if enabled:
            winreg.SetValueEx(key, APP_ID, 0, winreg.REG_SZ, launch_command())
        else:
            try:
                winreg.DeleteValue(key, APP_ID)
            except FileNotFoundError:
                pass
