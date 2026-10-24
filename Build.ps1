param([switch]$Debug)
$ErrorActionPreference='Stop'
Set-Location -LiteralPath $PSScriptRoot
$rustBin=Join-Path $env:USERPROFILE '.cargo\bin'
$tools=Join-Path $env:LOCALAPPDATA 'SpaceLens_Rust_Toolchain'
$qtRoot=Join-Path $tools 'Qt\6.10.2\mingw_64'
$mingw=Join-Path $tools 'Qt\Tools\mingw1310_64\bin'
if(-not (Test-Path -LiteralPath $qtRoot)){throw 'Run Setup.ps1 first.'}
$env:QT_ROOT=$qtRoot
$env:PATH="$mingw;$qtRoot\bin;$rustBin;$env:PATH"
$env:CXX=Join-Path $mingw 'g++.exe'
$env:CC=Join-Path $mingw 'gcc.exe'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=Join-Path $mingw 'gcc.exe'
$profile=if($Debug){'debug'}else{'release'}
$buildArgs=@('build','--target','x86_64-pc-windows-gnu','--locked','--bin','SpaceLens_Rust')
if(-not $Debug){$buildArgs+='--release'}
& (Join-Path $rustBin 'cargo.exe') @buildArgs
if($LASTEXITCODE -ne 0){throw 'Rust build failed.'}
$destination=Join-Path $PSScriptRoot 'dist\SpaceLens_Rust'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$executables=@('SpaceLens_Rust.exe')
foreach($name in $executables){Copy-Item -LiteralPath (Join-Path $PSScriptRoot "target\x86_64-pc-windows-gnu\$profile\$name") -Destination $destination -Force}
foreach($name in @('libstdc++-6.dll','libgcc_s_seh-1.dll','libwinpthread-1.dll')){Copy-Item -LiteralPath (Join-Path $mingw $name) -Destination $destination -Force}
& (Join-Path $qtRoot 'bin\windeployqt.exe') --no-translations --no-opengl-sw --no-system-d3d-compiler --no-compiler-runtime (Join-Path $destination 'SpaceLens_Rust.exe')
if($LASTEXITCODE -ne 0){throw 'Qt deployment failed.'}
foreach($name in @('LICENSE','THIRD_PARTY_NOTICES.md','README.md')){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $destination -Force}
foreach($name in @('licenses','docs')){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $destination -Recurse -Force}
Write-Host "Built: $destination\SpaceLens_Rust.exe"
