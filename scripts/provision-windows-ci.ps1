$ErrorActionPreference = 'Stop'
$archive = Join-Path $env:RUNNER_TEMP 'asio-sdk.zip'
Invoke-WebRequest -Uri 'https://download.steinberg.net/sdk_downloads/ASIO-SDK_2.3.4_2025-10-15.zip' -OutFile $archive
$expected = 'd5ebf0c20dd2c5f43771fd0c1418f4b361bf52434ee670097cfa6b3a335e2eca'
if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne $expected) { throw 'ASIO SDK checksum mismatch' }
$sdk = Join-Path $env:RUNNER_TEMP 'asio-sdk'
Expand-Archive -Path $archive -DestinationPath $sdk
$sdkRoot = Join-Path $sdk 'ASIOSDK'
if (-not (Test-Path (Join-Path $sdkRoot 'common/asio.h'))) { throw 'ASIO SDK layout changed' }
"CPAL_ASIO_DIR=$sdkRoot" >> $env:GITHUB_ENV
choco install llvm --version=18.1.8 --allow-downgrade --yes --no-progress
if ($LASTEXITCODE -ne 0) { throw 'LLVM installation failed' }
$llvmBin = Join-Path ${env:ProgramFiles} 'LLVM/bin'
if (-not (Test-Path (Join-Path $llvmBin 'libclang.dll'))) { throw 'libclang.dll missing' }
$version = & (Join-Path $llvmBin 'clang.exe') --version
if ($LASTEXITCODE -ne 0 -or $version[0] -notmatch 'clang version 18\.1\.8') { throw "Unexpected LLVM version: $version" }
"LIBCLANG_PATH=$llvmBin" >> $env:GITHUB_ENV

$inno = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
if (-not (Test-Path $inno)) {
  choco install innosetup --yes --no-progress
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $inno)) { throw 'Inno Setup 6 installation failed' }
}
