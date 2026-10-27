# -*- coding: utf-8 -*-
"""
Вікно налаштувань порталу: папка, колір, режим і правила сортування
"""

import os

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (QColorDialog, QComboBox, QDialog, QDialogButtonBox, QFileDialog, QFormLayout,
                               QHBoxLayout, QHeaderView, QLabel, QLineEdit, QMenu, QPushButton, QTableWidget,
                               QTableWidgetItem, QVBoxLayout, QWidget)

from .config import Portal, Rule
from .presets import PRESETS
from .rules import normalize_extensions

COLUMNS = ("Розширення", "Маска імені", "Мін, МБ", "Макс, МБ", "Підпапка")

HELP = ("Перше правило, що підійшло, вирішує, куди піде файл. Порожня клітинка = без умови.\n"
        "Підпапка: порожньо - прямо в папку порталу. Можна шаблон: {ext}, {EXT}, {year}, {month}, {day}, "
        "напр. «Video\\{year}-{month}».")


def _num(text: str) -> float:
    try:
        return max(float(text.replace(",", ".").strip() or 0), 0.0)
    except ValueError:
        return 0.0


def _fmt(value: float) -> str:
    return "" if not value else f"{value:g}"


class SettingsDialog(QDialog):

    def __init__(self, portal: Portal, parent: QWidget = None):
        super().__init__(parent)
        self.portal = portal
        self.setWindowTitle(f"Налаштування - {portal.name}")
        self.setWindowFlag(Qt.WindowStaysOnTopHint)
        self.resize(720, 480)
        self._color = QColor(portal.color)

        self.name_edit = QLineEdit(portal.name)
        self.folder_edit = QLineEdit(portal.folder)
        browse = QPushButton("Огляд…", clicked=self._browse)
        folder_row = QHBoxLayout()
        folder_row.addWidget(self.folder_edit, 1)
        folder_row.addWidget(browse)

        self.color_button = QPushButton(clicked=self._pick_color)
        self.color_button.setFixedWidth(90)
        self._paint_color_button()

        self.mode_combo = self._combo((("move", "Переміщувати"), ("copy", "Копіювати")), portal.mode)
        self.conflict_combo = self._combo((("rename", "Додати номер: файл (2).ext"),
                                           ("skip", "Пропустити"),
                                           ("replace", "Замінити")), portal.conflict)
        self.unmatched_combo = self._combo((("root", "Класти прямо в папку порталу"),
                                            ("skip", "Не приймати")), portal.unmatched)

        form = QFormLayout()
        form.addRow("Назва", self.name_edit)
        form.addRow("Папка", folder_row)
        form.addRow("Колір", self.color_button)
        form.addRow("Дія", self.mode_combo)
        form.addRow("Якщо файл уже є", self.conflict_combo)
        form.addRow("Файли без правила", self.unmatched_combo)

        self.table = QTableWidget(0, len(COLUMNS) + 1)
        self.table.setHorizontalHeaderLabels(("Увімк.",) + COLUMNS)
        header = self.table.horizontalHeader()
        header.setSectionResizeMode(QHeaderView.Interactive)
        header.setSectionResizeMode(1, QHeaderView.Stretch)
        header.setSectionResizeMode(5, QHeaderView.Stretch)
        header.setSectionResizeMode(0, QHeaderView.ResizeToContents)
        self.table.verticalHeader().setVisible(False)
        for rule in portal.rules:
            self._add_row(rule)
        self.table.clearSelection()
        self.table.scrollToTop()

        add = QPushButton("+ Правило", clicked=lambda: self._add_row(Rule()))
        preset_button = QPushButton("+ З набору")
        preset_menu = QMenu(preset_button)
        for name in PRESETS:
            preset_menu.addAction(name, lambda n=name: self._add_row(Rule(extensions=list(PRESETS[n][1]))))
        preset_button.setMenu(preset_menu)
        remove = QPushButton("Видалити", clicked=self._remove_row)
        up = QPushButton("↑", clicked=lambda: self._move_row(-1))
        down = QPushButton("↓", clicked=lambda: self._move_row(1))
        for button in (up, down):
            button.setFixedWidth(34)
        rule_buttons = QHBoxLayout()
        for widget in (add, preset_button, remove):
            rule_buttons.addWidget(widget)
        rule_buttons.addStretch(1)
        rule_buttons.addWidget(up)
        rule_buttons.addWidget(down)

        help_label = QLabel(HELP)
        help_label.setWordWrap(True)
        help_label.setStyleSheet("color: palette(mid);")

        buttons = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        buttons.button(QDialogButtonBox.Cancel).setText("Скасувати")
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout = QVBoxLayout(self)
        layout.addLayout(form)
        layout.addWidget(QLabel("<b>Правила сортування</b>"))
        layout.addWidget(self.table, 1)
        layout.addLayout(rule_buttons)
        layout.addWidget(help_label)
        layout.addWidget(buttons)

    @staticmethod
    def _combo(options, current) -> QComboBox:
        combo = QComboBox()
        for value, label in options:
            combo.addItem(label, value)
        index = combo.findData(current)
        combo.setCurrentIndex(max(index, 0))
        return combo

    def _paint_color_button(self) -> None:
        self.color_button.setText(self._color.name())
        text = "black" if self._color.lightness() > 150 else "white"
        self.color_button.setStyleSheet(f"background: {self._color.name()}; color: {text};")

    def _pick_color(self) -> None:
        color = QColorDialog.getColor(self._color, self, "Колір порталу")
        if color.isValid():
            self._color = color
            self._paint_color_button()

    def _browse(self) -> None:
        folder = QFileDialog.getExistingDirectory(self, "Папка порталу", self.folder_edit.text())
        if folder:
            self.folder_edit.setText(os.path.normpath(folder))
            if not self.name_edit.text().strip() or self.name_edit.text() in ("Портал", "Новий портал"):
                self.name_edit.setText(os.path.basename(folder.rstrip("\\/")))

    # ---------- таблиця правил ----------

    def _add_row(self, rule: Rule) -> None:
        row = self.table.rowCount()
        self.table.insertRow(row)
        check = QTableWidgetItem()
        check.setFlags(Qt.ItemIsUserCheckable | Qt.ItemIsEnabled | Qt.ItemIsSelectable)
        check.setCheckState(Qt.Checked if rule.enabled else Qt.Unchecked)
        self.table.setItem(row, 0, check)
        values = (" ".join(rule.extensions), rule.pattern, _fmt(rule.min_mb), _fmt(rule.max_mb), rule.subfolder)
        for column, value in enumerate(values, 1):
            self.table.setItem(row, column, QTableWidgetItem(value))
        self.table.setCurrentCell(row, 1)

    def _remove_row(self) -> None:
        row = self.table.currentRow()
        if row >= 0:
            self.table.removeRow(row)

    def _move_row(self, delta: int) -> None:
        row = self.table.currentRow()
        target = row + delta
        if row < 0 or not 0 <= target < self.table.rowCount():
            return
        rules = self.rules(skip_empty=False)
        rules.insert(target, rules.pop(row))
        self.table.setRowCount(0)
        for rule in rules:
            self._add_row(rule)
        self.table.setCurrentCell(target, 1)

    def _text(self, row: int, column: int) -> str:
        item = self.table.item(row, column)
        return item.text().strip() if item else ""

    def rules(self, skip_empty: bool = True) -> list:
        result = []
        for row in range(self.table.rowCount()):
            check = self.table.item(row, 0)
            rule = Rule(
                extensions=normalize_extensions(self._text(row, 1)),
                pattern=self._text(row, 2),
                min_mb=_num(self._text(row, 3)),
                max_mb=_num(self._text(row, 4)),
                subfolder=self._text(row, 5),
                enabled=check is None or check.checkState() == Qt.Checked,
            )
            if not skip_empty or rule.extensions or rule.pattern or rule.min_mb or rule.max_mb or rule.subfolder:
                result.append(rule)
        return result

    def accept(self) -> None:
        p = self.portal
        p.name = self.name_edit.text().strip() or "Портал"
        folder = self.folder_edit.text().strip()
        p.folder = os.path.normpath(folder) if folder else ""
        p.color = self._color.name()
        p.mode = self.mode_combo.currentData()
        p.conflict = self.conflict_combo.currentData()
        p.unmatched = self.unmatched_combo.currentData()
        p.rules = self.rules()
        super().accept()
