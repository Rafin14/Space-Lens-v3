# Space Lens Rust

The main Space Lens application for Windows, built in Rust and Qt. Explore folder sizes, find the largest items, and navigate an interactive space map with optimized scanning and rendering.

## Preview

The preview uses sample workspace data to showcase the main view, file filters, expanded-map zoom, folder breakdowns, light appearance, and interface scaling.

![Space Lens Rust feature showcase: overview, filters, map zoom, light appearance, and interface scale](docs/media/space-lens-demo.gif)

## Features

- A complete folder hierarchy and size breakdown, sorted largest first.
- Back, Forward, Up, and Collapse all, with matching full-path headings that wrap to fit both views.
- All items, folder, and file filters, light and dark themes, and interface scaling from 85% to 150%.
- A space map with pointer-centered scroll zoom, drag to pan, vertical labels for narrow tiles, and a full-screen view.
- A largest-items ranking, cancellable scans, refresh, and details for unreadable locations.
- Windows-native deletion dialogs, Recycle Bin behavior, progress, and cancellation.
- Keyboard navigation, copy-path actions, and a reduced-motion setting.

Sizes describe logical file bytes rather than allocated disk space. Hard-linked files are counted for each directory entry. Links and junctions are excluded. A scan is a snapshot; refresh to measure changes.

## Run

Download the portable ZIP from this repository’s Releases page, extract it, and open `SpaceLens_Rust.exe` in the application folder on Windows 10 or 11, 64-bit. Keep the accompanying Qt libraries and plugin folders beside the executable. Python is not required to run the app.

Choose a folder to begin. The main view opens with **All items**, combining folders and files; use the segmented control to filter either type. Use **Delete** or the context menu to let Windows handle deletion. Review any permanent-deletion warning before accepting it; permanent deletion cannot be undone.

## Build

Requires Windows 10 or 11, 64-bit, and [uv](https://docs.astral.sh/uv/getting-started/installation/).

```powershell
.\Setup.ps1
.\Build.ps1
```

Setup installs the pinned GNU Rust toolchain, Qt SDK, and MinGW compiler in the user profile. Build creates a portable application in `dist\SpaceLens_Rust`.

The application logic, scanner, virtual model data, treemap layout, navigation, preferences, and Windows Shell integration are Rust. A small C++ adapter exposes Qt Widgets, drawing primitives, and events to Rust. The application uses dynamically linked Qt libraries.

## Scan engines

The default engine scans independent folders with up to four workers. Rendering uses shared snapshot data, cached text, and a spatial tree to skip map tiles outside the viewport. Animation timers stop while the interface is idle.

Select an engine when launching from PowerShell:

```powershell
.\SpaceLens_Rust.exe "C:\Example" --engine parallel --workers 4
.\SpaceLens_Rust.exe "C:\Example" --engine indexed
.\SpaceLens_Rust.exe "C:\Example" --engine serial
```

`serial` uses the reference scanner. `parallel` uses one to eight workers. Parallel scanning can help broad directory trees; small folders and deep single-branch trees may take longer.

`indexed` is an optional NTFS journal-assisted engine. Its first scan enumerates directories and records file IDs. Later scans use the Windows change journal to identify folders that need another measurement. It falls back to a full scan for unsupported filesystems, unavailable journal access, journal gaps, and directory, rename, link, or permission changes. It does not parse the raw MFT or maintain a persistent disk index.

After deletion, the app updates the affected folder and its ancestor totals. Untouched branches retain their previous snapshot measurements. **Refresh scan** measures the whole selected root again; the indexed engine can reuse journal-validated data. Cancelling an unchanged file deletion preserves the snapshot. Windows continues to own deletion warnings, progress, and cancellation.

The executable requests administrator permission when launched.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl + O | Choose a folder |
| Alt + Left / Right | Back / Forward |
| Alt + Up | Parent folder |
| F5 | Refresh the scan |
| Ctrl + 1 / 2 / 3 | Folders / Files / All items |
| Enter | Open the selected folder |
| Ctrl + C | Copy the selected path |
| Delete | Delete through Windows |
| + / − | Zoom the focused space map |
| 0 | Fit the space map |
| Escape | Cancel a scan or exit the expanded map |

## License

The application is available under the [MIT license](LICENSE). Qt and other components retain their own licenses; see [third-party notices](THIRD_PARTY_NOTICES.md).
