$ErrorActionPreference='Stop'
Set-Location -LiteralPath $PSScriptRoot
if(-not (Get-Command uv -ErrorAction SilentlyContinue)){throw 'Install uv: https://docs.astral.sh/uv/getting-started/installation/'}
$tools=Join-Path $env:LOCALAPPDATA 'SpaceLens_Rust_Toolchain'
New-Item -ItemType Directory -Path $tools -Force | Out-Null
$rustup=Join-Path $env:USERPROFILE '.cargo\bin\rustup.exe'
if(-not (Test-Path -LiteralPath $rustup)){
    $installer=Join-Path $tools 'rustup-init.exe'
    Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile $installer
    & $installer -y --profile minimal --default-host x86_64-pc-windows-gnu --default-toolchain 1.99.1 --no-modify-path
    if($LASTEXITCODE -ne 0){throw 'Rust installation failed.'}
}
& $rustup toolchain install 1.99.1-x86_64-pc-windows-gnu --profile minimal
if($LASTEXITCODE -ne 0){throw 'GNU Rust toolchain installation failed.'}
$python=Join-Path $tools 'python\Scripts\python.exe'
if(-not (Test-Path -LiteralPath $python)){
    & uv venv --python 3.14.6 (Join-Path $tools 'python')
    if($LASTEXITCODE -ne 0){throw 'Dependency installer environment failed.'}
}
& uv pip install --python $python aqtinstall==3.3.0
if($LASTEXITCODE -ne 0){throw 'Qt installer setup failed.'}
$qt=Join-Path $tools 'Qt'
if(-not (Test-Path -LiteralPath (Join-Path $qt 'Tools\mingw1310_64\bin\g++.exe'))){
    & $python -B -m aqt install-tool windows desktop tools_mingw1310 qt.tools.win64_mingw1310 -O $qt
    if($LASTEXITCODE -ne 0){throw 'MinGW installation failed.'}
}
if(-not (Test-Path -LiteralPath (Join-Path $qt '6.10.2\mingw_64\bin\qmake.exe'))){
    & $python -B -m aqt install-qt windows desktop 6.10.2 win64_mingw --archives qtbase -O $qt
    if($LASTEXITCODE -ne 0){throw 'Qt SDK installation failed.'}
}
Write-Host 'Ready. Run .\Build.ps1 to create the portable application.'
