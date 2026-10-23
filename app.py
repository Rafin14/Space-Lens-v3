"""Space Lens v3 — local storage explorer with confirmed recycling."""
from __future__ import annotations

import os
import re
import sys

# Present complete widget frames through Qt's vsynced Windows swapchain.
if os.name == "nt":
    os.environ.setdefault("QT_WIDGETS_RHI", "1")
    os.environ.setdefault("QT_WIDGETS_RHI_BACKEND", "d3d11")

from PySide6.QtCore import QSettings, QSize, Qt, QTimer
from PySide6.QtGui import QAction, QColor, QFont, QIcon, QKeySequence, QPainter, QPalette, QPen, QPixmap, QSurfaceFormat
from PySide6.QtWidgets import (
    QApplication, QButtonGroup, QDialog, QDialogButtonBox, QFileDialog, QHBoxLayout,
    QHeaderView, QLabel, QMainWindow, QPushButton, QScrollArea, QSizePolicy,
    QSplitter, QStackedWidget, QStyle, QTableView, QTabWidget, QToolButton, QTreeView,
    QVBoxLayout, QWidget,
)

from charts import RankBars, SpaceMap
from models import BreakdownModel, FolderModel
from scanner import size_text
from session import Session
from motion import SnapshotTransition, reduced_motion, system_reduced_motion
from surfaces import LoadingPanel, MapExplorer, ScalePanel, label
from recycle import RecycleJob, target_identity, confirm_recycle, show_recycle_error, make_recycle_action
from elevation import ensure_elevated

presentation_format = QSurfaceFormat.defaultFormat()
presentation_format.setSwapInterval(1)
QSurfaceFormat.setDefaultFormat(presentation_format)

STYLE = """
QWidget { color: #131416; font-family: 'Segoe UI'; font-size: 14px; }
QMainWindow, QWidget#chrome, QWidget#sidebar { background: #f3f7f7; }
QWidget#sheet { background: #ffffff; border: 1px solid #e8eaec; border-radius: 16px; }
QWidget#sheet QWidget, QDialog { background: #ffffff; }
QLabel { background: transparent; border: none; }
QLabel#brand { font-size: 16px; font-weight: 600; }
QLabel#title { font-size: 24px; font-weight: 600; }
QLabel#total { font-size: 32px; font-weight: 400; }
QLabel#section { font-size: 16px; font-weight: 600; }
QLabel#muted, QLabel#path, QLabel#stamp, QLabel#footer { color: #5d5f60; }
QLabel#footer, QLabel#stamp { font-size: 12px; }
QLabel#notice { background: #f4f6f8; padding: 10px 12px; border-radius: 6px; }
QLabel#notice[warning='true'] { color: #92400e; background: #fffbeb; }
QPushButton, QToolButton { background: #f4f6f8; border: 1px solid #cdced1; border-radius: 6px; padding: 6px 12px; min-height: 20px; }
QPushButton:hover, QToolButton:hover { background: #ebedee; }
QPushButton:pressed, QToolButton:pressed { background: #dfe3e4; }
QPushButton:focus, QToolButton:focus { border: 2px solid #1b6c80; padding: 5px 11px; }
QPushButton:disabled, QToolButton:disabled { color: #65686b; background: #f4f6f8; border-color: #e3e5e7; }
QPushButton#primary { background: #202426; color: #ffffff; border-color: #202426; }
QPushButton#primary:hover { background: #363c3f; }
QPushButton#primary:pressed { background: #111416; }
QPushButton#tab { border: none; border-radius: 0; border-bottom: 2px solid transparent; background: transparent; color: #5d5f60; padding: 6px 8px; }
QPushButton#tab:checked { color: #131416; border-bottom: 2px solid #1b6c80; }
QPushButton#tab:focus { background: #ebedee; }
QTreeView, QTableView { background: transparent; border: none; selection-background-color: #e5eeee; selection-color: #131416; outline: none; }
QTreeView::item { min-height: 32px; padding: 2px 4px; }
QTableView::item { padding: 6px 8px; border-bottom: 1px solid #f0f1f2; }
QTreeView::item:focus, QTableView::item:focus { border: 1px solid #1b6c80; }
QHeaderView::section { background: #ffffff; color: #5d5f60; border: none; padding: 10px 8px; font-weight: 400; }
QScrollArea { border: none; background: transparent; }
QSplitter::handle { background: transparent; }
QSplitter::handle:hover { background: #e8eaec; }
QToolTip { background: #202426; color: #ffffff; border: none; padding: 8px; }
QScrollBar:vertical { background: #f4f6f8; width: 10px; margin: 0; border-radius: 5px; }
QScrollBar::handle:vertical { background: #aeb8bc; min-height: 28px; border-radius: 5px; }
QScrollBar::handle:vertical:hover { background: #7d8d94; }
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical { height: 0; }
QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical { background: transparent; }
QScrollBar#rankingScroll:vertical { background: transparent; width: 4px; }
QScrollBar#rankingScroll::handle:vertical { background: #aeb8bc; border-radius: 2px; min-height: 28px; }
QTabWidget::pane { border: none; }
QTabBar::tab { background: #ffffff; color: #5d5f60; padding: 8px 14px; border-bottom: 2px solid transparent; }
QTabBar::tab:selected { color: #131416; border-bottom: 2px solid #1b6c80; }
QComboBox { background: #ffffff; border: 1px solid #cdced1; border-radius: 6px; padding: 6px 10px; min-width: 60px; }
QComboBox:focus { border-color: #1b6c80; }
QComboBox QAbstractItemView { background: #ffffff; selection-background-color: #e5eeee; selection-color: #131416; }
QCheckBox { background: transparent; spacing: 7px; }
QCheckBox::indicator { width: 16px; height: 16px; }
QPushButton#theme:checked { background: #e5eeee; border-color: #1b6c80; color: #131416; }
QSpinBox { background: #ffffff; border: 1px solid #cdced1; border-radius: 6px; padding: 6px; min-height: 20px; }
QSlider::groove:horizontal { height: 4px; background: #cdced1; border-radius: 2px; }
QSlider::sub-page:horizontal { background: #1b6c80; border-radius: 2px; }
QSlider::handle:horizontal { width: 16px; margin: -7px 0; border-radius: 8px; background: #1b6c80; }
QMenu, QWidget#sheet QMenu { background: #ffffff; color: #131416; border: 1px solid #cdced1; border-radius: 8px; padding: 6px; }
QMenu::item { padding: 8px 16px; border-radius: 4px; background: transparent; }
QMenu::item:selected { background: #e5eeee; color: #131416; }
QMenu::item:disabled { color: #65686b; }
QMenu::separator { height: 1px; background: #e8eaec; margin: 6px 10px; }
"""

