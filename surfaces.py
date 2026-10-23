"""Appearance, honest scan progress, and the full-screen map workspace."""
import os
import time
import math

from PySide6.QtCore import QPoint, QRectF, Qt, QTimer, QSignalBlocker, QPropertyAnimation, QEasingCurve
from PySide6.QtGui import QAction, QColor, QKeySequence, QPainter, QPen
from PySide6.QtWidgets import (
    QApplication, QAbstractSpinBox, QButtonGroup, QCheckBox, QDialog, QHBoxLayout, QHeaderView,
    QLabel, QPushButton, QSizePolicy, QSlider, QSpinBox, QSplitter, QTableView, QToolButton, QVBoxLayout, QWidget,
)

from charts import SpaceMap
from models import BreakdownModel
from motion import reduced_motion
from scanner import size_text
from recycle import make_recycle_action


def label(text, name="", wrap=False):
    widget = QLabel(text)
    widget.setTextFormat(Qt.PlainText)
    widget.setObjectName(name)
    widget.setWordWrap(wrap)
    return widget


def button(text, callback, name=""):
    widget = QPushButton(text)
    widget.setObjectName(name)
    widget.setAccessibleName(text)
    widget.clicked.connect(callback)
    return widget


class ScalePanel(QDialog):
    def __init__(self, owner):
        super().__init__(owner, Qt.Popup)
        self.owner = owner
        self.setWindowTitle("Appearance")
        layout = QVBoxLayout(self)
        layout.setContentsMargins(20, 20, 20, 20)
        layout.setSpacing(14)
        layout.addWidget(label("Interface size", "section"))
        row = QHBoxLayout()
        self.slider = QSlider(Qt.Horizontal)
        self.slider.setRange(85, 150)
        self.slider.setAccessibleName("Interface scale slider")
        self.slider.setMinimumWidth(owner.s(180))
        self.percent = QSpinBox()
        self.percent.setButtonSymbols(QAbstractSpinBox.NoButtons)
        self.percent.setRange(85, 150)
        self.percent.setSuffix("%")
        self.percent.setKeyboardTracking(False)
        self.percent.setAccessibleName("Interface scale percentage")
        self.percent.setMinimumWidth(owner.s(80))
        self.slider.setValue(round(owner.ui_scale*100))
        self.percent.setValue(self.slider.value())
        self.slider.valueChanged.connect(self.percent.setValue)
        self.percent.valueChanged.connect(self.slider.setValue)
        self.slider.sliderReleased.connect(self.commit)
        self.slider.actionTriggered.connect(lambda _: QTimer.singleShot(0, self._keyboard_commit))
        self.percent.editingFinished.connect(self.commit)
        row.addWidget(self.slider, 1)
        row.addWidget(self.percent)
        layout.addLayout(row)
        ends = QHBoxLayout()
        ends.addWidget(label("85%", "muted"))
        ends.addStretch()
        ends.addWidget(label("150%", "muted"))
        layout.addLayout(ends)
        self.reduce = QCheckBox("Reduce motion")
        self.reduce.setChecked(owner.reduce_motion)
        self.reduce.setAccessibleName("Reduce motion")
        self.reduce.toggled.connect(owner.set_reduced_motion)
        layout.addWidget(self.reduce)
        bottom = QHBoxLayout()
        bottom.addWidget(button("Reset to 100%", self.reset))
        bottom.addStretch()
        bottom.addWidget(button("Done", self.hide))
        layout.addLayout(bottom)

    def _keyboard_commit(self):
        if not self.slider.isSliderDown():
            self.commit()

    def commit(self):
        self.owner.set_scale_percent(self.percent.value())

    def sync(self, value):
        with QSignalBlocker(self.slider), QSignalBlocker(self.percent):
            self.slider.setValue(value)
            self.percent.setValue(value)
        self.slider.setMinimumWidth(self.owner.s(180))
        self.percent.setMinimumWidth(self.owner.s(80))
        self.adjustSize()

    def reset(self):
        self.percent.setValue(100)
        self.commit()

    def open_at(self, anchor):
        self.sync(round(self.owner.ui_scale * 100))
        self.adjustSize()
        position = anchor.mapToGlobal(QPoint(anchor.width()-self.width(), anchor.height()+8))
        screen = anchor.screen().availableGeometry()
        position.setX(max(screen.left(), min(position.x(), screen.right()-self.width())))
        position.setY(max(screen.top(), min(position.y(), screen.bottom()-self.height())))
        self.move(position)
        self.show()
        self.percent.setFocus()


