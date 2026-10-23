"""One-hue map and ranking. The standard table is their accessible equivalent."""
from __future__ import annotations

import math
import time

from PySide6.QtCore import QPointF, QRectF, Qt, QTimer, Signal
from PySide6.QtGui import QColor, QPainter, QTransform
from PySide6.QtWidgets import QApplication, QWidget, QToolTip

from scanner import Entry, size_text
from motion import SnapshotTransition, reduced_motion, spring_step

DARK_TILE_COLORS = ("#29434d", "#335661", "#3e6570", "#263c49")
DARK_MAP_TEXT = ("#ecf4f7", "#c8dfe5")

def map_entries(entries, limit=20):
    positive = [e for e in entries if e.size and e.size > 0]
    if limit is None:
        return positive
    visible = positive[:limit]
    if len(positive) > limit:
        visible = visible + [Entry("", f"Other {len(positive) - limit:,} items", sum(e.size for e in positive[limit:]), False)]
    return visible


def split_boxes(entries, rect):
    if not entries or rect.width() <= 0 or rect.height() <= 0:
        return []
    if len(entries) == 1:
        return [(rect, entries[0])]
    total = sum(e.size for e in entries)
    running, cut = 0, 1
    for i, entry in enumerate(entries[:-1], 1):
        running += entry.size
        cut = i
        if running >= total / 2:
            break
    fraction = sum(e.size for e in entries[:cut]) / total
    if rect.width() >= rect.height():
        width = rect.width() * fraction
        first = QRectF(rect.x(), rect.y(), width, rect.height())
        second = QRectF(rect.x() + width, rect.y(), rect.width() - width, rect.height())
    else:
        height = rect.height() * fraction
        first = QRectF(rect.x(), rect.y(), rect.width(), height)
        second = QRectF(rect.x(), rect.y() + height, rect.width(), rect.height() - height)
    return split_boxes(entries[:cut], first) + split_boxes(entries[cut:], second)


def pixel_rect(rect, ratio):
    """Snap shared edges identically, including fractional Windows display scales."""
    left, top, right, bottom = (round(value * ratio) / ratio for value in
                               (rect.left(), rect.top(), rect.right(), rect.bottom()))
    return QRectF(left, top, right-left, bottom-top)


def vertical_label(box, name_width, scale, was_vertical=False):
    # A little hysteresis keeps labels from flipping around the zoom threshold.
    aspect = 1.35 if was_vertical else 1.7
    width_limit = max(name_width + 16*scale, 116*scale) + (12*scale if was_vertical else 0)
    return box.height() > box.width()*aspect and box.width() < width_limit


