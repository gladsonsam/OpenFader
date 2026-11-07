# Run from a Visual Studio x64 developer shell with Flutter and Rust on PATH.
$ErrorActionPreference = 'Stop'
if (-not $env:CPAL_ASIO_DIR -or -not (Test-Path (Join-Path $env:CPAL_ASIO_DIR 'common/asio.h'))) {
  throw 'Set CPAL_ASIO_DIR to an extracted Steinberg ASIO SDK containing common/asio.h.'
}
if (-not $env:LIBCLANG_PATH -or -not (Test-Path (Join-Path $env:LIBCLANG_PATH 'libclang.dll'))) {
  throw 'Set LIBCLANG_PATH to the LLVM bin directory containing libclang.dll.'
}
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
Push-Location (Join-Path $PSScriptRoot '../app')
try {
  flutter build web --release --no-web-resources-cdn
  if ($LASTEXITCODE -ne 0) { throw 'Flutter browser app build failed.' }
  flutter build windows --release
  if ($LASTEXITCODE -ne 0) { throw 'Flutter Windows build failed.' }
} finally { Pop-Location }
