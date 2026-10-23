"""One worker, one replaceable pending request, and generation-checked UI state."""
from __future__ import annotations

import os
import queue
import threading
import time
from dataclasses import dataclass

from scanner import Cancelled, Snapshot, measure, normalize, within


@dataclass(frozen=True)
class Request:
    generation: int
    root: str
    cancel: threading.Event


class Session:
    def __init__(self, scan=measure):
        self.root = self.current = ""
        self.history: list[str] = []
        self.history_index = -1
        self.snapshot: Snapshot | None = None
        self.state = "empty"
        self.message = "Choose a folder to see where its space goes."
        self.progress = (0, 0, 0)
        self.started_at = time.monotonic()
        self.generation = 0
        self._cancel = threading.Event()
        self._requests: queue.Queue[Request | None] = queue.Queue(maxsize=1)
        self._events: queue.Queue[tuple[int, str, object]] = queue.Queue()
        self._progress: tuple[int, tuple[int, int, int]] | None = None
        self._scan = scan
        self._closed = False
        self.worker = threading.Thread(target=self._work, name="SpaceLens scanner", daemon=True)
        self.worker.start()

    def _work(self):
        while True:
            request = self._requests.get()
            if request is None:
                return
            if request.cancel.is_set():
                continue
            def progress(files, folders, size):
                self._progress = (request.generation, (files, folders, size))
            try:
                result = self._scan(request.root, request.cancel, progress)
                self._events.put((request.generation, "ready", result))
            except Cancelled:
                pass
            except Exception as error:
                self._events.put((request.generation, "error", str(error)))

    def _replace_pending(self, request):
        try:
            self._requests.get_nowait()
        except queue.Empty:
            pass
        self._requests.put_nowait(request)

    def start(self, root: str, refresh=False):
        if self._closed:
            return
        root = normalize(root)
        self._cancel.set()
        self._cancel = threading.Event()
        self.generation += 1
        self.progress = (0, 0, 0)
        if not refresh or root != self.root:
            self.snapshot = None
            self.root = self.current = root
            self.history, self.history_index = [root], 0
        self.state, self.message = "loading", "Measuring files and folders…"
        self.started_at = time.monotonic()
        self._replace_pending(Request(self.generation, root, self._cancel))

    def poll(self) -> bool:
        changed = False
        progress = self._progress
        if progress and progress[0] == self.generation and self.state == "loading" and progress[1] != self.progress:
            self.progress = progress[1]
            changed = True
        for _ in range(32):
            try:
                generation, kind, value = self._events.get_nowait()
            except queue.Empty:
                break
            if generation != self.generation or self._closed:
                continue
            changed = True
            self.state = kind
            if kind == "ready":
                self.snapshot = value
                while self.current not in value.listings and self.current != self.root:
                    self.current = os.path.dirname(self.current)
                self.history = [path for path in self.history[:self.history_index + 1] if path in value.listings]
                if not self.history or self.history[-1] != self.current:
                    self.history.append(self.current)
                self.history_index = len(self.history) - 1
                self.message = ""
            else:
                self.message = str(value)
        return changed

    @property
    def listing(self):
        if self.snapshot and self.state == "ready":
            return self.snapshot.listings.get(self.current)
        return None

    def navigate(self, path: str, add_history=True) -> bool:
        path = normalize(path)
        if self.state != "ready" or not self.snapshot or not within(path, self.root) or path not in self.snapshot.listings:
            return False
        if path == self.current:
            return True
        self.current = path
        if add_history:
            self.history = self.history[:self.history_index + 1] + [path]
            self.history_index = len(self.history) - 1
        return True

    def back(self):
        if self.state == "ready" and self.history_index > 0:
            self.history_index -= 1
            self.navigate(self.history[self.history_index], False)

    def up(self):
        if self.current and self.current != self.root:
            self.navigate(os.path.dirname(self.current))

    def cancel(self):
        self._cancel.set()
        self.generation += 1
        self._replace_pending(None if self._closed else Request(self.generation, self.root, self._cancel))
        self.state = "ready" if self.snapshot else "cancelled"
        self.message = "Refresh cancelled. Showing the previous snapshot." if self.snapshot else "Scan cancelled. Choose a folder or retry."

    def close(self):
        self._closed = True
        self._cancel.set()
        self.generation += 1
        self._replace_pending(None)
