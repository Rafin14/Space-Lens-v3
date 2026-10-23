from pathlib import Path
import os

# Qt expects Windows ICU, not a DLL supplied by unrelated tools on PATH.
os.environ['PATH'] = str(Path(os.environ['SystemRoot']) / 'System32') + os.pathsep + os.environ.get('PATH', '')
root = Path(SPECPATH)
a = Analysis([str(root / 'app.py')], pathex=[str(root)], binaries=[],
             datas=[(str(root / 'THIRD_PARTY_NOTICES.md'), '.'), (str(root / 'licenses'), 'licenses')],
             hiddenimports=[], hookspath=[], hooksconfig={}, runtime_hooks=[], excludes=[], noarchive=False)
assert not any(Path(item[0]).name.lower() == 'icuuc.dll' for item in a.binaries), 'Unexpected bundled ICU library'
pyz = PYZ(a.pure)
exe = EXE(pyz, a.scripts, a.binaries, a.datas, [], name='Space Lens v3', debug=False,
          bootloader_ignore_signals=False, strip=False, upx=False, console=False, uac_admin=True,
          icon=str(root / 'assets' / 'space-lens.ico'))
