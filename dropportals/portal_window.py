# -*- coding: utf-8 -*-
"""
Плаваюче вікно-портал: приймає перетягнуті файли і відправляє їх у папку
"""

import copy
import os
import sys
from pathlib import Path
from typing import List, Optional

from PySide6.QtCore import QObject, QPoint, QRectF, QRunnable, Qt, QThreadPool, QTimer, QUrl, Signal
from PySide6.QtGui import QAction, QActionGroup, QColor, QDesktopServices, QFont, QPainter, QPen
from PySide6.QtWidgets import QFileDialog, QMenu, QSizeGrip, QWidget

from .config import Portal
from .mover import TransferResult, transfer_many, undo
from .rules import resolve_destination, rules_summary

OK_COLOR = QColor("#3DD68C")
ERROR_COLOR = QColor("#FF6369")


def open_folder(path: str) -> None:
    if not path:
        return
    if sys.platform == "win32":
        os.startfile(path)  # noqa: S606 - відкрити у Провіднику
    else:
        QDesktopServices.openUrl(QUrl.fromLocalFile(path))


def local_paths(mime) -> List[str]:
    if not mime.hasUrls():
        return []
    return [url.toLocalFile() for url in mime.urls() if url.isLocalFile() and url.toLocalFile()]


class _TaskSignals(QObject):
    progress = Signal(int, int)
    finished = Signal(object, list)   # (задача, результати)


class TransferTask(QRunnable):
    """Переміщення у фоні - великі відео не підвішують вікно"""

    def __init__(self, portal: Portal, paths: List[str], mode: str):
        super().__init__()
        self.portal = copy.deepcopy(portal)
        self.paths = paths
        self.mode = mode
        self.signals = _TaskSignals()

    def run(self):
        results = transfer_many(self.portal, self.paths, self.mode, progress=self.signals.progress.emit)
        self.signals.finished.emit(self, results)


