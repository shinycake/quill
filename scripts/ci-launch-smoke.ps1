# Windows launch smoke (docs/decisions/ci-launch-smoke.md): open the real UI
# window of a demo-capture build on a hosted runner (no GPU; DXGI enumerates the
# Microsoft Basic Render Driver, i.e. WARP), let the app render its own frame to
# a PNG (QUILL_DEMO_CAPTURE), and fail on a non-zero exit, a missing PNG or a
# blank picture. The first kind also prints its idle wakeups (context switches
# over 10 s of an idle window, all threads and the main one, from the raw
# Thread performance data) and the same with QUILL_IDLE_FRAMES=0 for contrast
# (docs/decisions/sweep-idle-heartbeat.md); report only.
# Usage: ci-launch-smoke.ps1 <quill.exe> <out-dir> [kind...]
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    [string[]]$Kinds = @('ready-chats', 'ready-chats-composer', 'ready-chat-list-search')
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$Exe = (Resolve-Path $Exe).Path
$failed = $false
$idleKind = $Kinds[0]

function Start-Demo([string]$kind, [string]$png, [string]$linger, [string]$logBase) {
    Remove-Item -ErrorAction SilentlyContinue $png, (Join-Path $Out ".quill-ready-$kind")
    $env:QUILL_DEMO_CAPTURE = $png
    $env:QUILL_DEMO_LINGER_MS = $linger
    $env:QUILL_REDUCED_MOTION = '1'
    $env:QUILL_DEMO_WINDOW_SIZE = '1200x740'
    Start-Process -FilePath $Exe -ArgumentList @('--screenshot-demo', $kind, $Out) `
        -RedirectStandardError "$logBase.log" -RedirectStandardOutput "$logBase.out.log" `
        -PassThru -NoNewWindow
}

# Waits for the capture, lets the window settle, then samples context switches
# of every thread of the process. Prints and saves the result; never fails.
function Measure-Idle($proc, [string]$png, [string]$label, [string]$file) {
    try {
        $waited = 0
        while (-not (Test-Path $png) -and -not $proc.HasExited -and $waited -lt 180) {
            Start-Sleep -Seconds 1
            $waited++
        }
        if ($proc.HasExited -or -not (Test-Path $png)) {
            Write-Host "idle wakeups ($label): window never became ready"
            return
        }
        Start-Sleep -Seconds 3
        # Raw (cumulative) context switches of each thread; the main thread is
        # the one whose instance name ends in /0.
        $query = "IDProcess = $($proc.Id)"
        $before = @{}
        Get-CimInstance Win32_PerfRawData_PerfProc_Thread -Filter $query |
            ForEach-Object { $before[[int]$_.IDThread] = [int64]$_.ContextSwitchesPersec }
        $cpu0 = (Get-Process -Id $proc.Id).TotalProcessorTime
        $clock = [System.Diagnostics.Stopwatch]::StartNew()
        Start-Sleep -Seconds 10
        $elapsed = $clock.Elapsed.TotalSeconds
        $cpu1 = (Get-Process -Id $proc.Id).TotalProcessorTime
        $after = @(Get-CimInstance Win32_PerfRawData_PerfProc_Thread -Filter $query)
        $total = 0
        $main = 0
        $rows = @()
        foreach ($thread in $after) {
            $id = [int]$thread.IDThread
            if (-not $before.ContainsKey($id)) { continue }
            $switches = [int64]$thread.ContextSwitchesPersec - $before[$id]
            $total += $switches
            if ($thread.Name -like '*/0') { $main = $switches }
            if ($switches -gt 0) {
                $rows += [pscustomobject]@{ Name = $thread.Name; Id = $id; Switches = $switches }
            }
        }
        $cpu = 100.0 * ($cpu1 - $cpu0).TotalSeconds / $elapsed
        $line = "idle wakeups ($label) over {0:N1} s: main thread {1} ({2:N1}/s), all threads {3} ({4:N1}/s), cpu {5:N2}%" -f `
            $elapsed, $main, ($main / $elapsed), $total, ($total / $elapsed), $cpu
        Write-Host $line
        $text = $rows | Sort-Object -Property Switches -Descending |
            ForEach-Object { "  {0,-16} tid {1,-8} switched {2}" -f $_.Name, $_.Id, $_.Switches }
        $text | ForEach-Object { Write-Host $_ }
        @($line) + $text | Set-Content -Path $file
    }
    catch {
        Write-Host "idle wakeups ($label): could not measure: $_"
    }
}

foreach ($kind in $Kinds) {
    Write-Host "::group::launch smoke: $kind"
    $png = Join-Path $Out "$kind.png"
    $log = Join-Path $Out "$kind.log"
    $linger = if ($kind -eq $idleKind) { '30000' } else { '6000' }
    $p = Start-Demo $kind $png $linger (Join-Path $Out $kind)
    if ($kind -eq $idleKind) {
        Measure-Idle $p $png 'parking on' (Join-Path $Out 'idle-wakeups.txt')
    }
    if (-not $p.WaitForExit(240000)) {
        $p.Kill()
        Write-Host "FAIL ${kind}: did not exit within 240 s"
        $failed = $true
    }
    elseif ($p.ExitCode -ne 0) {
        Write-Host "FAIL ${kind}: exit code $($p.ExitCode)"
        $failed = $true
    }
    elseif (-not (Test-Path $png)) {
        Write-Host "FAIL ${kind}: no picture written"
        $failed = $true
    }
    else {
        python -I scripts/ci-image-check.py $png
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAIL ${kind}: blank or flat picture"
            $failed = $true
        }
        else { Write-Host "ok $kind" }
    }
    Get-Content $log -Tail 20 -ErrorAction SilentlyContinue
    Write-Host '::endgroup::'
}

# Contrast for the idle numbers: the same window with parking off.
Write-Host "::group::idle contrast: $idleKind with QUILL_IDLE_FRAMES=0"
$env:QUILL_IDLE_FRAMES = '0'
$png = Join-Path $Out "$idleKind-idle-off.png"
$p = Start-Demo $idleKind $png '30000' (Join-Path $Out "$idleKind-idle-off")
Measure-Idle $p $png 'parking off' (Join-Path $Out 'idle-wakeups-parking-off.txt')
if (-not $p.WaitForExit(240000)) { $p.Kill() }
Remove-Item Env:QUILL_IDLE_FRAMES
Write-Host '::endgroup::'

if ($failed) { exit 1 }
