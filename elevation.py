"""Use Windows UAC for source launches; frozen builds request elevation in their manifest."""
import ctypes
import os
import subprocess
import sys


def ensure_elevated():
    if os.name != "nt" or ctypes.windll.shell32.IsUserAnAdmin():
        return True
    arguments = sys.argv[1:] if getattr(sys, "frozen", False) else [os.path.abspath(sys.argv[0]), *sys.argv[1:]]
    shell = ctypes.windll.shell32.ShellExecuteW
    shell.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_wchar_p,
                      ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_int]
    shell.restype = ctypes.c_void_p
    result = shell(None, "runas", sys.executable, subprocess.list2cmdline(arguments), None, 1)
    if not result or result <= 32:
        raise OSError("Administrator access was not granted. Open Space Lens again and approve the Windows prompt.")
    return False