DARK_COLORS = {
    "#f3f7f7": "#15191e", "#ffffff": "#222830", "#f4f6f8": "#2b333d",
    "#131416": "#edf2f7", "#5d5f60": "#acb8c6", "#e8eaec": "#39434f",
    "#cdced1": "#4a5664", "#ebedee": "#35414d", "#dfe3e4": "#414f5e",
    "#e3e5e7": "#343d48", "#65686b": "#8391a1", "#202426": "#e1e9f0",
    "#363c3f": "#f0f5f9", "#111416": "#bdcbd8", "#e5eeee": "#354e59",
    "#f0f1f2": "#343e49", "#1b6c80": "#8ccbd5", "#92400e": "#edc887",
    "#fffbeb": "#3a3227", "#aeb8bc": "#617080", "#7d8d94": "#8e9eaf",
}

DARK_STYLE = """
QWidget#sidebar { background: #191f26; border: 1px solid #2b343f; border-radius: 12px; }
QWidget#sheet { background: #222830; border: 1px solid #39434f; }
QWidget#breakdownPanel, QWidget#mapDetails { background: #272f39; border: 1px solid #3b4755; border-radius: 10px; }
QWidget#breakdownPanel QLabel, QWidget#mapDetails QLabel { background: transparent; border: none; }
QWidget#sheet QTableView, QWidget#sheet QTableView QWidget, QWidget#mapDetails QTableView, QWidget#mapDetails QTableView QWidget { background: #272f39; }
QDialog#mapWorkspace { background: #1c222a; }
QHeaderView::section { background: #272f39; color: #aebdcd; border-bottom: 1px solid #3b4755; }
QTableView::item:hover, QTreeView::item:hover { background: #303d49; }
QMenu, QWidget#sheet QMenu { background: #2b3440; border: 1px solid #536171; }
QToolTip { background: #2c3743; color: #eef5fa; border: 1px solid #596c7e; border-radius: 6px; padding: 9px 12px; }
"""


def mark_icon():
    pixmap = QPixmap(64, 64)
    pixmap.fill(Qt.transparent)
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.Antialiasing)
    painter.setPen(QPen(QColor("#80c8d4" if QApplication.instance().property("darkMode") else "#1b6c80"), 4))
    painter.drawRoundedRect(9, 9, 42, 42, 11, 11)
    painter.drawLine(33, 11, 33, 49)
    painter.drawLine(34, 32, 49, 32)
    painter.drawLine(48, 48, 59, 59)
    painter.end()
    return QIcon(pixmap)


