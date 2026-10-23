"""Virtual Qt views: complete file rows and a folder hierarchy expanded on demand."""
from __future__ import annotations

import os
from dataclasses import dataclass, field

from PySide6.QtCore import QAbstractItemModel, QAbstractTableModel, QModelIndex, Qt
from PySide6.QtGui import QColor, QGuiApplication, QIcon

from scanner import Entry, Snapshot, size_text

DISPLAY, ALIGN, TOOLTIP = Qt.DisplayRole, Qt.TextAlignmentRole, Qt.ToolTipRole


class BreakdownModel(QAbstractTableModel):
    headers = ("Name", "Logical size", "Of folder", "Measurement")

    def __init__(self, parent=None):
        super().__init__(parent)
        self.entries: list[Entry] = []
        self.total = 0
        self.partial = False
        self.icons: dict[bool, QIcon] = {}

    def set_entries(self, entries, total=0, partial=False):
        self.beginResetModel()
        self.entries, self.total, self.partial = list(entries), total, partial
        self.endResetModel()

    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.entries)

    def columnCount(self, parent=QModelIndex()):
        return len(self.headers)

    def headerData(self, section, orientation, role=DISPLAY):
        if orientation == Qt.Horizontal and role == DISPLAY:
            return self.headers[section]
        if orientation == Qt.Horizontal and role == ALIGN:
            return int(Qt.AlignRight | Qt.AlignVCenter) if section in (1, 2) else int(Qt.AlignLeft | Qt.AlignVCenter)

    def data(self, index, role=DISPLAY):
        if not index.isValid():
            return None
        entry, column = self.entries[index.row()], index.column()
        if role in (DISPLAY, Qt.AccessibleTextRole):
            return (entry.name, size_text(entry.size) + (" +" if entry.partial and entry.size is not None else ""),
                    "—" if entry.size is None or self.partial else f"{entry.size / self.total * 100:.1f}%" if self.total else "0.0%",
                    "Unreadable" if entry.size is None else "Partial" if entry.partial else "Measured")[column]
        if role in (TOOLTIP, Qt.AccessibleDescriptionRole):
            exact = "Size unavailable" if entry.size is None else f"{entry.size:,} bytes" + (" measured; incomplete" if entry.partial else "")
            return f"{entry.path}\n{exact}"
        if role == ALIGN:
            return int(Qt.AlignRight | Qt.AlignVCenter) if column in (1, 2) else int(Qt.AlignLeft | Qt.AlignVCenter)
        if role == Qt.DecorationRole and column == 0:
            return self.icons.get(entry.is_dir)
        if role == Qt.ForegroundRole and column == 3:
            dark = bool(QGuiApplication.instance().property("darkMode"))
            return QColor(("#edc887" if dark else "#92400e") if entry.partial else ("#acb8c6" if dark else "#5d5f60"))


@dataclass(slots=True)
class FolderNode:
    entry: Entry
    parent: FolderNode | None
    row: int
    entries: list[Entry] | None = None
    children: dict[int, FolderNode] = field(default_factory=dict)


class FolderModel(QAbstractItemModel):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.snapshot: Snapshot | None = None
        self.root: FolderNode | None = None

    def set_snapshot(self, snapshot):
        self.beginResetModel()
        self.snapshot = snapshot
        if snapshot:
            listing = snapshot.listings[snapshot.root]
            self.root = FolderNode(Entry(snapshot.root, os.path.basename(snapshot.root) or snapshot.root,
                                         listing.total, True, bool(listing.errors)), None, 0)
        else:
            self.root = None
        self.endResetModel()

    def children(self, node):
        if node.entries is None:
            listing = self.snapshot.listings.get(node.entry.path)
            node.entries = [e for e in listing.entries if e.is_dir] if listing else []
        return node.entries

    def rowCount(self, parent=QModelIndex()):
        if not self.root or parent.column() > 0:
            return 0
        return len(self.children(parent.internalPointer())) if parent.isValid() else 1

    def columnCount(self, parent=QModelIndex()):
        return 2

    def index(self, row, column, parent=QModelIndex()):
        if not self.root or row < 0 or column < 0 or column >= 2:
            return QModelIndex()
        if not parent.isValid():
            return self.createIndex(0, column, self.root) if row == 0 else QModelIndex()
        node = parent.internalPointer()
        entries = self.children(node)
        if row >= len(entries):
            return QModelIndex()
        if row not in node.children:
            node.children[row] = FolderNode(entries[row], node, row)
        return self.createIndex(row, column, node.children[row])

    def parent(self, index):
        if not index.isValid():
            return QModelIndex()
        node = index.internalPointer().parent
        return self.createIndex(node.row, 0, node) if node else QModelIndex()

    def data(self, index, role=DISPLAY):
        if not index.isValid():
            return None
        entry = index.internalPointer().entry
        if role in (DISPLAY, Qt.AccessibleTextRole):
            return entry.name if index.column() == 0 else size_text(entry.size) + (" +" if entry.partial and entry.size is not None else "")
        if role in (TOOLTIP, Qt.AccessibleDescriptionRole):
            return entry.path + ("\nMeasurement incomplete. Refresh to retry." if entry.partial else "")
        if role == ALIGN and index.column() == 1:
            return int(Qt.AlignRight | Qt.AlignVCenter)

    def headerData(self, section, orientation, role=DISPLAY):
        if orientation == Qt.Horizontal and role == DISPLAY:
            return ("Folder", "Logical size")[section]
        if orientation == Qt.Horizontal and role == ALIGN:
            return int((Qt.AlignRight if section else Qt.AlignLeft) | Qt.AlignVCenter)

    def index_for_path(self, path):
        if not self.root or not self.snapshot or path not in self.snapshot.listings:
            return QModelIndex()
        parts = []
        while path != self.snapshot.root:
            parts.append(path)
            parent = os.path.dirname(path)
            if parent == path:
                return QModelIndex()
            path = parent
        index = self.index(0, 0)
        for target in reversed(parts):
            entries = self.children(index.internalPointer())
            row = next((i for i, entry in enumerate(entries) if entry.path == target), -1)
            if row < 0:
                return QModelIndex()
            index = self.index(row, 0, index)
        return index
