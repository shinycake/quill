# Windows launch smoke (docs/decisions/ci-launch-smoke.md): open the real UI
# window of a demo-capture build on a hosted runner (no GPU; DXGI enumerates the
# Microsoft Basic Render Driver, i.e. WARP), let the app render its own frame to
# a PNG (QUILL_DEMO_CAPTURE), and fail on a non-zero exit, a missing PNG or a
# blank picture. Usage: ci-launch-smoke.ps1 <quill.exe> <out-dir> [kind...]
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

foreach ($kind in $Kinds) {
    Write-Host "::group::launch smoke: $kind"
    $png = Join-Path $Out "$kind.png"
    $log = Join-Path $Out "$kind.log"
    Remove-Item -ErrorAction SilentlyContinue $png, (Join-Path $Out ".quill-ready-$kind")
    $env:QUILL_DEMO_CAPTURE = $png
    $env:QUILL_DEMO_LINGER_MS = '6000'
    $env:QUILL_REDUCED_MOTION = '1'
    $env:QUILL_DEMO_WINDOW_SIZE = '1200x740'
    $p = Start-Process -FilePath $Exe -ArgumentList @('--screenshot-demo', $kind, $Out) `
        -RedirectStandardError $log -RedirectStandardOutput (Join-Path $Out "$kind.out.log") `
        -PassThru -NoNewWindow
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
if ($failed) { exit 1 }