class SpaceMap(QWidget):
    chosen = Signal(object)
    zoomChanged = Signal(float)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.entries = []
        self.boxes = []
        self.message = "Choose a folder to build its space map."
        self.full = False
        self.zoom = self.target_zoom = 1.0
        self.velocity = 0.0
        self.offset = QPointF()
        self.anchor = self.scene_anchor = QPointF()
        self.press = None
        self.dragging = False
        self.last_chosen_rect = None
        self._layout_key = None
        self._transition = None
        self._label_angles = {}
        self._spring = QTimer(self)
        self._spring.setInterval(16)
        self._spring.timeout.connect(self._tick)
        self.setAttribute(Qt.WA_OpaquePaintEvent)
        self.setFocusPolicy(Qt.StrongFocus)
        self.setMinimumHeight(150)
        self.setMouseTracking(True)
        self.setAccessibleName("Space map")
        self.setAccessibleDescription("Scroll to zoom at the pointer. Drag to pan. Plus/minus zoom; zero fits. The Breakdown table contains every exact size.")

    def set_entries(self, entries, message="No measured bytes in this view.", origin=None):
        snapshot = self.grab() if origin is not None and self.isVisible() and not reduced_motion() else None
        if self._transition is not None:
            self._transition.animation.stop()
            self._transition.hide()
            self._transition.deleteLater()
            self._transition = None
        self.entries, self.message = map_entries(entries, None if self.full else 20), message
        self.boxes = []
        self._label_angles.clear()
        self._layout_key = None
        self.reset_zoom()
        self.update()
        if snapshot is not None:
            # Shrink the previous context away as the new folder grows from its clicked region.
            self._transition = SnapshotTransition(self, snapshot, duration=180)
            self._transition.animation.finished.connect(lambda: setattr(self, "_transition", None))
            self._incoming_origin = QRectF(origin)
            self._incoming_start = time.monotonic()
        else:
            self._incoming_origin = None

    def _layout(self):
        key = (self.width(), self.height(), id(self.entries))
        if self._layout_key != key:
            self.boxes = split_boxes(self.entries, QRectF(self.rect()).adjusted(1, 1, -1, -1))
            self._layout_key = key

    def transform(self):
        transform = QTransform()
        if getattr(self, "_incoming_origin", None) is not None:
            t = min(1.0, (time.monotonic() - self._incoming_start) / .22)
            t = 1 - (1-t)**3
            origin = self._incoming_origin
            transform.translate(origin.x()*(1-t), origin.y()*(1-t))
            transform.scale(origin.width()/max(1,self.width())*(1-t)+t, origin.height()/max(1,self.height())*(1-t)+t)
            if t >= .9999:
                self._incoming_origin = None
            else:
                QTimer.singleShot(16, self.update)
        transform.translate(self.offset.x(), self.offset.y())
        transform.scale(self.zoom, self.zoom)
        return transform

    def _clamp(self):
        self.offset.setX(min(0, max(self.width()*(1-self.zoom), self.offset.x())))
        self.offset.setY(min(0, max(self.height()*(1-self.zoom), self.offset.y())))

    def zoom_to(self, value, anchor=None, animated=True):
        QToolTip.hideText()
        if not self.entries:
            return
        self.anchor = QPointF(anchor if anchor is not None else self.rect().center())
        self.scene_anchor = (self.anchor - self.offset) / self.zoom
        self.target_zoom = max(1.0, min(12.0, value))
        if not animated or reduced_motion():
            self.zoom = self.target_zoom
            self.velocity = 0
            self.offset = self.anchor - self.scene_anchor*self.zoom
            self._clamp()
            self._spring.stop()
            self.zoomChanged.emit(self.zoom)
            self.update()
        else:
            if not self._spring.isActive():
                self._last_tick = time.monotonic()
                self._spring.start()

    def _tick(self):
        now = time.monotonic()
        self.zoom, self.velocity = spring_step(self.zoom, self.velocity, self.target_zoom, min(.05, now-self._last_tick))
        self._last_tick = now
        if abs(self.zoom-self.target_zoom) < .001 and abs(self.velocity) < .01:
            self.zoom = self.target_zoom
            self._spring.stop()
        self.offset = self.anchor - self.scene_anchor*self.zoom
        self._clamp()
        self.zoomChanged.emit(self.zoom)
        self.update()

    def reset_zoom(self):
        self._spring.stop()
        self.zoom = self.target_zoom = 1.0
        self.velocity = 0.0
        self.offset = QPointF()
        self.zoomChanged.emit(self.zoom)
        self.update()

    def wheelEvent(self, event):
        delta = event.angleDelta().y() or event.pixelDelta().y()*3
        self.zoom_to(self.target_zoom * math.pow(1.18, max(-8, min(8, delta/120))), event.position())
        event.accept()

    def keyPressEvent(self, event):
        if event.key() in (Qt.Key_Plus, Qt.Key_Equal, Qt.Key_Minus, Qt.Key_0):
            value = 1 if event.key() == Qt.Key_0 else self.target_zoom*(1.2 if event.key() != Qt.Key_Minus else 1/1.2)
            self.zoom_to(value, animated=False)
            event.accept()
        else:
            super().keyPressEvent(event)

    def paintEvent(self, event):
        painter = QPainter(self)
        dark = bool(QApplication.instance().property("darkMode"))
        scale = QApplication.instance().property("uiScale") or 1.0
        # Camera motion must replace every old pixel, including the gutters between tiles.
        painter.fillRect(self.rect(), QColor("#171e25" if dark else "#ffffff"))
        painter.setRenderHint(QPainter.Antialiasing, False)
        ratio = self.devicePixelRatioF()
        gutter = 1 / ratio
        self._layout()
        if not self.boxes:
            painter.setPen(QColor("#b8b8bf" if dark else "#5d5f60"))
            painter.drawText(self.rect().adjusted(16, 0, -16, 0), Qt.AlignCenter | Qt.TextWordWrap, self.message)
        colors = DARK_TILE_COLORS if dark else ("#d9e9ed", "#c4dce2", "#accdd6", "#e8f1f3")
        transform = self.transform()
        for i, (bounds, entry) in enumerate(self.boxes):
            box = pixel_rect(transform.mapRect(bounds), ratio).adjusted(gutter, gutter, -gutter, -gutter)
            if not box.intersects(QRectF(self.rect())) or box.width() < gutter or box.height() < gutter:
                continue
            painter.fillRect(box, QColor(colors[i % len(colors)] if entry.path else ("#353538" if dark else "#e8eaec")))
            box = box.intersected(QRectF(self.rect()))
            metrics = painter.fontMetrics()
            vertical = vertical_label(box, metrics.horizontalAdvance(entry.name), scale,
                                      self._label_angles.get(entry.path) == -90)
            inner = box.adjusted(8*scale, 8*scale, -8*scale, -8*scale)
            width, height = (inner.height(), inner.width()) if vertical else (inner.width(), inner.height())
            line = metrics.height()
            if width < min(metrics.horizontalAdvance(entry.name), 32*scale) or height < line:
                continue
            self._label_angles[entry.path] = -90 if vertical else 0
            painter.save()
            painter.setClipRect(box)
            if vertical:
                painter.translate(inner.left(), inner.bottom())
                painter.rotate(-90)
            else:
                painter.translate(inner.topLeft())
            painter.setPen(QColor(DARK_MAP_TEXT[0] if dark else "#153c46"))
            text = metrics.elidedText(entry.name, Qt.ElideMiddle, int(width))
            painter.drawText(QRectF(0, 0, width, line), Qt.AlignLeft | Qt.AlignVCenter, text)
            if height >= 2*line + 3*scale:
                painter.setPen(QColor(DARK_MAP_TEXT[1] if dark else "#355a63"))
                painter.drawText(QRectF(0, line+3*scale, width, line), Qt.AlignLeft | Qt.AlignVCenter,
                                 metrics.elidedText(size_text(entry.size) + (" +" if entry.partial else ""), Qt.ElideRight, int(width)))
            painter.restore()

    def mouseMoveEvent(self, event):
        if self.press is not None:
            delta = event.position()-self.press
            if self.dragging or delta.manhattanLength() > 6:
                self.dragging = True
                self.offset = self._press_offset + delta
                self._clamp()
                self.setCursor(Qt.ClosedHandCursor)
                self.update()
                QToolTip.hideText()
                return
        if self._spring.isActive():
            QToolTip.hideText()
            return
        inverse, valid = self.transform().inverted()
        point = inverse.map(event.position())
        for bounds, entry in self.boxes:
            if bounds.contains(point):
                self.setCursor(Qt.OpenHandCursor if self.zoom > 1.01 else Qt.PointingHandCursor)
                QToolTip.showText(event.globalPosition().toPoint(), f"{entry.name}\n{entry.size:,} bytes" + (" · partial" if entry.partial else ""), self)
                return
        self.unsetCursor()
        QToolTip.hideText()

    def mouseReleaseEvent(self, event):
        was_dragging = self.dragging
        self.press = None
        self.dragging = False
        if was_dragging:
            self.setCursor(Qt.OpenHandCursor)
            return
        if event.button() == Qt.LeftButton:
            inverse, valid = self.transform().inverted()
            point = inverse.map(event.position())
            for bounds, entry in self.boxes:
                if bounds.contains(point):
                    self.last_chosen_rect = self.transform().mapRect(bounds)
                    self.chosen.emit(entry)
                    return

    def mousePressEvent(self, event):
        if event.button() == Qt.LeftButton:
            self._spring.stop()
            self.target_zoom = self.zoom
            self.velocity = 0
            self.press = QPointF(event.position())
            self._press_offset = QPointF(self.offset)
            self.dragging = False
            self.setFocus(Qt.MouseFocusReason)

    def resizeEvent(self, event):
        self.reset_zoom()
        self._layout_key = None
        super().resizeEvent(event)

    def hideEvent(self, event):
        self._spring.stop()
        super().hideEvent(event)