class ScanGlyph(QWidget):
    def __init__(self):
        super().__init__()
        self.setFixedSize(112, 112)
        self.phase = 0

    def paintEvent(self, event):
        dark = bool(QApplication.instance().property("darkMode"))
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing)
        painter.setPen(Qt.NoPen)
        color = QColor("#80c8d4" if dark else "#1b6c80")
        for i, rect in enumerate((QRectF(12,12,42,88), QRectF(60,12,40,40), QRectF(60,58,40,42))):
            color.setAlphaF(.34 + .12*math.sin(2*math.pi*(self.phase/100+i/3)) if not reduced_motion() else .4)
            painter.setBrush(color)
            painter.drawRoundedRect(rect, 7, 7)
        painter.setPen(QPen(QColor("#80c8d4" if dark else "#1b6c80"), 2))
        painter.setBrush(Qt.NoBrush)
        painter.drawRoundedRect(QRectF(4,4,104,104),14,14)


class LoadingPanel(QWidget):
    def __init__(self, owner):
        super().__init__()
        self.owner = owner
        self.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Ignored)
        self.state = "empty"
        self.started = time.monotonic()
        self.setAccessibleName("Folder scan in progress")
        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(16)
        layout.addStretch()
        self.glyph = ScanGlyph()
        layout.addWidget(self.glyph, alignment=Qt.AlignCenter)
        title = label("Measuring your folder", "title")
        self.title = title
        title.setAlignment(Qt.AlignCenter)
        layout.addWidget(title)
        message = label("Building a complete snapshot. You can cancel at any time.", "muted", True)
        self.message = message
        message.setAlignment(Qt.AlignCenter)
        layout.addWidget(message)
        stats = QHBoxLayout()
        self.values = []
        for name in ("Files measured", "Folders read", "Logical bytes found"):
            block = QVBoxLayout()
            value = label("0", "section")
            value.setAlignment(Qt.AlignCenter)
            caption = label(name, "muted")
            caption.setAlignment(Qt.AlignCenter)
            block.addWidget(value)
            block.addWidget(caption)
            stats.addLayout(block)
            self.values.append(value)
        self.stats = QWidget()
        self.stats.setLayout(stats)
        layout.addWidget(self.stats)
        self.elapsed = label("Starting…", "muted")
        self.elapsed.setAlignment(Qt.AlignCenter)
        layout.addWidget(self.elapsed)
        self.action = button("Cancel scan", self.activate)
        layout.addWidget(self.action, alignment=Qt.AlignCenter)
        layout.addStretch()
        self.timer = QTimer(self)
        self.timer.setInterval(40)
        self.timer.timeout.connect(self.tick)

    def set_state(self, state):
        self.state = state
        title, message, action = {
            "loading": ("Measuring your folder", "Building a complete snapshot. You can cancel at any time.", "Cancel scan"),
            "empty": ("See where your space goes", "Choose a folder to explore its files, folders, and size breakdown.", "Choose folder"),
            "error": ("This folder couldn’t be read", "Restore access, then retry the scan or choose another folder.", "Retry scan"),
            "cancelled": ("Scan cancelled", "Retry this scan or choose a different folder.", "Retry scan"),
        }[state]
        self.title.setText(title)
        self.message.setText(message)
        self.action.setText(action)
        self.action.setAccessibleName(action)
        self.stats.setVisible(state == "loading")
        self.elapsed.setVisible(state == "loading")
        self.setAccessibleName(title)
        if state == "loading":
            self.started = self.owner.session.started_at
            if self.isVisible() and not self.timer.isActive():
                self.timer.start(1000 if reduced_motion() else 40)
        else:
            self.timer.stop()
            self.setAccessibleDescription(message)
        self._compact(self.height())

    def activate(self):
        if self.state == "loading":
            self.owner.cancel()
        elif self.state == "empty":
            self.owner.choose_folder()
        else:
            self.owner.refresh()

    def tick(self):
        self.glyph.phase = (self.glyph.phase+2)%100
        self.glyph.update()
        seconds = int(time.monotonic()-self.started)
        self.elapsed.setText(f"{seconds//60:d}:{seconds%60:02d} elapsed · Progress depends on folder access")

    def update_progress(self, progress):
        files, folders, size = progress
        for value, text in zip(self.values, (f"{files:,}", f"{folders:,}", size_text(size))):
            value.setText(text)
        self.setAccessibleDescription(f"{files} files, {folders} folders, {size} logical bytes found. Scan in progress.")

    def showEvent(self, event):
        self.started = self.owner.session.started_at
        if self.state == "loading":
            self.timer.start(1000 if reduced_motion() else 40)
        super().showEvent(event)

    def hideEvent(self, event):
        self.timer.stop()
        super().hideEvent(event)

    def resizeEvent(self, event):
        self._compact(event.size().height())
        super().resizeEvent(event)

    def _compact(self, height):
        compact = height < self.owner.s(350)
        self.glyph.setVisible(not compact and self.state in ("empty", "loading"))
        self.message.setVisible(not compact)
        self.layout().setSpacing(self.owner.s(8 if compact else 16))


