# Release package for Windows: the executable with the game data beside it,
# zipped. Run from a checkout on Windows (MSVC toolchain, cargo on PATH):
#
#   powershell -ExecutionPolicy Bypass -File tools\package-windows.ps1
#
# -> dist\MeteorBlitz-<version>-windows.zip. Keep CARGO_TARGET_DIR short:
# long paths break the MSVC linker.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot)

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = 'target' }
cargo build --release
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$name = "MeteorBlitz-$version-windows"
$stage = Join-Path dist $name
Remove-Item -Recurse -Force $stage, "$stage.zip" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force (Join-Path $stage 'original\app') | Out-Null

# The data the game reads (see src/paths.rs): all of assets\, and from the
# original app only what is loaded at run time.
Copy-Item (Join-Path $env:CARGO_TARGET_DIR 'release\meteor-blitz.exe') (Join-Path $stage 'MeteorBlitz.exe')
Copy-Item -Recurse assets (Join-Path $stage 'assets')
# The web build's MP3s; desktop loads the WAVs.
Remove-Item -Recurse -Force (Join-Path $stage 'assets\web-audio')
Copy-Item original\app\ngage.dat, original\app\systems.dat (Join-Path $stage 'original\app')
foreach ($dir in 'audio', 'sprites', 'textures') {
    Copy-Item -Recurse (Join-Path original $dir) (Join-Path $stage "original\$dir")
}
Copy-Item README.md $stage

Compress-Archive -Path $stage -DestinationPath "$stage.zip" -Force
Write-Output "$stage.zip"