class MainWindow(QMainWindow):
    def __init__(self, session=None, preferences=True):
        super().__init__()
        self.settings = QSettings("Space Lens", "Space Lens v3") if preferences else None
        self.dark = self.settings.value("darkMode", False, type=bool) if self.settings else False
        scale = self.settings.value("uiScale", 100, type=int) if self.settings else 100
        self.ui_scale = scale / 100 if 85 <= scale <= 150 else 1.0
        self.reduce_motion = self.settings.value("reduceMotion", system_reduced_motion(), type=bool) if self.settings else True
        QApplication.instance().setProperty("reduceMotion", self.reduce_motion)
        self.explorer = None
        self.recycle_job = None
        self.scale_panel = None
        self._theme_transition = None
        apply_style(QApplication.instance(), self.dark, self.ui_scale)
        self.session = session or Session()
        self.mode = "folders"
        self._shown_snapshot = None
        self._shown_location = None
        self._shown_state = None
        self._expanded: set[str] = set()
        self.setWindowTitle("Space Lens v3")
        self.setWindowIcon(mark_icon())
        self.resize(1360, 860)
        self.setMinimumSize(800, 600)
        self._ui()
        self._scale_layout()
        self._responsive(self.width())
        self._shortcuts()
        self.timer = QTimer(self)
        self.timer.timeout.connect(self._poll)
        self.timer.start(40)
        self.render()

    def button(self, text, callback, primary=False):
        button = QPushButton(text)
        if primary:
            button.setObjectName("primary")
        button.setAccessibleName(text.replace("&", ""))
        button.clicked.connect(callback)
        return button

    def _ui(self):
        chrome = QWidget()
        chrome.setObjectName("chrome")
        outer = QVBoxLayout(chrome)
        outer.setContentsMargins(16, 0, 8, 8)
        outer.setSpacing(0)
        top = QHBoxLayout()
        top.setSpacing(10)
        top.setContentsMargins(8, 8, 16, 8)
        mark = QLabel()
        self.mark_label = mark
        mark.setPixmap(self.windowIcon().pixmap(QSize(self.s(26), self.s(26))))
        mark.setAccessibleName("Space Lens")
        top.addWidget(mark)
        top.addWidget(label("Space Lens", "brand"))
        top.addWidget(label("v3", "muted"))
        top.addStretch()
        theme_group = QButtonGroup(chrome)
        for text, dark in (("Light", False), ("Dark", True)):
            theme_button = self.button(text, lambda checked=False, dark=dark: self.set_dark_mode(dark))
            theme_button.setObjectName("theme")
            theme_button.setAccessibleName(f"{text} theme")
            theme_button.setCheckable(True)
            theme_button.setChecked(self.dark == dark)
            theme_group.addButton(theme_button)
            top.addWidget(theme_button)
            if dark:
                self.dark_button = theme_button
            else:
                self.light_button = theme_button
        self.scale_button = self.button(f"Scale {self.ui_scale*100:.0f}%", self.show_scale_panel)
        self.scale_button.setAccessibleName("Interface scale and motion settings")
        top.addWidget(self.scale_button)
        self.help_button = self.button("Shortcuts", self.show_help)
        self.choose_button = self.button("Choose folder", self.choose_folder, True)
        self.choose_button.setToolTip("Choose a folder · Ctrl+O")
        top.addWidget(self.help_button)
        top.addWidget(self.choose_button)
        outer.addLayout(top)
        self.horizontal = QSplitter(Qt.Horizontal)
        self.horizontal.setHandleWidth(8)
        self.sidebar = QWidget()
        self.sidebar.setObjectName("sidebar")
        sidebar_layout = QVBoxLayout(self.sidebar)
        sidebar_layout.setContentsMargins(8, 20, 12, 12)
        sidebar_layout.setSpacing(12)
        sidebar_layout.addWidget(label("Folder hierarchy", "section"))
        sidebar_layout.addWidget(label("The selected scan stays in this folder.", "muted", True))
        self.folder_model = FolderModel(self)
        self.tree = QTreeView()
        self.tree.setAccessibleName("Folder hierarchy")
        self.tree.setAccessibleDescription("Use arrows to expand and collapse. Press Enter to inspect the focused folder.")
        self.tree.setModel(self.folder_model)
        self.tree.setUniformRowHeights(True)
        self.tree.setIndentation(16)
        self.tree.setAnimated(False)
        self.tree.setExpandsOnDoubleClick(False)
        self.tree.setSelectionMode(QTreeView.SingleSelection)
        self.tree.header().setStretchLastSection(False)
        self.tree.header().setSectionResizeMode(0, QHeaderView.Stretch)
        self.tree.header().setSectionResizeMode(1, QHeaderView.Fixed)
        self.tree.setColumnWidth(1, 98)
        self.tree.activated.connect(self.open_tree)
        self.tree.expanded.connect(lambda index: self._expanded.add(index.internalPointer().entry.path))
        self.tree.collapsed.connect(lambda index: self._expanded.discard(index.internalPointer().entry.path))
        sidebar_layout.addWidget(self.tree, 1)
        sidebar_layout.addWidget(label("Snapshot scan\nLinks and reparse points are excluded.", "footer", True))
        self.sidebar.setMinimumWidth(220)
        self.horizontal.addWidget(self.sidebar)
        sheet = QWidget()
        sheet.setObjectName("sheet")
        self.sheet = sheet
        body = QVBoxLayout(sheet)
        body.setContentsMargins(24, 20, 24, 16)
        body.setSpacing(16)
        nav = QHBoxLayout()
        nav.setSpacing(8)
        self.back_button = self.button("Back", self.go_back)
        self.up_button = self.button("Up", self.go_up)
        self.refresh_button = self.button("Refresh scan", self.refresh)
        self.cancel_button = self.button("Cancel", self.cancel)
        self.path = label("No folder selected", "path")
        self.path.setTextInteractionFlags(Qt.TextSelectableByMouse | Qt.TextSelectableByKeyboard)
        self.path.setAccessibleName("Current folder path")
        self.path.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        for button in (self.back_button, self.up_button):
            nav.addWidget(button)
        nav.addWidget(self.path, 1)
        nav.addWidget(self.refresh_button)
        nav.addWidget(self.cancel_button)
        body.addLayout(nav)
        title_row = QHBoxLayout()
        self.title = label("Folder space", "title")
        self.title.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        title_row.addWidget(self.title, 1)
        self.stamp = label("Local · Recycle Bin", "stamp")
        title_row.addWidget(self.stamp)
        body.addLayout(title_row)
        summary = QHBoxLayout()
        self.total = label("—", "total")
        self.total.setAccessibleName("Measured logical size")
        self.summary = label("Choose a folder to begin.", "muted", True)
        summary.addWidget(self.total)
        summary.addSpacing(12)
        summary.addWidget(self.summary, 1)
        self.tabs = []
        for text, mode in (("Folders", "folders"), ("Files", "files")):
            tab = self.button(text, lambda checked=False, mode=mode: self.change_mode(mode))
            tab.setObjectName("tab")
            tab.setCheckable(True)
            tab.setAccessibleName(f"Show {text.lower()}")
            self.tabs.append(tab)
            summary.addWidget(tab)
        body.addLayout(summary)
        notice_row = QHBoxLayout()
        self.notice = label("", "notice", True)
        self.notice.setAccessibleName("Scan status")
        self.notice.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        self.retry_button = self.button("Retry scan", self.refresh)
        self.issues_button = self.button("Scan details", self.show_issues)
        notice_row.addWidget(self.notice, 1)
        notice_row.addWidget(self.retry_button)
        notice_row.addWidget(self.issues_button)
        body.addLayout(notice_row)
        self.content = QSplitter(Qt.Horizontal)
        self.content.setHandleWidth(16)
        overview = QWidget()
        self.overview = overview
        overview_layout = QVBoxLayout(overview)
        overview_layout.setContentsMargins(0, 0, 0, 0)
        overview_layout.setSpacing(12)
        map_header = QHBoxLayout()
        map_header.addWidget(label("Space map", "section"))
        map_header.addStretch()
        self.expand_button = self.button("Expand space map", self.expand_map)
        map_header.addWidget(self.expand_button)
        overview_layout.addLayout(map_header)
        self.map_caption = label("Largest 20 + Other · Logical size", "muted", True)
        overview_layout.addWidget(self.map_caption)
        self.map = SpaceMap()
        self.map.chosen.connect(self.open_map_entry)
        overview_layout.addWidget(self.map, 1)
        map_tools = QHBoxLayout()
        self.map_zoom = label("100%", "muted")
        self.map.zoomChanged.connect(lambda zoom: self.map_zoom.setText(f"{zoom*100:.0f}%"))
        map_tools.addWidget(label("Scroll to zoom · Drag to pan", "footer"))
        map_tools.addStretch()
        map_tools.addWidget(self.map_zoom)
        map_tools.addWidget(self.button("Fit", self.map.reset_zoom))
        overview_layout.addLayout(map_tools)
        overview_layout.addWidget(label("Largest items", "section"))
        self.rank_scroll = QScrollArea()
        self.rank_scroll.setAccessibleName("Scrollable largest items ranking")
        self.rank_scroll.setWidgetResizable(True)
        self.rank_scroll.verticalScrollBar().setObjectName("rankingScroll")
        self.rank_scroll.setViewportMargins(0, 0, self.s(12), 0)
        self.ranks = RankBars()
        self.ranks.chosen.connect(self.open_entry)
        self.rank_scroll.setWidget(self.ranks)
        self.rank_scroll.setMinimumHeight(108)
        overview_layout.addWidget(self.rank_scroll, 1)
        self.content.addWidget(overview)
        table_panel = QWidget()
        table_panel.setObjectName("breakdownPanel")
        self.table_panel = table_panel
        table_layout = QVBoxLayout(table_panel)
        table_layout.setContentsMargins(16, 16, 16, 16)
        table_layout.setSpacing(12)
        table_heading = QHBoxLayout()
        table_heading.addWidget(label("Breakdown", "section"))
        table_heading.addStretch()
        self.count = label("All items", "muted")
        table_heading.addWidget(self.count)
        table_layout.addLayout(table_heading)
        self.table_model = BreakdownModel(self)
        self.table_model.icons = {True: self.style().standardIcon(QStyle.SP_DirIcon), False: self.style().standardIcon(QStyle.SP_FileIcon)}
        self.table = QTableView()
        self.table.setAccessibleName("Complete file and folder breakdown")
        self.table.setAccessibleDescription("All direct items in the selected view, sorted largest first. Enter opens folders; Ctrl+C copies the selected path.")
        self.table.setVerticalScrollMode(QTableView.ScrollPerItem)
        self.table.setModel(self.table_model)
        self.table.setSelectionBehavior(QTableView.SelectRows)
        self.table.setSelectionMode(QTableView.SingleSelection)
        self.table.setEditTriggers(QTableView.NoEditTriggers)
        self.table.setShowGrid(False)
        self.table.setWordWrap(False)
        self.table.setTextElideMode(Qt.ElideMiddle)
        self.table.verticalHeader().hide()
        self.table.verticalHeader().setDefaultSectionSize(44)
        self.table.horizontalHeader().setStretchLastSection(False)
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.Stretch)
        for column, width in ((1, 110), (2, 70), (3, 106)):
            self.table.horizontalHeader().setSectionResizeMode(column, QHeaderView.Fixed)
            self.table.setColumnWidth(column, width)
        self.table.horizontalHeader().setMinimumSectionSize(80)
        self.table.activated.connect(self.open_table)
        copy = QAction("Copy path", self.table)
        copy.setShortcut(QKeySequence.Copy)
        copy.setShortcutContext(Qt.WidgetWithChildrenShortcut)
        copy.triggered.connect(self.copy_selected)
        self.table.addAction(copy)
        self.table.selectionModel().currentChanged.connect(self.show_selected)
        self.table_stack = QStackedWidget()
        self.table_stack.addWidget(self.table)
        self.table_empty = label("Choose a folder to see its contents.", "muted", True)
        self.table_empty.setAlignment(Qt.AlignCenter)
        self.table_stack.addWidget(self.table_empty)
        table_layout.addWidget(self.table_stack, 1)
        self.selected = label("Enter opens a folder · Ctrl+C copies its path", "footer", True)
        self.selected.setTextInteractionFlags(Qt.TextSelectableByMouse | Qt.TextSelectableByKeyboard)
        selection_row = QHBoxLayout()
        self.selected.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        selection_row.addWidget(self.selected, 1)
        self.recycle_action = make_recycle_action(self, self.table, self.table_model)
        self.recycle_button = QToolButton()
        self.recycle_button.setDefaultAction(self.recycle_action)
        self.recycle_button.setAccessibleName("Move selected item to Recycle Bin")
        selection_row.addWidget(self.recycle_button)
        table_layout.addLayout(selection_row)
        self.content.addWidget(table_panel)
        self.content.setSizes([340, 620])
        self.content.setStretchFactor(1, 2)
        self.content.setChildrenCollapsible(False)
        self.compact_tabs = QTabWidget()
        self.compact_tabs.setAccessibleName("Workspace views")
        self.overview_scroll = QScrollArea()
        self.overview_scroll.setAccessibleName("Scrollable space overview")
        self.overview_scroll.setWidgetResizable(True)
        self.workspace = QStackedWidget()
        self.workspace.addWidget(self.content)
        self.workspace.addWidget(self.compact_tabs)
        self.loading_panel = LoadingPanel(self)
        self.workspace.addWidget(self.loading_panel)
        self._compact = False
        body.addWidget(self.workspace, 1)
        body.addWidget(label("Logical file size, not allocated space. Snapshots update when you refresh.", "footer", True))
        self.horizontal.addWidget(sheet)
        self.horizontal.setSizes([270, 1060])
        self.horizontal.setStretchFactor(1, 1)
        self.horizontal.setChildrenCollapsible(False)
        outer.addWidget(self.horizontal, 1)
        self.setCentralWidget(chrome)

    def _shortcuts(self):
        for keys, callback in (("Ctrl+O", self.choose_folder), ("Alt+Left", self.go_back),
                               ("Alt+Up", self.go_up), ("F5", self.refresh), ("Escape", self.cancel),
                               ("Ctrl+1", lambda: self.change_mode("folders")),
                               ("Ctrl+2", lambda: self.change_mode("files")), ("F1", self.show_help)):
            action = QAction(self)
            action.setShortcut(QKeySequence(keys))
            action.triggered.connect(callback)
            self.addAction(action)

    def s(self, value):
        return round(value * self.ui_scale)

    def _scale_layout(self):
        def scale_layout(layout):
            margins = layout.contentsMargins()
            layout.setContentsMargins(*(self.s(value) for value in (margins.left(), margins.top(), margins.right(), margins.bottom())))
            if layout.spacing() >= 0:
                layout.setSpacing(self.s(layout.spacing()))
            for i in range(layout.count()):
                if layout.itemAt(i).layout():
                    scale_layout(layout.itemAt(i).layout())
        for widget in [self.centralWidget()] + self.centralWidget().findChildren(QWidget):
            if widget.layout():
                scale_layout(widget.layout())
            minimum = widget.minimumSize()
            widget.setMinimumSize(self.s(minimum.width()), self.s(minimum.height()))
        self.setMinimumSize(self.s(800), self.s(600))
        self.tree.setIndentation(self.s(16))
        self.tree.setColumnWidth(1, self.s(98))
        self.table.verticalHeader().setDefaultSectionSize(self.s(44))
        for column, width in ((1, 110), (2, 80), (3, 106)):
            self.table.setColumnWidth(column, self.s(width))
        self.table.horizontalHeader().setMinimumSectionSize(self.s(80))
        self.horizontal.setHandleWidth(self.s(8))
        self.content.setHandleWidth(self.s(16))
        self.horizontal.setSizes([self.s(270), max(1, self.width() - self.s(270))])

    def set_dark_mode(self, dark):
        if self.dark == dark:
            return
        previous = self.centralWidget().grab() if self.isVisible() and not reduced_motion() else None
        self.dark = dark
        if self.settings:
            self.settings.setValue("darkMode", dark)
        apply_style(QApplication.instance(), dark, self.ui_scale)
        self.light_button.setChecked(not dark)
        self.dark_button.setChecked(dark)
        self.setWindowIcon(mark_icon())
        self.mark_label.setPixmap(self.windowIcon().pixmap(QSize(self.s(26), self.s(26))))
        self.map.update()
        self.ranks.update()
        if self.table_model.rowCount():
            self.table_model.dataChanged.emit(self.table_model.index(0, 0), self.table_model.index(self.table_model.rowCount()-1, 3))
        if previous is not None:
            if self._theme_transition is not None:
                self._theme_transition.animation.stop()
                self._theme_transition.hide()
                self._theme_transition.deleteLater()
            self._theme_transition = SnapshotTransition(self.centralWidget(), previous, duration=150)
            self._theme_transition.animation.finished.connect(lambda: setattr(self, "_theme_transition", None))

    def set_scale_percent(self, value):
        value = max(85, min(150, int(value)))
        if self.scale_panel is not None:
            self.scale_panel.sync(value)
        if round(self.ui_scale * 100) == value:
            return
        if self._theme_transition is not None:
            self._theme_transition.animation.stop()
            self._theme_transition.deleteLater()
            self._theme_transition = None
        self.ui_scale = value / 100
        if self.settings:
            self.settings.setValue("uiScale", value)
        current = self.table.currentIndex()
        old_models = (self.folder_model, self.table_model)
        selected_path = self.table_model.entries[current.row()].path if current.isValid() else None
        apply_style(QApplication.instance(), self.dark, self.ui_scale)
        old = self.takeCentralWidget()
        self._shown_snapshot = self._shown_location = self._shown_state = None
        self._ui()
        self._scale_layout()
        self._responsive(self.width())
        self.render()
        if selected_path:
            row = next((i for i, entry in enumerate(self.table_model.entries) if entry.path == selected_path), -1)
            if row >= 0:
                self.table.setCurrentIndex(self.table_model.index(row, 0))
                self.table.scrollTo(self.table_model.index(row, 0))
        self.scale_button.setFocus()
        if self.scale_panel is not None:
            self.scale_panel.sync(value)
        old.deleteLater()
        for model in old_models:
            model.deleteLater()

    def show_scale_panel(self):
        if self.scale_panel is None:
            self.scale_panel = ScalePanel(self)
        if self.scale_panel.isVisible():
            self.scale_panel.hide()
        else:
            self.scale_panel.open_at(self.scale_button)

    def set_reduced_motion(self, enabled):
        self.reduce_motion = enabled
        QApplication.instance().setProperty("reduceMotion", enabled)
        if self.settings:
            self.settings.setValue("reduceMotion", enabled)
        if enabled:
            self.map.zoom_to(self.map.target_zoom, animated=False)
        if self.loading_panel.isVisible():
            self.loading_panel.timer.setInterval(1000 if enabled else 40)

    def expand_map(self):
        if self.recycle_job is not None or self.session.state != "ready" or not self.session.listing:
            return
        if self.explorer is not None:
            self.explorer.raise_()
            return
        self.explorer = MapExplorer(self)
        self.explorer.finished.connect(lambda _: setattr(self, "explorer", None))
        self.explorer.present()

    def open_map_entry(self, entry):
        if not entry.path:
            self.expand_map()
            return
        self.open_entry(entry)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        if hasattr(self, "sidebar"):
            self._responsive(event.size().width())

    def _responsive(self, width):
        narrow = width < self.s(1120)
        self.sidebar.setVisible(not narrow)
        if narrow != self._compact:
            self._compact = narrow
            if narrow:
                self.map.setMinimumHeight(self.s(100))
                self.rank_scroll.setMinimumHeight(self.s(72))
                self.overview_scroll.setWidget(self.overview)
                self.compact_tabs.addTab(self.overview_scroll, "Space overview")
                self.compact_tabs.addTab(self.table_panel, "Breakdown")
                self.workspace.setCurrentWidget(self.compact_tabs)
            else:
                self.map.setMinimumHeight(self.s(150))
                self.rank_scroll.setMinimumHeight(self.s(108))
                while self.compact_tabs.count():
                    self.compact_tabs.removeTab(0)
                self.overview_scroll.takeWidget()
                self.content.addWidget(self.overview)
                self.content.addWidget(self.table_panel)
                self.content.setSizes([340, 620])
                self.workspace.setCurrentWidget(self.content)
                self.overview.show()
                self.table_panel.show()
        if hasattr(self, "loading_panel") and self.session.state != "ready":
            self.workspace.setCurrentWidget(self.loading_panel)

    def choose_folder(self):
        if self.recycle_job is not None:
            return
        path = QFileDialog.getExistingDirectory(self, "Choose a folder to inspect", self.session.root)
        if path:
            self._expanded.clear()
            self.session.start(path)
            self.render()

    def refresh(self):
        if self.recycle_job is not None:
            return
        if self.session.root:
            self.session.start(self.session.root, refresh=True)
            self.render()

    def cancel(self):
        if self.session.state == "loading":
            self.session.cancel()
            self.render()

    def change_mode(self, mode):
        if self.mode != mode:
            self.mode = mode
            self.render()

    def go_back(self):
        if self.recycle_job is not None:
            return
        self.session.back()
        self.render()

    def go_up(self):
        if self.recycle_job is not None:
            return
        self.session.up()
        self.render()

    def open_entry(self, entry):
        if self.session.state != "ready" or self.recycle_job is not None:
            return
        if entry.is_dir:
            if self.session.navigate(entry.path):
                self.render()
        else:
            row = next((i for i, e in enumerate(self.table_model.entries) if e.path == entry.path), -1)
            if row >= 0:
                self.table.setCurrentIndex(self.table_model.index(row, 0))
                self.table.scrollTo(self.table_model.index(row, 0))
                self.table.setFocus()

    def open_tree(self, index):
        if index.isValid():
            self.open_entry(index.internalPointer().entry)

    def open_table(self, index):
        if index.isValid():
            self.open_entry(self.table_model.entries[index.row()])

    def show_selected(self, index, previous=None):
        if index.isValid() and index.row() < len(self.table_model.entries):
            entry = self.table_model.entries[index.row()]
            exact = "Unknown size" if entry.size is None else f"{entry.size:,} bytes" + (" measured · Partial" if entry.partial else "")
            self.selected.setText(f"{entry.name} · {exact}")
            self.selected.setToolTip(entry.path)

    def copy_selected(self):
        index = self.table.currentIndex()
        if index.isValid():
            QApplication.clipboard().setText(self.table_model.entries[index.row()].path)
            self.selected.setText("Path copied to clipboard")

    def recycle_selected(self, table, model):
        index = table.currentIndex()
        if self.recycle_job is not None or self.session.state != "ready" or not index.isValid() or index.row() >= model.rowCount():
            return
        entry = model.entries[index.row()]
        if not self.session.listing or entry not in self.session.listing.entries:
            return
        parent = self.explorer or self
        root = self.session.root
        try:
            identity = target_identity(entry.path, root)
        except (OSError, ValueError) as error:
            show_recycle_error(parent, str(error))
            return
        if not confirm_recycle(parent, entry):
            return
        if root != self.session.root or self.session.state != "ready":
            return
        self.recycle_job = RecycleJob(entry.path, root, identity, self)
        self.recycle_job.finished.connect(self._recycle_finished)
        self.render()
        self.recycle_job.start()

    def _recycle_finished(self):
        job = self.recycle_job
        self.recycle_job = None
        error, name = job.error, os.path.basename(job.path)
        job.deleteLater()
        if self.explorer is not None:
            self.explorer.animate_exit = False
            self.explorer.reject()
        self.refresh()
        if error:
            show_recycle_error(self, error)
        else:
            self.session.message = f"Moved {name} to Recycle Bin. Refreshing measurements…"
            self.render()

    def _poll(self):
        if self.session.poll():
            self.render()

    def _sync_tree(self):
        session = self.session
        snapshot = session.snapshot if session.state == "ready" else None
        if snapshot is not self._shown_snapshot:
            self.folder_model.set_snapshot(snapshot)
            self._shown_snapshot = snapshot
            if snapshot:
                for path in self._expanded | {snapshot.root}:
                    index = self.folder_model.index_for_path(path)
                    if index.isValid():
                        self.tree.expand(index)
        if snapshot and session.current != self._shown_location:
            index = self.folder_model.index_for_path(session.current)
            if index.isValid():
                ancestor = index.parent()
                while ancestor.isValid():
                    self.tree.expand(ancestor)
                    ancestor = ancestor.parent()
                self.tree.setCurrentIndex(index)
                self.tree.scrollTo(index)
        self._shown_location = session.current if snapshot else None

    def render(self):
        session = self.session
        ready = session.state == "ready"
        loading = session.state == "loading"
        self.back_button.setEnabled(ready and session.history_index > 0)
        self.up_button.setEnabled(ready and session.current != session.root)
        self.refresh_button.setEnabled(bool(session.root))
        self.cancel_button.setVisible(False)
        self.expand_button.setEnabled(ready and self.session.listing is not None)
        self.retry_button.setVisible(session.state in ("error", "cancelled"))
        self.tabs[0].setChecked(self.mode == "folders")
        self.tabs[1].setChecked(self.mode == "files")
        self.path.setText(session.current or "No folder selected")
        self.path.setToolTip(session.current)
        self.title.setText(os.path.basename(session.current) or session.current or "Folder space")
        self.title.setToolTip(session.current)
        listing = session.listing
        partial = bool(listing and listing.errors)
        failed = bool(listing and listing.error)
        self.issues_button.setVisible(bool(ready and session.snapshot and session.snapshot.issues))
        self.retry_button.setVisible(partial)
        self._sync_tree()
        # Progress updates only change status text, never reset virtual views or focus.
        view_key = (id(session.snapshot), session.state, session.current, self.mode)
        rebuild = view_key != self._shown_state
        self._shown_state = view_key
        if ready and listing:
            shown = [e for e in listing.entries if e.is_dir == (self.mode == "folders")]
            self.total.setText("Unknown" if failed and not listing.entries else size_text(listing.total) + (" +" if partial else ""))
            self.summary.setText(f"{len(listing.entries):,} direct items · Folder total" + (" · Partial measurement" if partial else ""))
            self.stamp.setText(f"Measured {session.snapshot.measured_at:%H:%M:%S}")
            self.stamp.setToolTip(session.snapshot.measured_at.isoformat())
            links = session.snapshot.links_skipped
            self.notice.setText(session.message or (f"Partial scan: {listing.errors:,} read failure(s). Sizes marked + are lower bounds; shares are unavailable." if partial else
                f"Snapshot complete · {links:,} link(s) excluded · Refresh to measure changes."))
            self.notice.setProperty("warning", partial)
            self.count.setText(f"All {len(shown):,} {self.mode}")
            positive = sum(bool(entry.size) for entry in shown)
            self.map_caption.setText((f"Largest 20 + Other" if positive > 20 else f"{positive:,} measured {self.mode}") + (" · Partial" if partial else ""))
            if rebuild:
                current = self.table.currentIndex()
                selected_path = self.table_model.entries[current.row()].path if current.isValid() and current.row() < len(self.table_model.entries) else None
                scroll = self.table.verticalScrollBar().value()
                self.table_model.set_entries(shown, listing.total, partial)
                row = next((i for i, entry in enumerate(shown) if entry.path == selected_path), -1)
                if row >= 0:
                    self.table.setCurrentIndex(self.table_model.index(row, 0))
                    self.table.verticalScrollBar().setValue(scroll)
                self.map.set_entries(shown)
                self.ranks.set_entries(shown)
                message = listing.error or f"No {self.mode} in this folder."
                self.table_empty.setText(message)
                self.table_stack.setCurrentIndex(0 if shown else 1)
                self.selected.setText("Enter opens a folder · Ctrl+C copies its path")
        else:
            self.total.setText("Scanning…" if loading else "—")
            self.summary.setText("Building a complete folder snapshot" if loading else
                                 "Scan failed" if session.state == "error" else "Scan cancelled" if session.state == "cancelled" else "Choose a folder to begin.")
            self.stamp.setText("Measuring…" if loading else "Local · Recycle Bin")
            self.notice.setText("Couldn’t read this folder. Restore access or choose another folder." if session.state == "error" else session.message)
            self.notice.setToolTip(session.message)
            self.notice.setProperty("warning", session.state == "error")
            self.count.setText(f"{self.mode.capitalize()}")
            if rebuild:
                self.table_model.set_entries([])
                message = "Scan in progress. You can cancel or choose another folder." if loading else session.message
                self.table_empty.setText(message)
                self.table_stack.setCurrentIndex(1)
                self.map.set_entries([], "Measuring this folder…" if loading else "Choose a folder to build its space map.")
                self.ranks.set_entries([])
                self.selected.setText("")
        self.notice.style().unpolish(self.notice)
        self.notice.style().polish(self.notice)
        self.path.setAccessibleName(f"Current folder: {session.current or 'None selected'}")
        self.total.setAccessibleName(f"Measured logical size: {self.total.text()}")
        self.notice.setAccessibleName(self.notice.text())
        busy = self.recycle_job is not None
        self.expand_button.setEnabled(ready and not busy and self.session.listing is not None)
        self.choose_button.setEnabled(not busy)
        self.refresh_button.setEnabled(bool(session.root) and not busy)
        self.table.setEnabled(not busy)
        self.tree.setEnabled(not busy)
        if busy:
            self.back_button.setEnabled(False)
            self.up_button.setEnabled(False)
            self.notice.setText("Moving to Recycle Bin… Keep Space Lens open until this finishes.")
            self.notice.setAccessibleName(self.notice.text())
        index = self.table.currentIndex()
        self.recycle_action.setEnabled(ready and not busy and index.isValid() and index.row() < self.table_model.rowCount())
        if not ready:
            self.loading_panel.set_state(session.state)
            if loading:
                self.loading_panel.update_progress(session.progress)
            self.workspace.setCurrentWidget(self.loading_panel)
        else:
            self.workspace.setCurrentWidget(self.compact_tabs if self._compact else self.content)
        if self.explorer is not None:
            self.explorer.refresh()

    def show_help(self):
        dialog = QDialog(self)
        dialog.setWindowTitle("Keyboard shortcuts · Space Lens v3")
        layout = QVBoxLayout(dialog)
        layout.setContentsMargins(*(self.s(24) for _ in range(4)))
        layout.setSpacing(self.s(16))
        layout.addWidget(label("Keyboard shortcuts", "section"))
        layout.addWidget(label("Ctrl+O     Choose folder\nAlt+Left   Back\nAlt+Up     Parent folder\nF5         Refresh the selected root\nEscape     Cancel an active scan\nCtrl+1     Show folders\nCtrl+2     Show files\nEnter      Open the focused folder\nCtrl+C     Copy the selected table row’s path\nF1         Keyboard shortcuts", wrap=True))
        layout.addWidget(label("Use Tab to move between controls. Arrow keys select rows and expand the hierarchy.\nSpace map: scroll to zoom at the pointer, drag to pan, +/− to zoom, 0 to fit. Escape exits the expanded map.", "muted", True))
        buttons = QDialogButtonBox(QDialogButtonBox.Close)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)
        dialog.resize(self.s(460), self.s(420))
        dialog.exec()

    def show_issues(self):
        snapshot = self.session.snapshot
        if not snapshot:
            return
        dialog = QDialog(self)
        dialog.setWindowTitle("Scan details · Space Lens v3")
        layout = QVBoxLayout(dialog)
        layout.setContentsMargins(*(self.s(24) for _ in range(4)))
        layout.setSpacing(self.s(16))
        layout.addWidget(label("Some locations could not be measured", "section", True))
        layout.addWidget(label("Refresh the scan after access is restored. Up to 100 failure details are retained.", "muted", True))
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        details = label("\n\n".join(f"{path}\n{error}" for path, error in snapshot.issues), wrap=True)
        details.setAlignment(Qt.AlignLeft | Qt.AlignTop)
        details.setTextInteractionFlags(Qt.TextSelectableByMouse | Qt.TextSelectableByKeyboard)
        scroll.setWidget(details)
        layout.addWidget(scroll, 1)
        buttons = QDialogButtonBox(QDialogButtonBox.Close)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)
        dialog.resize(self.s(680), self.s(440))
        dialog.exec()

    def closeEvent(self, event):
        if self.recycle_job is not None:
            event.ignore()
            return
        self.timer.stop()
        self.session.close()
        super().closeEvent(event)


