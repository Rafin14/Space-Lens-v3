# Third-party components

Space Lens v3 uses Qt for Python (PySide6 Essentials and Shiboken6) 6.11.2.
Qt and PySide include components under LGPLv3/GPLv3 and other licenses.

- [Qt for Python licenses](https://doc.qt.io/qtforpython-6/licenses.html)
- [Qt open-source licensing](https://www.qt.io/licensing/open-source-lgpl-obligations)
- [PySide source](https://code.qt.io/cgit/pyside/pyside-setup.git/)
- [Qt 6.11.2 source archives](https://download.qt.io/official_releases/qt/6.11/6.11.2/)
- [PySide source archives](https://download.qt.io/official_releases/QtForPython/pyside6/)

Qt libraries are loaded dynamically. In a standalone build, PyInstaller extracts the Qt DLLs, this notice, and the license texts into its temporary application directory. A source build can use compatible replacement Qt libraries installed in the Python environment. Do not restrict modification or reverse engineering of these components.

PyInstaller 6.22.2 is a build tool. Its bootloader has an exception permitting distribution of generated applications. See the [PyInstaller license](https://pyinstaller.org/en/stable/license.html).

Python 3.14 uses the Python Software Foundation License and includes additional notices. See the [Python license](https://docs.python.org/3.14/license.html).

Copies of LGPLv3, GPLv3, and Python's license are included in `licenses/` and bundled with the executable.
