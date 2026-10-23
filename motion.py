"""Short native transitions and a velocity-preserving, critically damped spring."""
import math
import os

from PySide6.QtCore import QEasingCurve, QRectF, Qt, QVariantAnimation
from PySide6.QtGui import QColor, QPainter
from PySide6.QtWidgets import QApplication, QWidget


def system_reduced_motion():
    if os.name == "nt":
        import ctypes
        enabled = ctypes.c_int(1)
        if ctypes.windll.user32.SystemParametersInfoW(0x1042, 0, ctypes.byref(enabled), 0):
            return not enabled.value
    return False


def reduced_motion():
    return bool(QApplication.instance().property("reduceMotion"))


def spring_step(value, velocity, target, dt, response=.24):
    omega = 4.6 / response
    delta = value - target
    decay = math.exp(-omega * dt)
    step = (velocity + omega * delta) * dt
    return target + (delta + step) * decay, (velocity - omega * step) * decay


class SnapshotTransition(QWidget):
    """Animate one cached image, keeping the actual widgets responsive underneath."""
    def __init__(self, parent, pixmap, start=None, finish=None, duration=220, fade=True):
        super().__init__(parent)
        self.setAttribute(Qt.WA_TransparentForMouseEvents)
        self.setAttribute(Qt.WA_TranslucentBackground)
        self.setStyleSheet("background: transparent;")
        self.setGeometry(parent.rect())
        self.pixmap = pixmap
        self.start_rect = QRectF(start if start is not None else self.rect())
        self.finish_rect = QRectF(finish if finish is not None else self.rect())
        self.fade = fade
        self.progress = 0.0
        self.animation = QVariantAnimation(self)
        self.animation.setDuration(0 if reduced_motion() else duration)
        self.animation.setStartValue(0.0)
        self.animation.setEndValue(1.0)
        self.animation.setEasingCurve(QEasingCurve.OutCubic)
        self.animation.valueChanged.connect(self._step)
        self.animation.finished.connect(self.deleteLater)
        self.show()
        self.raise_()
        self.animation.start()

    def _step(self, value):
        self.progress = value
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.SmoothPixmapTransform)
        if self.fade:
            painter.setOpacity(1 - self.progress)
        a, b, t = self.start_rect, self.finish_rect, self.progress
        rect = QRectF(a.x()+(b.x()-a.x())*t, a.y()+(b.y()-a.y())*t,
                      a.width()+(b.width()-a.width())*t, a.height()+(b.height()-a.height())*t)
        painter.drawPixmap(rect, self.pixmap, QRectF(self.pixmap.rect()))
