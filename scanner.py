"""Read-only, cancellable folder measurement. No GUI or third-party imports."""
from __future__ import annotations

import os
import stat
import threading
import time
from dataclasses import dataclass, field
from datetime import datetime
from typing import Callable


@dataclass(frozen=True, slots=True)
class Entry:
    path: str
    name: str
    size: int | None
    is_dir: bool
    partial: bool = False


@dataclass(slots=True)
class Listing:
    entries: list[Entry] = field(default_factory=list)
    total: int = 0
    errors: int = 0
    error: str = ""


@dataclass(slots=True)
class Snapshot:
    root: str
    listings: dict[str, Listing]
    measured_at: datetime
    files: int
    links_skipped: int
    issues: list[tuple[str, str]]


class Cancelled(Exception):
    pass


def normalize(path: str) -> str:
    return os.path.normpath(os.path.abspath(path))


def within(path: str, root: str) -> bool:
    try:
        return os.path.normcase(os.path.commonpath((normalize(path), normalize(root)))) == os.path.normcase(normalize(root))
    except ValueError:
        return False


def is_link(info: os.stat_result) -> bool:
    # Junctions are reparse points but are not symbolic links on Windows.
    return stat.S_ISLNK(info.st_mode) or bool((getattr(info, "st_file_attributes", 0) or 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT)


def size_text(value: int | None) -> str:
    if value is None:
        return "Unknown"
    amount = float(value)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB", "PiB"):
        if amount < 1024 or unit == "PiB":
            return f"{amount:,.0f} {unit}" if unit == "B" else f"{amount:,.1f} {unit}"
        amount /= 1024
    return "0 B"


def measure(root: str, cancel: threading.Event, progress: Callable[[int, int, int], None] | None = None) -> Snapshot:
    if cancel.is_set():
        raise Cancelled()
    root = normalize(root)
    info = os.stat(root, follow_symlinks=False)
    if is_link(info):
        raise OSError("Choose a real folder. Linked folders and Windows reparse points are excluded.")
    if not stat.S_ISDIR(info.st_mode):
        raise NotADirectoryError(root)
    listings: dict[str, Listing] = {}
    stack = [root]
    order: list[str] = []
    issues: list[tuple[str, str]] = []
    files = links = bytes_seen = 0
    last_progress = 0.0

    def check_cancel() -> None:
        if cancel.is_set():
            raise Cancelled()

    def issue(path: str, error: OSError) -> None:
        # shortcut: retain the first 100 explanations, while totals count every failure.
        if len(issues) < 100:
            issues.append((path, str(error)))

    while stack:
        check_cancel()
        path = stack.pop()
        listing = listings[path] = Listing()
        order.append(path)
        try:
            # Recheck before opening: a queued directory may have changed into a link.
            if is_link(os.stat(path, follow_symlinks=False)):
                raise OSError("Folder changed into an excluded link during the scan")
            with os.scandir(path) as children:
                for child in children:
                    check_cancel()
                    directory = False
                    try:
                        directory = child.is_dir(follow_symlinks=False)
                        info = child.stat(follow_symlinks=False)
                        if is_link(info):
                            links += 1
                            continue
                        if directory:
                            stack.append(child.path)
                            listing.entries.append(Entry(child.path, child.name, 0, True))
                        elif stat.S_ISREG(info.st_mode):
                            listing.entries.append(Entry(child.path, child.name, info.st_size, False))
                            files += 1
                            bytes_seen += info.st_size
                    except OSError as error:
                        listing.errors += 1
                        issue(child.path, error)
                        listing.entries.append(Entry(child.path, child.name, None, directory, True))
                    now = time.monotonic()
                    if progress and now - last_progress >= .15:
                        progress(files, len(listings), bytes_seen)
                        last_progress = now
        except OSError as error:
            if path == root:
                raise
            listing.error = str(error)
            listing.errors += 1
            issue(path, error)

    # Parents are visited before children; reverse order aggregates without recursion.
    for path in reversed(order):
        check_cancel()
        listing = listings[path]
        entries = []
        for entry in listing.entries:
            if entry.is_dir and entry.path in listings:
                child = listings[entry.path]
                entry = Entry(entry.path, entry.name, None if child.error and not child.entries else child.total, True, bool(child.errors))
                listing.errors += child.errors
            entries.append(entry)
        listing.entries = sorted(entries, key=lambda e: (e.size is None, -(e.size or 0), e.name.casefold()))
        listing.total = sum(e.size or 0 for e in entries)
    if progress:
        progress(files, len(listings), bytes_seen)
    return Snapshot(root, listings, datetime.now().astimezone(), files, links, issues)

