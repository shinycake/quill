# ready-showcase on WARP: glass on (light, dark) and off. Each run writes the
# in-app capture (QUILL_DEMO_CAPTURE, the renderer's render_to_image) and a
# grab of the screen while the window lingers. Usage: glass-shots.ps1 <exe> <out>
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out
)
$ErrorActionPreference = 'Continue'
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$Exe = (Resolve-Path $Exe).Path
Add-Type -AssemblyName System.Drawing, System.Windows.Forms

function Shot([string]$name, [hashtable]$extra) {
    $dir = Join-Path $Out $name
    New-Item -ItemType Directory -Force $dir | Out-Null
    $png = Join-Path $Out "$name.png"
    Remove-Item -ErrorAction SilentlyContinue $png
    foreach ($key in @('QUILL_DEMO_THEME', 'QUILL_GLASS')) { Remove-Item -ErrorAction SilentlyContinue "Env:$key" }
    $env:QUILL_DEMO_CAPTURE = $png
    $env:QUILL_DEMO_LINGER_MS = '9000'
    $env:QUILL_REDUCED_MOTION = '1'
    $env:QUILL_DEMO_WINDOW_SIZE = '1200x800'
    $env:QUILL_DEMO_CLICK = 'r:480,250;p:600'
    $env:RUST_LOG = 'info'
    foreach ($key in $extra.Keys) { Set-Item "Env:$key" $extra[$key] }
    $p = Start-Process -FilePath $Exe -ArgumentList @('--screenshot-demo', 'ready-showcase', $dir) `
        -RedirectStandardError (Join-Path $dir 'run.log') -RedirectStandardOutput (Join-Path $dir 'run.out.log') `
        -PassThru -NoNewWindow
    $waited = 0
    while (-not (Test-Path $png) -and -not $p.HasExited -and $waited -lt 240) { Start-Sleep -Seconds 1; $waited++ }
    Start-Sleep -Seconds 3
    try {
        $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
        $bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
        $bitmap.Save((Join-Path $Out "$name-screen.png"), [System.Drawing.Imaging.ImageFormat]::Png)
        $graphics.Dispose(); $bitmap.Dispose()
    } catch { Write-Host "screen grab failed: $_" }
    if (-not $p.WaitForExit(240000)) { $p.Kill(); Write-Host "$name did not exit" }
    "$name exit $($p.ExitCode) png $(Test-Path $png)" | Tee-Object -Append (Join-Path $Out 'summary.txt')
    Get-Content (Join-Path $dir 'run.log') -ErrorAction SilentlyContinue |
        Select-String -Pattern 'glass|lens|shader|DirectX|panic|error' | Select-Object -First 40 |
        Set-Content (Join-Path $dir 'grep.txt')
    Get-Content (Join-Path $dir 'grep.txt') -ErrorAction SilentlyContinue
}

Shot 'warp-light' @{ QUILL_GLASS = 'force' }
Shot 'warp-dark' @{ QUILL_DEMO_THEME = 'dark'; QUILL_GLASS = 'force' }
Shot 'warp-software-on' @{ QUILL_GLASS = 'on' }
Shot 'warp-off' @{ QUILL_GLASS = 'off' }
exit 0
