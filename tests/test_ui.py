#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Перевірка вікон без екрана (QT_QPA_PLATFORM=offscreen)
"""

import os
import sys
import tempfile
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

try:
    from PySide6.QtCore import QMimeData, QPointF, Qt, QThreadPool, QUrl
    from PySide6.QtGui import QDropEvent
    from PySide6.QtWidgets import QApplication
except ImportError:  # PySide6 не встановлено
    QApplication = None

if QApplication is not None:
    from dropportals.app import PortalManager
    from dropportals.config import AppConfig, load_config
    from dropportals.presets import make_preset_portal, make_sorter_portal
    from dropportals.settings_dialog import SettingsDialog


@unittest.skipIf(QApplication is None, "PySide6 не встановлено")
class TestUi(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.src = self.root / "Downloads"
        self.src.mkdir()
        self.dst = self.root / "Video"
        self.cfg = self.root / "cfg" / "config.json"

    def tearDown(self):
        QThreadPool.globalInstance().waitForDone()
        for widget in QApplication.topLevelWidgets():
            widget.deleteLater()
        self.app.processEvents()
        self._tmp.cleanup()

    def wait(self, window):
        for _ in range(200):
            QThreadPool.globalInstance().waitForDone(50)
            self.app.processEvents()
            if not window.busy:
                return
        self.fail("перенесення не завершилось")

    def drop(self, window, paths, modifiers=Qt.NoModifier):
        mime = QMimeData()
        mime.setUrls([QUrl.fromLocalFile(p) for p in paths])
        pos = QPointF(window.width() / 2, window.height() / 2)
        from PySide6.QtGui import QDragEnterEvent
        enter = QDragEnterEvent(pos.toPoint(), Qt.CopyAction | Qt.MoveAction, mime, Qt.LeftButton, modifiers)
        window.dragEnterEvent(enter)
        state = window._hover
        event = QDropEvent(pos, Qt.CopyAction | Qt.MoveAction, mime, Qt.LeftButton, modifiers)
        window.dropEvent(event)
        self.app.processEvents()
        return state

    def test_drop_moves_and_rejects(self):
        manager = PortalManager(AppConfig(portals=[make_preset_portal("Відео", str(self.dst))]), self.cfg)
        manager.start()
        window = next(iter(manager.windows.values()))
        self.assertTrue(window.isVisible())

        video = self.src / "clip.mp4"
        video.write_bytes(b"v")
        self.assertEqual(self.drop(window, [str(video)]), "ok")
        self.wait(window)
        self.assertTrue((self.dst / "clip.mp4").exists())
        self.assertFalse(video.exists())
        self.assertIn("переміщено 1", window._flash_text)
        window.grab()  # малювання не падає

        doc = self.src / "doc.pdf"
        doc.write_bytes(b"d")
        self.assertEqual(self.drop(window, [str(doc)]), "reject")
        self.wait(window)
        self.assertTrue(doc.exists())

        window.undo_last()
        self.assertTrue(video.exists())

    def test_ctrl_inverts_mode(self):
        manager = PortalManager(AppConfig(portals=[make_sorter_portal(str(self.dst))]), self.cfg)
        manager.start()
        window = next(iter(manager.windows.values()))
        doc = self.src / "doc.pdf"
        doc.write_bytes(b"d")
        self.drop(window, [str(doc)], Qt.ControlModifier)
        self.wait(window)
        self.assertTrue(doc.exists())
        self.assertTrue((self.dst / "Документи" / "doc.pdf").exists())

    def test_manager_save_duplicate_delete(self):
        manager = PortalManager(AppConfig(), self.cfg)
        manager.start()  # перший запуск створює порожній портал
        self.assertEqual(len(manager.config.portals), 1)
        first = manager.config.portals[0]
        manager.duplicate_portal(first)
        self.assertEqual(len(manager.windows), 2)
        manager.set_visible(first, False)
        manager.save()
        loaded = load_config(self.cfg)
        self.assertEqual(len(loaded.portals), 2)
        self.assertFalse(loaded.portals[0].visible)
        manager._build_menu()
        self.assertTrue(manager.menu.actions())
        manager.windows[first.id].grab()

    def test_settings_dialog_roundtrip(self):
        portal = make_sorter_portal(str(self.dst))
        dialog = SettingsDialog(portal)
        self.assertEqual(dialog.table.rowCount(), len(portal.rules))
        dialog.name_edit.setText("Сорт")
        dialog.table.item(0, 1).setText(".PDF, docx")
        dialog.table.item(0, 3).setText("1,5")
        dialog._add_row(dialog.rules()[0].__class__())  # порожній рядок відкидається
        dialog.table.setCurrentCell(1, 1)
        dialog._move_row(-1)
        dialog.accept()
        self.assertEqual(portal.name, "Сорт")
        self.assertEqual(portal.rules[1].extensions, ["pdf", "docx"])
        self.assertEqual(portal.rules[1].min_mb, 1.5)
        self.assertEqual(len(portal.rules), 9)


if __name__ == "__main__":
    unittest.main()