class PortalWindow(QWidget):
    changed = Signal()                 # змінились налаштування або положення
    settings_requested = Signal(object)
    duplicate_requested = Signal(object)
    delete_requested = Signal(object)

    def __init__(self, portal: Portal, pool: Optional[QThreadPool] = None):
        super().__init__(None, Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool)
        self.portal = portal
        self.pool = pool or QThreadPool.globalInstance()
        self.setAttribute(Qt.WA_TranslucentBackground)
        self.setAcceptDrops(True)
        self.setMinimumSize(110, 64)
        self.setWindowTitle(portal.name)

        self._hover = ""                # "" | ok | reject
        self._running = 0
        self._progress = (0, 0)
        self._flash_text = ""
        self._flash_color = OK_COLOR
        self._drag_offset: Optional[QPoint] = None
        self._moved = False
        self.last_results: List[TransferResult] = []
        self._tasks = set()

        self._flash_timer = QTimer(self, singleShot=True, timeout=self._clear_flash)
        self._grip = QSizeGrip(self)
        self._grip.setFixedSize(14, 14)
        self._grip.setStyleSheet("background: transparent;")

        self.apply_portal()

    # ---------- стан ----------

    def apply_portal(self) -> None:
        p = self.portal
        self.setGeometry(p.x, p.y, max(p.width, 110), max(p.height, 64))
        self.setWindowOpacity(min(max(p.opacity, 0.2), 1.0))
        self.setWindowTitle(p.name)
        self._update_tooltip()
        self.update()

    def store_geometry(self) -> None:
        g = self.geometry()
        self.portal.x, self.portal.y = g.x(), g.y()
        self.portal.width, self.portal.height = g.width(), g.height()

    def _update_tooltip(self, extra: str = "") -> None:
        p = self.portal
        mode = "копіювати" if p.mode == "copy" else "переміщувати"
        lines = [p.name, p.folder or "папку не вибрано", f"Режим: {mode} (Ctrl під час кидання - навпаки)"]
        if extra:
            lines += ["", extra]
        self.setToolTip("\n".join(lines))

    @property
    def busy(self) -> bool:
        return self._running > 0

    # ---------- малювання ----------

    def paintEvent(self, _event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing)
        rect = QRectF(self.rect()).adjusted(1.5, 1.5, -1.5, -1.5)
        base = QColor(self.portal.color)
        if not base.isValid():
            base = QColor("#4F7CFF")

        fill = QColor(base)
        if self._hover == "ok":
            fill = fill.lighter(125)
        elif self._hover == "reject":
            fill = QColor("#3A3A3F")
        fill.setAlpha(235)
        painter.setPen(Qt.NoPen)
        painter.setBrush(fill)
        painter.drawRoundedRect(rect, 14, 14)

        if self._hover:
            pen = QPen(QColor("white") if self._hover == "ok" else ERROR_COLOR, 2.5, Qt.DashLine)
            painter.setPen(pen)
            painter.setBrush(Qt.NoBrush)
            painter.drawRoundedRect(rect.adjusted(4, 4, -4, -4), 11, 11)

        inner = rect.adjusted(12, 10, -12, -10)
        painter.setPen(QColor("white"))
        title_font = QFont(self.font())
        title_font.setBold(True)
        title_font.setPointSizeF(max(self.font().pointSizeF(), 9) + 1.5)
        painter.setFont(title_font)
        title = painter.fontMetrics().elidedText(self.portal.name, Qt.ElideRight, int(inner.width()))
        painter.drawText(inner, Qt.AlignLeft | Qt.AlignTop, title)

        small = QFont(self.font())
        small.setPointSizeF(max(self.font().pointSizeF(), 9) - 0.5)
        painter.setFont(small)
        metrics = painter.fontMetrics()
        line_h = metrics.height()
        y = inner.top() + line_h + 8

        if self._flash_text and not self.busy:
            lines = [self._flash_text]
            painter.setPen(QColor("white"))
        elif self._hover == "reject":
            lines = ["не приймає ці файли"]
        elif not self.portal.folder:
            lines = ["кинь файли або", "ПКМ → Налаштування"]
        else:
            folder = Path(self.portal.folder).name or self.portal.folder
            lines = [f"→ {folder}", rules_summary(self.portal)]
            if self.portal.mode == "copy":
                lines.append("копіювання")
        text_color = QColor(255, 255, 255, 215)
        for line in lines:
            if y + line_h > inner.bottom() + 2:
                break
            painter.setPen(text_color)
            painter.drawText(QRectF(inner.left(), y, inner.width(), line_h), Qt.AlignLeft | Qt.AlignVCenter,
                             metrics.elidedText(line, Qt.ElideRight, int(inner.width())))
            y += line_h + 1

        if self._flash_text and not self.busy:
            painter.setPen(Qt.NoPen)
            painter.setBrush(self._flash_color)
            painter.drawEllipse(QRectF(rect.right() - 20, rect.top() + 10, 9, 9))

        if self.busy:
            done, total = self._progress
            bar = QRectF(inner.left(), rect.bottom() - 12, inner.width(), 4)
            painter.setPen(Qt.NoPen)
            painter.setBrush(QColor(255, 255, 255, 70))
            painter.drawRoundedRect(bar, 2, 2)
            if total:
                painter.setBrush(QColor("white"))
                painter.drawRoundedRect(QRectF(bar.left(), bar.top(), bar.width() * done / total, 4), 2, 2)

    def resizeEvent(self, event):
        self._grip.move(self.width() - self._grip.width() - 3, self.height() - self._grip.height() - 3)
        super().resizeEvent(event)
        if self.isVisible() and (self.width(), self.height()) != (self.portal.width, self.portal.height):
            self.store_geometry()
            self.changed.emit()

    # ---------- перетягування файлів ----------

    def _drop_state(self, paths: List[str]) -> str:
        if not paths:
            return ""
        if not self.portal.folder:
            return "ok"
        return "ok" if any(resolve_destination(self.portal, p) for p in paths) else "reject"

    def dragEnterEvent(self, event):
        paths = local_paths(event.mimeData())
        if not paths:
            event.ignore()
            return
        self._hover = self._drop_state(paths)
        event.setDropAction(Qt.CopyAction)
        event.accept()
        self.update()

    def dragMoveEvent(self, event):
        if self._hover == "ok":
            # Провіднику завжди кажемо "копія": переміщуємо ми самі,
            # щоб Windows не намагалась ще й видалити оригінал
            event.setDropAction(Qt.CopyAction)
            event.accept()
        else:
            event.ignore()

    def dragLeaveEvent(self, _event):
        self._hover = ""
        self.update()

    def dropEvent(self, event):
        paths = local_paths(event.mimeData())
        state, self._hover = self._hover, ""
        self.update()
        if not paths or state != "ok":
            event.ignore()
            return
        event.setDropAction(Qt.CopyAction)
        event.accept()
        mode = self.portal.mode
        if event.modifiers() & Qt.ControlModifier:
            mode = "move" if mode == "copy" else "copy"
        # не блокуємо Провідник, поки показуємо діалоги
        QTimer.singleShot(0, lambda: self.send(paths, mode))

    def send(self, paths: List[str], mode: Optional[str] = None) -> None:
        if not self.portal.folder and not self.choose_folder():
            return
        task = TransferTask(self.portal, paths, mode or self.portal.mode)
        task.signals.progress.connect(self._on_progress)
        # підключаємо методи вікна, а не lambda - тоді Qt викликає їх у головному потоці
        task.signals.finished.connect(self._on_finished)
        self._tasks.add(task)
        self._running += 1
        self._progress = (0, len(paths))
        self.update()
        self.pool.start(task)

    def _on_progress(self, done: int, total: int) -> None:
        self._progress = (done, total)
        self.update()

    def _on_finished(self, task: TransferTask, results: List[TransferResult]) -> None:
        self._tasks.discard(task)
        self._running = max(self._running - 1, 0)
        if any(r.ok for r in results):
            self.last_results = results
        done = sum(r.ok for r in results)
        skipped = [r for r in results if r.action == "skipped"]
        errors = [r for r in results if r.action == "error"]

        verb = "скопійовано" if task.mode == "copy" else "переміщено"
        if errors:
            text, color = f"помилок: {len(errors)}", ERROR_COLOR
        elif done == 0:
            text, color = f"пропущено {len(skipped)}", ERROR_COLOR
        else:
            text, color = f"✓ {verb} {done}", OK_COLOR
            if skipped:
                text += f" · пропущено {len(skipped)}"

        details = [f"{Path(r.source).name}: {r.reason}" for r in errors + skipped][:12]
        self._update_tooltip("\n".join(details))
        self.flash(text, color)

    def flash(self, text: str, color: QColor = OK_COLOR, ms: int = 4000) -> None:
        self._flash_text, self._flash_color = text, color
        self._flash_timer.start(ms)
        self.update()

    def _clear_flash(self) -> None:
        self._flash_text = ""
        self.update()

    # ---------- дії ----------

    def choose_folder(self) -> bool:
        folder = QFileDialog.getExistingDirectory(self, f"Папка для «{self.portal.name}»", self.portal.folder)
        if not folder:
            return False
        self.portal.folder = os.path.normpath(folder)
        if self.portal.name in ("Портал", "Новий портал"):
            self.portal.name = Path(folder).name or folder
        self.apply_portal()
        self.changed.emit()
        return True

    def undo_last(self) -> None:
        if not self.last_results:
            return
        errors = undo(self.last_results)
        self.last_results = []
        if errors:
            self._update_tooltip("\n".join(errors[:12]))
            self.flash(f"не вдалося повернути: {len(errors)}", ERROR_COLOR)
        else:
            self.flash("↶ повернуто")

    def set_mode(self, mode: str) -> None:
        self.portal.mode = mode
        self._update_tooltip()
        self.update()
        self.changed.emit()

    def set_opacity(self, value: float) -> None:
        self.portal.opacity = value
        self.setWindowOpacity(value)
        self.changed.emit()

    def contextMenuEvent(self, event):
        menu = QMenu(self)
        menu.addAction("Відкрити папку", lambda: open_folder(self.portal.folder)).setEnabled(bool(self.portal.folder))
        menu.addAction("Вибрати папку…", self.choose_folder)
        menu.addAction("Налаштування…", lambda: self.settings_requested.emit(self))
        undo_action = menu.addAction("Скасувати останнє", self.undo_last)
        undo_action.setEnabled(bool(self.last_results) and not self.busy)
        menu.addSeparator()

        group = QActionGroup(menu)
        for mode, label in (("move", "Переміщувати"), ("copy", "Копіювати")):
            action = QAction(label, menu, checkable=True, checked=self.portal.mode == mode)
            action.triggered.connect(lambda _=False, m=mode: self.set_mode(m))
            group.addAction(action)
            menu.addAction(action)

        opacity_menu = menu.addMenu("Прозорість")
        for value in (1.0, 0.9, 0.75, 0.6, 0.4):
            action = opacity_menu.addAction(f"{int(value * 100)}%", lambda v=value: self.set_opacity(v))
            action.setCheckable(True)
            action.setChecked(abs(self.portal.opacity - value) < 0.01)
        menu.addSeparator()
        menu.addAction("Дублювати", lambda: self.duplicate_requested.emit(self))
        menu.addAction("Сховати", self._hide)
        menu.addAction("Видалити портал…", lambda: self.delete_requested.emit(self))
        menu.exec(event.globalPos())

    def _hide(self) -> None:
        self.portal.visible = False
        self.hide()
        self.changed.emit()

    # ---------- переміщення вікна ----------

    def mousePressEvent(self, event):
        if event.button() == Qt.LeftButton:
            self._drag_offset = event.globalPosition().toPoint() - self.frameGeometry().topLeft()
            self._moved = False

    def mouseMoveEvent(self, event):
        if self._drag_offset is not None and event.buttons() & Qt.LeftButton:
            self.move(event.globalPosition().toPoint() - self._drag_offset)
            self._moved = True

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.LeftButton:
            self._drag_offset = None
            if self._moved:
                self.store_geometry()
                self.changed.emit()

    def mouseDoubleClickEvent(self, event):
        if event.button() == Qt.LeftButton:
            if self.portal.folder:
                open_folder(self.portal.folder)
            else:
                self.choose_folder()

    def closeEvent(self, event):
        # Alt+F4 на порталі - лише ховає його
        event.ignore()
        self._hide()
