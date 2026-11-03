<#
.SYNOPSIS
Lists ASIO interfaces, or records a short take using only a portable app bundle.
.EXAMPLE
powershell -NoProfile -ExecutionPolicy Bypass -File .\check-audio.ps1
.EXAMPLE
.\check-audio.ps1 -DeviceId 'ASIO:driver name from list' -RecordSeconds 10 -OutputDirectory D:\AudioChecks
.DESCRIPTION
Close other audio applications before checking. No developer tools are required.
This check never opens audio outputs or changes mixer routing. RecordSeconds must
be supplied explicitly to create a recording. Takes and check.json stay in the
chosen output directory so the result can be inspected and shared.
#>
param(
  [string]$Bundle = $PSScriptRoot,
  [string]$DeviceId,
  [ValidateRange(0, 60)][int]$RecordSeconds = 0,
  [string]$OutputDirectory = (Get-Location).Path
)
$ErrorActionPreference = 'Stop'
$helper = Join-Path (Resolve-Path -LiteralPath $Bundle).Path 'openfader-capture-helper.exe'
if (-not (Test-Path -LiteralPath $helper -PathType Leaf)) { throw "Missing packaged audio helper: $helper" }
function Start-Helper([string]$Arguments) {
  $info = New-Object System.Diagnostics.ProcessStartInfo
  $info.FileName = $helper
  $info.Arguments = $Arguments
  $info.UseShellExecute = $false
  $info.CreateNoWindow = $true
  $info.RedirectStandardInput = $true
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  $process = New-Object System.Diagnostics.Process
  $process.StartInfo = $info
  if (-not $process.Start()) { throw 'Could not start audio helper' }
  return $process
}
function Read-Reply($Process, [string]$Expected) {
  $read = $Process.StandardOutput.ReadLineAsync()
  if (-not $read.Wait(15000)) { throw 'ASIO driver operation timed out' }
  $line = $read.Result
  if (-not $line) { throw 'Audio helper exited before replying' }
  $reply = $line | ConvertFrom-Json
  if ($reply.state -ne $Expected) { throw "Audio helper: $line" }
  return $reply
}
function Exchange($Process, $Request, [string]$Expected) {
  $Process.StandardInput.WriteLine(($Request | ConvertTo-Json -Depth 12 -Compress))
  $Process.StandardInput.Flush()
  $reply = Read-Reply $Process $Expected
  if ($reply.generation -ne 1) { throw 'Unexpected helper generation' }
  return $reply
}
$process = Start-Helper '--list-devices'
try {
  $stderr = $process.StandardError.ReadToEndAsync()
  $scan = Read-Reply $process 'DEVICES'
  if (-not $process.WaitForExit(5000) -or $process.ExitCode -ne 0) { throw 'Device scan did not exit successfully' }
} finally {
  if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
  $process.Dispose()
}
@($scan.devices) | Format-List id, name, input_channels, output_channels, sample_rate | Out-Host
if (@($scan.devices).Count -eq 0) { throw 'No ASIO interfaces found. Check the manufacturer 64-bit ASIO driver and USB connection, then close other audio applications.' }
if ($RecordSeconds -eq 0) { return }
if (-not $DeviceId) { throw 'Pass -DeviceId exactly as listed to record' }
$device = @($scan.devices | Where-Object { $_.id -ceq $DeviceId })
if ($device.Count -ne 1 -or $device[0].input_channels -lt 1) { throw 'Select an interface with recording inputs from the list' }
$root = Join-Path ([System.IO.Path]::GetFullPath($OutputDirectory)) ('OpenFader-ASIO-check-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$process = Start-Helper ''
try {
  $stderr = $process.StandardError.ReadToEndAsync()
  $null = Exchange $process @{op='prepare'; generation=1; root=$root; device=$device[0]} 'READY'
  $started = Exchange $process @{op='commit'; generation=1; take_id='capture'} 'RECORDING'
  Start-Sleep -Seconds $RecordSeconds
  $stopped = Exchange $process @{op='stop'; generation=1} 'STOPPED'
  if (-not $process.WaitForExit(5000) -or $process.ExitCode -ne 0) { throw 'Capture helper did not exit successfully' }
  $manifest = Get-Content -LiteralPath (Join-Path $root 'capture/manifest.json') -Raw | ConvertFrom-Json
  if (-not $manifest.complete -or $manifest.frames -lt $RecordSeconds * $device[0].sample_rate * 0.8 -or $manifest.channels -ne $device[0].input_channels -or $manifest.sample_rate -ne $device[0].sample_rate -or $stopped.status.error -or $stopped.status.xruns -ne 0) {
    throw 'Capture failed its frame/layout/rate/xrun checks; retain the take for diagnosis'
  }
  foreach ($segment in $manifest.segments) {
    $file = Join-Path (Join-Path $root 'capture') $segment.file
    if ((Get-Item -LiteralPath $file).Length -ne 44 + $segment.frames * $manifest.channels * 3) { throw 'WAV frame alignment or length is invalid' }
  }
  @{device=$device[0]; requested_seconds=$RecordSeconds; started=$started; stopped=$stopped; manifest=$manifest} |
    ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $root 'check.json') -Encoding UTF8
  Write-Host "Recorded $($manifest.channels) channels at $($manifest.sample_rate) Hz: $($manifest.frames) frames, no reported xruns."
  Write-Host "Recording and diagnostic report: $root"
} finally {
  if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
  $process.Dispose()
}
