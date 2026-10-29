# Assemble the Windows package: one flat directory + zip holding quill.exe and
# the native DLLs Quill loads at runtime, so it needs no env vars and no installer.
#
#   quill-windows-<arch>/
#     quill.exe
#     quill-cli.exe              console front for `quill agent` (waits, passes the exit code on)
#     quill-webview.exe          mini apps (WebView2; the Evergreen runtime ships with Windows 10/11)
#     tdjson.dll                         TDLib, OpenSSL and zlib linked in statically
#     quill_calls.dll                    calls (tgcalls behind Quill's C API, LGPL-3.0; imports the FFmpeg DLLs below)
#     licenses/quill-calls/              its source pins (SOURCES.txt) and every part's notice
#     quillvideo.dll av{codec,format,util,filter,device}-N.dll sw{scale,resample}-N.dll   in-process video (FFmpeg, LGPL-2.1+)
#     ffmpeg.exe ffprobe.exe             the programs for video send/edit, photos, camera (same FFmpeg build, no libx264)
#     licenses/ffmpeg/                   FFmpeg license texts + exact source and configure line
#     README.txt LICENSE THIRD_PARTY.md THIRD_PARTY_LICENSES.md
#     licenses/                          native-library license texts and notices
#
# Env (all optional): QUILL_BIN, QUILL_CLI_BIN, QUILL_TDJSON_DIR, QUILL_CALLS_DIR, QUILL_FFMPEG_PREFIX, OUT.
# Needs an MSVC developer environment (dumpbin.exe). No VC++ runtime DLLs are
# shipped: quill.exe and tdjson.dll link the C runtime statically
# (/MT), quill_calls.dll is built the same way, and the MinGW-built FFmpeg DLLs use the
# Universal CRT that is part of Windows 10+ (docs/decisions/codex-release-pipeline.md).
. "$PSScriptRoot/windows-common.ps1"
$root = (Resolve-Path "$PSScriptRoot/..").Path
Set-Location $root
function Pick($value, $default) { if ($value) { $value } else { $default } }
$bin = Pick $env:QUILL_BIN 'target/release/quill.exe'
$cli = Pick $env:QUILL_CLI_BIN 'target/release/quill-cli.exe'
$webview = Pick $env:QUILL_WEBVIEW_BIN 'target/release/quill-webview.exe'
$tdDir = Pick $env:QUILL_TDJSON_DIR 'native/prefix/bin'
$callsDir = Pick $env:QUILL_CALLS_DIR 'vendor/quill-calls'
$callsDll = Join-Path $callsDir 'lib/quill_calls.dll'
$ffmpeg = Pick $env:QUILL_FFMPEG_PREFIX 'vendor/ffmpeg/prefix'
$out = Pick $env:OUT 'dist/windows'

$videoDlls = @(Get-ChildItem -File (Join-Path $ffmpeg 'bin') -Filter *.dll -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^(quillvideo|avcodec-\d+|avformat-\d+|avutil-\d+|avfilter-\d+|avdevice-\d+|swscale-\d+|swresample-\d+)\.dll$' })
if ($videoDlls.Count -ne 8) { throw "expected quillvideo.dll and 7 FFmpeg DLLs in $ffmpeg/bin, found $($videoDlls.Name -join ', ')" }
foreach ($exe in 'ffmpeg.exe', 'ffprobe.exe') {
    if (-not (Test-Path (Join-Path $ffmpeg "bin/$exe"))) { throw "missing $ffmpeg/bin/$exe" }
}
foreach ($f in @($bin, $cli, $webview, (Join-Path $tdDir 'tdjson.dll'), $callsDll, (Join-Path $callsDir 'share/SOURCES.txt'))) {
    if (-not (Test-Path $f)) { throw "missing input $f" }
}

$name = Invoke-QuillExe $bin @('--release-asset-name')
if ($name -notmatch '^quill-windows-(x86_64)$') { throw "unexpected asset name '$name'" }
$pkg = Join-Path $out $name
$zip = Join-Path $out "$name.zip"
Remove-Item -Recurse -Force $pkg, $zip, "$zip.sha256" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $pkg | Out-Null

Copy-Item $bin (Join-Path $pkg 'quill.exe')
Copy-Item $cli (Join-Path $pkg 'quill-cli.exe')
Copy-Item $webview (Join-Path $pkg 'quill-webview.exe')
Copy-Item (Join-Path $tdDir 'tdjson.dll') $pkg
# The call library, beside quill.exe where quill-calls-sys looks for it
# (docs/decisions/tgcalls-engine.md). It imports the FFmpeg DLLs copied
# below; OpenSSL, zlib and the C runtime are linked in.
Copy-Item $callsDll (Join-Path $pkg 'quill_calls.dll')
$callsLic = Join-Path $pkg 'licenses/quill-calls'
New-Item -ItemType Directory -Force $callsLic | Out-Null
Copy-Item (Join-Path $callsDir 'share/SOURCES.txt') $callsLic
Copy-Item -Recurse (Join-Path $callsDir 'share/licenses') (Join-Path $callsLic 'licenses')
# In-process video: the shim plus FFmpeg's DLLs, dynamically linked so they
# stay replaceable (LGPL), with FFmpeg's license texts.
foreach ($dll in $videoDlls) { Copy-Item $dll.FullName $pkg }
# The command-line tools Quill runs for the video editor, photo downscale,
# round videos and the camera (src/media_tools.rs finds them next to quill.exe).
Copy-Item (Join-Path $ffmpeg 'bin/ffmpeg.exe'), (Join-Path $ffmpeg 'bin/ffprobe.exe') $pkg
$lic = Join-Path $pkg 'licenses/ffmpeg'
New-Item -ItemType Directory -Force $lic | Out-Null
Copy-Item (Join-Path $ffmpeg 'share/quillvideo/*') $lic

Copy-Item 'scripts/windows-package-README.txt' (Join-Path $pkg 'README.txt')
# License and third-party notices (mirrors scripts/stage-licenses.sh).
Copy-Item 'LICENSE', 'THIRD_PARTY.md', 'THIRD_PARTY_LICENSES.md' $pkg
$licDir = Join-Path $pkg 'licenses'
New-Item -ItemType Directory -Force (Join-Path $licDir 'thorvg'), (Join-Path $licDir 'unicode-emoji') | Out-Null
Copy-Item 'licenses/*.txt' $licDir
Copy-Item 'third_party/thorvg/LICENSE' (Join-Path $licDir 'thorvg/LICENSE')
Copy-Item 'third_party/thorvg/src/loaders/lottie/rapidjson/LICENSE' (Join-Path $licDir 'thorvg/rapidjson-LICENSE')
Copy-Item 'assets/emoji/LICENSE.txt' (Join-Path $licDir 'unicode-emoji/LICENSE.txt')

& "$PSScriptRoot/check-bundle-pe.ps1" $pkg
if ($LASTEXITCODE -ne 0) { throw 'check-bundle-pe failed' }

Compress-Archive -Path $pkg -DestinationPath $zip -CompressionLevel Optimal
$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
"$hash  $name.zip" | Set-Content -Encoding ascii "$zip.sha256"
"Packaged $zip ({0:N1} MB)" -f ((Get-Item $zip).Length / 1MB) | Write-Host