class MapExplorer(QDialog):
    def __init__(self, owner):
        super().__init__(owner, Qt.Dialog | Qt.FramelessWindowHint)
        self.owner = owner
        self.kind = "all"
        self._key = None
        self._next_origin = None
        self.animate_exit = True
        self._opacity_animation = None
        self.setAttribute(Qt.WA_DeleteOnClose)
        self.setModal(True)
        self.setWindowTitle("Space map · Space Lens v3")
        self.setObjectName("mapWorkspace")
        layout = QVBoxLayout(self)
        layout.setContentsMargins(owner.s(28), owner.s(24), owner.s(28), owner.s(24))
        layout.setSpacing(owner.s(16))
        header = QHBoxLayout()
        self.back_button = button("Back", self.back)
        self.up_button = button("Up", self.up)
        header.addWidget(self.back_button)
        header.addWidget(self.up_button)
        self.path = label("", "path")
        self.path.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        header.addWidget(self.path, 1)
        header.addWidget(button("Exit full screen", self.reject))
        layout.addLayout(header)
        title_row = QHBoxLayout()
        self.title = label("Space map", "title")
        self.title.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        title_row.addWidget(self.title, 1)
        self.total = label("", "section")
        title_row.addWidget(self.total)
        layout.addLayout(title_row)
        tools = QHBoxLayout()
        group = QButtonGroup(self)
        self.filters = {}
        for name, kind in (("All items", "all"), ("Folders", "folders"), ("Files", "files")):
            tab = button(name, lambda checked=False, kind=kind: self.set_kind(kind), "tab")
            tab.setCheckable(True)
            group.addButton(tab)
            tools.addWidget(tab)
            self.filters[kind] = tab
        tools.addStretch()
        self.zoom_label = label("100%", "muted")
        self.map = SpaceMap()
        self.map.full = True
        self.map.setAccessibleName("Expanded space map")
        self.map.chosen.connect(lambda entry: self.open_entry(entry, self.map.last_chosen_rect))
        self.map.zoomChanged.connect(lambda zoom: self.zoom_label.setText(f"{zoom*100:.0f}%"))
        tools.addWidget(button("−", lambda: self.map.zoom_to(self.map.target_zoom/1.3)))
        tools.addWidget(self.zoom_label)
        tools.addWidget(button("+", lambda: self.map.zoom_to(self.map.target_zoom*1.3)))
        tools.addWidget(button("Fit map", self.map.reset_zoom))
        layout.addLayout(tools)
        self.notice = label("", "muted", True)
        layout.addWidget(self.notice)
        split = QSplitter(Qt.Horizontal)
        split.setChildrenCollapsible(False)
        split.setHandleWidth(owner.s(20))
        split.addWidget(self.map)
        self.map.setMinimumWidth(owner.s(300))
        detail = QWidget()
        detail.setObjectName("mapDetails")
        details = QVBoxLayout(detail)
        details.setContentsMargins(*(owner.s(16) for _ in range(4)))
        self.count = label("Breakdown", "section")
        details.addWidget(self.count)
        self.model = BreakdownModel(self)
        self.model.icons = owner.table_model.icons
        self.table = QTableView()
        self.table.setModel(self.model)
        self.table.setAccessibleName("Expanded map complete breakdown")
        self.table.setSelectionBehavior(QTableView.SelectRows)
        self.table.setSelectionMode(QTableView.SingleSelection)
        self.table.setEditTriggers(QTableView.NoEditTriggers)
        self.table.setShowGrid(False)
        self.table.setWordWrap(False)
        self.table.verticalHeader().hide()
        self.table.verticalHeader().setDefaultSectionSize(owner.s(44))
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.Stretch)
        for column, width in ((1, 110), (2, 80), (3, 106)):
            self.table.setColumnWidth(column, owner.s(width))
        self.table.activated.connect(lambda index: self.open_entry(self.model.entries[index.row()]))
        self.table.selectionModel().currentChanged.connect(self.selected_entry)
        copy = QAction("Copy path", self.table)
        copy.setShortcut(QKeySequence.Copy)
        copy.setShortcutContext(Qt.WidgetWithChildrenShortcut)
        copy.triggered.connect(self.copy_selected)
        self.table.addAction(copy)
        details.addWidget(self.table, 1)
        self.selected = label("Select a file for its exact size. Enter opens a folder.", "footer", True)
        selection_row = QHBoxLayout()
        self.selected.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        selection_row.addWidget(self.selected, 1)
        self.recycle_action = make_recycle_action(owner, self.table, self.model)
        recycle_button = QToolButton()
        recycle_button.setDefaultAction(self.recycle_action)
        recycle_button.setAccessibleName("Move selected item to Recycle Bin")
        selection_row.addWidget(recycle_button)
        details.addLayout(selection_row)
        detail.setMinimumWidth(owner.s(380))
        split.addWidget(detail)
        split.setSizes([1100, 520])
        layout.addWidget(split, 1)
        layout.addWidget(label("Scroll to zoom at the pointer · Drag to pan · Click a folder to explore · Escape exits", "footer", True))
        for key, action in (("Alt+Left", self.back), ("Alt+Up", self.up), ("0", self.map.reset_zoom)):
            shortcut = QAction(self)
            shortcut.setShortcut(QKeySequence(key))
            shortcut.triggered.connect(action)
            self.addAction(shortcut)
        self.refresh()

    def present(self):
        self.setWindowOpacity(1 if reduced_motion() else 0)
        self.showFullScreen()
        if not reduced_motion():
            self._fade(1, 200)
        self.map.setFocus()

    def _fade(self, opacity, duration):
        if self._opacity_animation is not None:
            self._opacity_animation.stop()
            self._opacity_animation.deleteLater()
        self._opacity_animation = QPropertyAnimation(self, b"windowOpacity", self)
        self._opacity_animation.setDuration(duration)
        self._opacity_animation.setEasingCurve(QEasingCurve.OutCubic)
        self._opacity_animation.setStartValue(self.windowOpacity())
        self._opacity_animation.setEndValue(opacity)
        self._opacity_animation.start()

    def set_kind(self, kind):
        self.kind = kind
        self.refresh()

    def refresh(self):
        session, listing = self.owner.session, self.owner.session.listing
        if not listing:
            self.reject()
            return
        self.path.setText(session.current)
        self.path.setToolTip(session.current)
        self.title.setText(os.path.basename(session.current) or session.current)
        self.total.setText(size_text(listing.total) + (" + measured" if listing.errors else " logical size"))
        busy = self.owner.recycle_job is not None
        self.back_button.setEnabled(not busy and session.history_index > 0)
        self.up_button.setEnabled(not busy and session.current != session.root)
        self.table.setEnabled(not busy)
        self.map.setEnabled(not busy)
        index = self.table.currentIndex()
        self.recycle_action.setEnabled(not busy and index.isValid() and index.row() < self.model.rowCount())
        if busy:
            self.notice.setText("Moving to Recycle Bin… Keep Space Lens open until this finishes.")
        for kind, tab in self.filters.items():
            tab.setChecked(kind == self.kind)
        key = (id(session.snapshot), session.current, self.kind)
        if key == self._key:
            return
        self._key = key
        entries = [e for e in listing.entries if self.kind == "all" or e.is_dir == (self.kind == "folders")]
        self.model.set_entries(entries, listing.total, bool(listing.errors))
        self.map.set_entries(entries, listing.error or "No measured bytes in this folder.", origin=self._next_origin)
        self._next_origin = None
        self.count.setText(f"Breakdown · All {len(entries):,} items")
        self.notice.setText("Partial measurement · Unknown sizes have no map area." if listing.errors else
                           "Every positive measured item is represented. Zero-byte items remain in the breakdown.")
        self.selected.setText("Select a file for its exact size. Enter opens a folder.")

    def open_entry(self, entry, origin=None):
        if self.owner.recycle_job is not None:
            return
        if entry.is_dir:
            self._next_origin = origin
            if self.owner.session.navigate(entry.path):
                self.owner.render()
        else:
            row = next((i for i, value in enumerate(self.model.entries) if value.path == entry.path), -1)
            if row >= 0:
                self.table.setCurrentIndex(self.model.index(row, 0))
                self.table.scrollTo(self.model.index(row, 0))

    def selected_entry(self, index, previous=None):
        if index.isValid():
            entry = self.model.entries[index.row()]
            self.selected.setText(f"{entry.name} · {entry.size:,} bytes" if entry.size is not None else f"{entry.name} · Size unavailable")
            self.selected.setToolTip(entry.path)

    def copy_selected(self):
        index = self.table.currentIndex()
        if index.isValid():
            QApplication.clipboard().setText(self.model.entries[index.row()].path)
            self.selected.setText("Path copied to clipboard")

    def back(self):
        self.owner.go_back()

    def up(self):
        self.owner.go_up()

    def keyPressEvent(self, event):
        if event.key() == Qt.Key_Escape:
            self.animate_exit = False
            self.reject()
        else:
            super().keyPressEvent(event)

    def reject(self):
        self.map._spring.stop()
        if self.animate_exit and not reduced_motion() and self.isVisible():
            self._fade(0, 150)
            self._opacity_animation.finished.connect(lambda: QDialog.reject(self))
            return
        if self._opacity_animation is not None:
            self._opacity_animation.stop()
        super().reject()
