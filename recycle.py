"""Confirmed, single-item recycling through Qt's native Windows implementation."""
import os

from PySide6.QtCore import QFile, QThread, Qt
from PySide6.QtGui import QAction, QKeySequence
from PySide6.QtWidgets import QMessageBox

from scanner import is_link, normalize, within


def target_identity(path, root):
    path, root = normalize(path), normalize(root)
    if os.path.normcase(path) == os.path.normcase(root) or not within(path, root):
        raise ValueError("Select an item inside the scanned folder. The scan root cannot be recycled.")
    current = path
    while True:
        info = os.lstat(current)
        if is_link(info):
            raise ValueError("This path contains a link or junction. Refresh and choose a regular item.")
        if os.path.normcase(current) == os.path.normcase(root):
            break
        current = os.path.dirname(current)
    info = os.lstat(path)
    return info.st_dev, info.st_ino, info.st_ctime_ns


def confirm_recycle(parent, entry):
    box = QMessageBox(parent)
    box.setWindowTitle("Move to Recycle Bin")
    box.setIcon(QMessageBox.Question)
    box.setTextFormat(Qt.PlainText)
    box.setText(f'Move “{entry.name}” to the Recycle Bin?')
    box.setInformativeText(entry.path + ("\n\nThe folder and everything inside it will be moved." if entry.is_dir else "")
                           + "\n\nRestore it using Windows Recycle Bin. If recycling is unavailable, the move will fail.")
    accept = box.addButton("Move to Recycle Bin", QMessageBox.AcceptRole)
    cancel = box.addButton(QMessageBox.Cancel)
    box.setDefaultButton(cancel)
    box.setEscapeButton(cancel)
    box.exec()
    return box.clickedButton() is accept


def show_recycle_error(parent, text, title="Couldn’t move to Recycle Bin"):
    box = QMessageBox(QMessageBox.Warning, title, "", QMessageBox.Ok, parent)
    box.setTextFormat(Qt.PlainText)
    box.setText(text)
    box.exec()


def make_recycle_action(owner, table, model):
    action = QAction("Move to Recycle Bin…", table)
    action.setShortcut(QKeySequence(Qt.Key_Delete))
    action.setShortcutContext(Qt.WidgetWithChildrenShortcut)
    action.setEnabled(False)
    action.triggered.connect(lambda: owner.recycle_selected(table, model))
    separator = QAction(table)
    separator.setSeparator(True)
    table.addAction(separator)
    table.addAction(action)
    table.setContextMenuPolicy(Qt.ActionsContextMenu)
    def update():
        index = table.currentIndex()
        action.setEnabled(owner.session.state == "ready" and owner.recycle_job is None
                          and index.isValid() and index.row() < model.rowCount())
    table.selectionModel().currentChanged.connect(lambda *_: update())
    model.modelReset.connect(update)
    return action


class RecycleJob(QThread):
    def __init__(self, path, root, identity, parent=None):
        super().__init__(parent)
        self.path, self.root, self.identity = path, root, identity
        self.error = ""

    def run(self):
        try:
            if target_identity(self.path, self.root) != self.identity:
                raise ValueError("The selected item changed after confirmation. Refresh and try again.")
            file = QFile(self.path)
            # Qt 6.11's Windows progress sink vetoes permanent deletion.
            if not file.moveToTrash():
                raise OSError(file.errorString() or "Windows could not recycle this item.")
        except Exception as error:
            self.error = str(error)
