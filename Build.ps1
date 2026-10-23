$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
$python = Join-Path $PSScriptRoot '.venv\Scripts\python.exe'
if (-not (Test-Path -LiteralPath $python)) { throw 'Run Setup.ps1 first.' }
& $python -B -m PyInstaller --clean --noconfirm 'Space Lens v3 Portable.spec'
if ($LASTEXITCODE -ne 0) { throw 'Packaging failed.' }
Write-Host 'Built: dist\Space Lens v3.exe'
