# -*- coding: utf-8 -*-
"""
Drop Portals: іконка в треї, керування порталами, збереження налаштувань
"""

import copy
import os
import sys
import uuid
from pathlib import Path
from typing import Dict, Optional

from PySide6.QtCore import QObject, QPoint, QRect, QRectF, Qt, QTimer
from PySide6.QtGui import QColor, QCursor, QGuiApplication, QIcon, QPainter, QPen, QPixmap
from PySide6.QtNetwork import QLocalServer, QLocalSocket
from PySide6.QtWidgets import QApplication, QFileDialog, QMenu, QMessageBox, QSystemTrayIcon

from . import APP_ID, APP_NAME, __version__, autostart
from .config import AppConfig, Portal, config_dir, load_config, save_config
from .portal_window import PortalWindow, open_folder
from .presets import PRESETS, SORTER_NAME, make_preset_portal, make_sorter_portal
from .settings_dialog import SettingsDialog


def app_icon() -> QIcon:
    """Іконка-портал: кольорове кільце"""
    icon = QIcon()
    for size in (16, 24, 32, 48, 64, 256):
        pixmap = QPixmap(size, size)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing)
        width = max(size * 0.16, 2.0)
        rect = QRectF(width / 2 + 1, width / 2 + 1, size - width - 2, size - width - 2)
        painter.setPen(QPen(QColor("#4F7CFF"), width))
        painter.drawEllipse(rect)
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor("#12A594"))
        inner = size * 0.32
        painter.drawEllipse(QRectF((size - inner) / 2, (size - inner) / 2, inner, inner))
        painter.end()
        icon.addPixmap(pixmap)
    return icon


def server_name() -> str:
    user = os.environ.get("USERNAME") or os.environ.get("USER") or "user"
    return f"{APP_ID}-{user}"


def notify_running_instance() -> bool:
    """Якщо програма вже запущена - просимо її показати портали і виходимо"""
    socket = QLocalSocket()
    socket.connectToServer(server_name())
    if not socket.waitForConnected(300):
        return False
    socket.write(b"show")
    socket.waitForBytesWritten(300)
    socket.disconnectFromServer()
    return True