def apply_style(app, dark=False, scale=1.0):
    app.setProperty("darkMode", dark)
    app.setProperty("uiScale", scale)
    app.setStyle("Fusion")
    font = QFont("Segoe UI")
    font.setPointSizeF(10 * scale)
    app.setFont(font)
    palette = QPalette()
    for role, color in ((QPalette.Window, "#f3f7f7"), (QPalette.WindowText, "#131416"),
                        (QPalette.Base, "#ffffff"), (QPalette.AlternateBase, "#f4f6f8"),
                        (QPalette.Text, "#131416"), (QPalette.Button, "#f4f6f8"),
                        (QPalette.ButtonText, "#131416"), (QPalette.Highlight, "#e5eeee"),
                        (QPalette.HighlightedText, "#131416"), (QPalette.PlaceholderText, "#5d5f60")):
        palette.setColor(role, QColor(DARK_COLORS.get(color, color) if dark else color))
    palette.setColor(QPalette.Disabled, QPalette.Text, QColor("#b0b0b7" if dark else "#65686b"))
    palette.setColor(QPalette.Disabled, QPalette.ButtonText, QColor("#b0b0b7" if dark else "#65686b"))
    app.setPalette(palette)
    stylesheet = re.sub(r"#[0-9a-fA-F]{6}", lambda match: DARK_COLORS.get(match[0].lower(), match[0]) if dark else match[0], STYLE)
    if dark:
        stylesheet += DARK_STYLE
    stylesheet = re.sub(r"(\d+)px", lambda match: f"{max(1, round(int(match[1]) * scale))}px", stylesheet)
    app.setStyleSheet(stylesheet)


def main():
    app = QApplication(sys.argv)
    try:
        if not ensure_elevated():
            return 0
    except OSError as error:
        show_recycle_error(None, str(error), "Administrator access required")
        return 1
    app.setApplicationName("Space Lens v3")
    app.setApplicationVersion("3.3.0")
    app.setOrganizationName("Space Lens")
    apply_style(app)
    window = MainWindow()
    if len(sys.argv) > 1:
        window.session.start(sys.argv[1])
        window.render()
    window.show()
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())