class RankBars(QWidget):
    chosen = Signal(object)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.entries = []
        self.setMinimumHeight(72)
        self.setAccessibleName("Largest measured items")
        self.setAccessibleDescription("Visual top-six ranking. Use the Breakdown table for all rows and keyboard actions.")

    def set_entries(self, entries):
        self.entries = entries[:6]
        scale = QApplication.instance().property("uiScale") or 1.0
        self.setMinimumHeight(round(max(72, len(self.entries) * 58) * scale))
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        scale = QApplication.instance().property("uiScale") or 1.0
        dark = bool(QApplication.instance().property("darkMode"))
        maximum = max((e.size or 0 for e in self.entries), default=0)
        for i, entry in enumerate(self.entries):
            y = i * 58 * scale
            painter.setPen(QColor("#f4f4f5" if dark else "#131416"))
            painter.drawText(QRectF(0, y, self.width() - 108 * scale, 24 * scale), Qt.AlignVCenter,
                             painter.fontMetrics().elidedText(entry.name, Qt.ElideMiddle, max(0, round(self.width() - 120 * scale))))
            painter.setPen(QColor("#b8b8bf" if dark else "#5d5f60"))
            painter.drawText(QRectF(self.width() - 104 * scale, y, 104 * scale, 24 * scale), Qt.AlignRight | Qt.AlignVCenter,
                             size_text(entry.size) + (" +" if entry.partial and entry.size is not None else ""))
            if maximum and entry.size:
                painter.fillRect(QRectF(0, y + 33 * scale, self.width() * entry.size / maximum, 4 * scale), QColor("#80c8d4" if dark else "#1b6c80"))
        if not self.entries:
            painter.setPen(QColor("#b8b8bf" if dark else "#5d5f60"))
            painter.drawText(self.rect(), Qt.AlignCenter, "No items in this view.")

    def mouseReleaseEvent(self, event):
        row = int(event.position().y() // (58 * (QApplication.instance().property("uiScale") or 1.0)))
        if event.button() == Qt.LeftButton and 0 <= row < len(self.entries):
            self.chosen.emit(self.entries[row])