class PortalManager(QObject):

    def __init__(self, config: Optional[AppConfig] = None, config_file: Optional[Path] = None):
        super().__init__()
        self.config_file = config_file
        self.config = config if config is not None else load_config(config_file)
        self.windows: Dict[str, PortalWindow] = {}
        self._save_timer = QTimer(self, singleShot=True, interval=400, timeout=self.save)
        self.icon = app_icon()
        QApplication.setWindowIcon(self.icon)

        self.tray = QSystemTrayIcon(self.icon, self)
        self.tray.setToolTip(APP_NAME)
        self.menu = QMenu()
        self.menu.aboutToShow.connect(self._build_menu)
        self.tray.setContextMenu(self.menu)
        self.tray.activated.connect(self._on_tray_activated)

        for portal in self.config.portals:
            self._create_window(portal)

    def start(self) -> None:
        if QSystemTrayIcon.isSystemTrayAvailable():
            self.tray.show()
        if not self.config.portals:
            self.add_portal(Portal(name="Новий портал"), ask_folder=False)
            if self.tray.isVisible():
                self.tray.showMessage(APP_NAME, "Перетягни файли на портал. Нові портали - у меню цієї іконки.",
                                      QSystemTrayIcon.Information, 6000)

    # ---------- збереження ----------

    def schedule_save(self) -> None:
        self._save_timer.start()

    def save(self) -> None:
        for window in self.windows.values():
            window.store_geometry()
        try:
            save_config(self.config, self.config_file)
        except OSError as error:
            if self.tray.isVisible():
                self.tray.showMessage(APP_NAME, f"Не вдалося зберегти налаштування: {error}",
                                      QSystemTrayIcon.Warning, 6000)

    # ---------- портали ----------

    def _create_window(self, portal: Portal) -> PortalWindow:
        self._fit_on_screen(portal)
        window = PortalWindow(portal)
        window.changed.connect(self.schedule_save)
        window.settings_requested.connect(lambda w: self.edit_portal(w.portal))
        window.duplicate_requested.connect(lambda w: self.duplicate_portal(w.portal))
        window.delete_requested.connect(lambda w: self.delete_portal(w.portal))
        self.windows[portal.id] = window
        if portal.visible:
            window.show()
        return window

    @staticmethod
    def _fit_on_screen(portal: Portal) -> None:
        """Якщо монітор від'єднали - повертаємо портал на видимий екран"""
        rect = QRect(portal.x, portal.y, portal.width, portal.height)
        screens = QGuiApplication.screens()
        if not screens or any(s.availableGeometry().intersects(rect.adjusted(20, 20, -20, -20)) for s in screens):
            return
        area = QGuiApplication.primaryScreen().availableGeometry()
        portal.x = area.left() + 60
        portal.y = area.top() + 60

    def _place_near_cursor(self, portal: Portal) -> None:
        pos = QCursor.pos() - QPoint(portal.width // 2, portal.height // 2)
        screen = QGuiApplication.screenAt(QCursor.pos()) or QGuiApplication.primaryScreen()
        if screen:
            area = screen.availableGeometry()
            pos.setX(min(max(pos.x(), area.left()), area.right() - portal.width))
            pos.setY(min(max(pos.y(), area.top()), area.bottom() - portal.height))
        portal.x, portal.y = pos.x(), pos.y()

    def add_portal(self, portal: Portal, ask_folder: bool = True) -> Optional[PortalWindow]:
        if ask_folder and not portal.folder:
            folder = QFileDialog.getExistingDirectory(None, f"Папка для порталу «{portal.name}»")
            if not folder:
                return None
            portal.folder = os.path.normpath(folder)
            if portal.name in ("Портал", "Новий портал"):
                portal.name = Path(folder).name or folder
        self._place_near_cursor(portal)
        self.config.portals.append(portal)
        window = self._create_window(portal)
        window.raise_()
        self.schedule_save()
        return window

    def new_empty(self) -> None:
        self.add_portal(Portal(name="Новий портал"))

    def new_from_preset(self, name: str) -> None:
        self.add_portal(make_preset_portal(name))

    def new_sorter(self) -> None:
        self.add_portal(make_sorter_portal())

    def duplicate_portal(self, portal: Portal) -> None:
        clone = copy.deepcopy(portal)
        clone.id = uuid.uuid4().hex[:8]
        clone.name = f"{portal.name} (копія)"
        clone.visible = True
        clone.x += 30
        clone.y += 30
        self.config.portals.append(clone)
        self._create_window(clone).raise_()
        self.schedule_save()

    def edit_portal(self, portal: Portal) -> None:
        window = self.windows.get(portal.id)
        dialog = SettingsDialog(portal, window)
        if dialog.exec():
            if window:
                window.apply_portal()
            self.schedule_save()

    def delete_portal(self, portal: Portal) -> None:
        answer = QMessageBox.question(None, APP_NAME,
                                      f"Видалити портал «{portal.name}»?\nФайли в папці не постраждають.")
        if answer != QMessageBox.Yes:
            return
        window = self.windows.pop(portal.id, None)
        if window:
            window.hide()
            window.deleteLater()
        self.config.portals = [p for p in self.config.portals if p.id != portal.id]
        self.schedule_save()

    def set_visible(self, portal: Portal, visible: bool) -> None:
        portal.visible = visible
        window = self.windows.get(portal.id)
        if window:
            if visible:
                window.show()
                window.raise_()
            else:
                window.hide()
        self.schedule_save()

    def show_all(self) -> None:
        for portal in self.config.portals:
            self.set_visible(portal, True)

    def hide_all(self) -> None:
        for portal in self.config.portals:
            self.set_visible(portal, False)

    # ---------- трей ----------

    def _on_tray_activated(self, reason) -> None:
        if reason in (QSystemTrayIcon.Trigger, QSystemTrayIcon.DoubleClick):
            if any(not p.visible for p in self.config.portals):
                self.show_all()
            else:
                for window in self.windows.values():
                    window.raise_()

    def _build_menu(self) -> None:
        menu = self.menu
        menu.clear()
        new_menu = menu.addMenu("Новий портал")
        new_menu.addAction("Порожній (у вибрану папку)…", self.new_empty)
        new_menu.addAction(f"{SORTER_NAME} (розкладає по підпапках)…", self.new_sorter)
        new_menu.addSeparator()
        for name in PRESETS:
            new_menu.addAction(f"{name}…", lambda n=name: self.new_from_preset(n))

        menu.addSeparator()
        for portal in self.config.portals:
            action = menu.addAction(portal.name)
            action.setCheckable(True)
            action.setChecked(portal.visible)
            action.toggled.connect(lambda checked, p=portal: self.set_visible(p, checked))
        if self.config.portals:
            menu.addSeparator()
            menu.addAction("Показати всі", self.show_all)
            menu.addAction("Сховати всі", self.hide_all)

        menu.addSeparator()
        if autostart.supported():
            action = menu.addAction("Запускати з Windows")
            action.setCheckable(True)
            action.setChecked(autostart.is_enabled())
            action.toggled.connect(self._toggle_autostart)
        menu.addAction("Папка налаштувань", lambda: open_folder(str(self._config_folder())))
        menu.addAction(f"Вихід ({APP_NAME} {__version__})", self.quit)

    def _config_folder(self) -> Path:
        folder = self.config_file.parent if self.config_file else config_dir()
        folder.mkdir(parents=True, exist_ok=True)
        return folder

    def _toggle_autostart(self, enabled: bool) -> None:
        try:
            autostart.set_enabled(enabled)
        except OSError as error:
            QMessageBox.warning(None, APP_NAME, f"Не вдалося змінити автозапуск: {error}")

    def quit(self) -> None:
        self._save_timer.stop()
        self.save()
        self.tray.hide()
        QApplication.quit()


def main() -> int:
    if hasattr(Qt, "HighDpiScaleFactorRoundingPolicy"):
        QGuiApplication.setHighDpiScaleFactorRoundingPolicy(Qt.HighDpiScaleFactorRoundingPolicy.PassThrough)
    app = QApplication(sys.argv)
    app.setApplicationName(APP_NAME)
    app.setQuitOnLastWindowClosed(False)

    if notify_running_instance():
        return 0

    manager = PortalManager()
    server = QLocalServer()
    QLocalServer.removeServer(server_name())
    server.listen(server_name())

    def on_connection():
        socket = server.nextPendingConnection()
        if socket:
            socket.readyRead.connect(lambda: (socket.readAll(), manager.show_all()))
            socket.disconnected.connect(socket.deleteLater)

    server.newConnection.connect(on_connection)
    app.aboutToQuit.connect(manager.save)
    manager.start()
    return app.exec()
