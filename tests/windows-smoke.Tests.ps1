param([string]$Executable = 'target/release/satori.exe', [int]$TimeoutSeconds = 30)
# Requires an interactive Windows runner with WebView2; no real devices or AI.
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'This native startup smoke test requires Windows'
}
$path = (Resolve-Path -LiteralPath $Executable).Path
$process = Start-Process -FilePath $path -ArgumentList '--demo' -PassThru
$timer = [Diagnostics.Stopwatch]::StartNew()
try {
    if (!$process.WaitForInputIdle($TimeoutSeconds * 1000)) { throw 'GUI did not become idle before the deadline' }
    $ready = $false
    while ($timer.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        $process.Refresh()
        if ($process.HasExited) { throw "Satori exited during startup: $($process.ExitCode)" }
        if ($process.MainWindowHandle -ne [IntPtr]::Zero -and $process.MainWindowTitle -eq 'Satori') {
            $ready = $true
            break
        }
        Start-Sleep -Milliseconds 100
    }
    if (!$ready) { throw 'No Satori panel window appeared before the deadline' }
    if (!$process.WaitForInputIdle(5000)) { throw 'Satori panel stopped responding' }
    $process.Refresh()
    if ($process.HasExited) { throw 'Satori exited after creating its panel' }
    Write-Output 'Native demo startup: Satori panel created and GUI responsive; no real device operations or API requests.'
} finally {
    $process.Refresh()
    if (!$process.HasExited) { Stop-Process -Id $process.Id -Force }
    $process.Dispose()
}
