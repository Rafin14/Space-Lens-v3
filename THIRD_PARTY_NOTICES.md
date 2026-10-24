# Third-party components

Space Lens Rust uses Qt 6 Core, GUI, and Widgets through dynamically linked libraries. Qt is developed by The Qt Company and contributors and is available under its applicable open-source licenses. The Qt LGPL and GPL license texts are provided in `licenses/qt`.

The distributed Qt runtime is version 6.10.2. Bundled component attribution records and license texts are included in `licenses/qt/third-party`, derived from the SDK's software bill of materials. The corresponding Qt Base source is available from [Qt's v6.10.2 source tree](https://github.com/qt/qtbase/tree/v6.10.2). The runtime libraries can be replaced by compatible builds.

The Windows build uses the GNU Rust toolchain and MinGW runtime libraries. Their applicable license notices are supplied in `licenses/toolchain`. The GCC runtime libraries include the GCC Runtime Library Exception.

Rust dependencies and their license notices are supplied in `licenses/rust`. Dependency versions are recorded in `Cargo.lock`. The application does not use Python or Qt for Python at runtime.

Component source and license information:

- [Qt](https://www.qt.io/licensing/open-source-lgpl-obligations)
- [Rust](https://www.rust-lang.org/policies/licenses)
- [MinGW-w64](https://www.mingw-w64.org/)
- [GCC Runtime Library Exception](https://www.gnu.org/licenses/gcc-exception-3.1.html)
- [serde](https://github.com/serde-rs/serde)
- [serde_json](https://github.com/serde-rs/json)
- [windows-rs](https://github.com/microsoft/windows-rs)
- [unicode-casefold](https://github.com/lfairy/unicode-casefold)
