# Assemble the Windows package: one flat directory + zip holding quill.exe and
# the native DLLs Quill loads at runtime, so it needs no env vars and no installer.
#
#   quill-windows-<arch>/
#     quill.exe
#     tdjson.dll libssl-3-x64.dll libcrypto-3-x64.dll z.dll
#     ntgcalls.dll rlottie.dll
#     quillvideo.dll av{codec,format,util}-N.dll sw{scale,resample}-N.dll   in-process video (FFmpeg, LGPL-2.1+)
#     licenses/ffmpeg/                   FFmpeg license texts + exact source and configure line
#     vcruntime140*.dll msvcp140*.dll   (app-local VC++ runtime, see decision doc)
#     README.txt LICENSE THIRD_PARTY.md THIRD_PARTY_LICENSES.md
#     licenses/                          native-library license texts and notices
#
# Env (all optional): QUILL_BIN, QUILL_TDJSON_DIR, QUILL_NTGCALLS_DLL, QUILL_RLOTTIE_DLL, QUILL_FFMPEG_PREFIX, OUT.
# Needs an MSVC developer environment (dumpbin.exe, VCToolsRedistDir).
. "$PSScriptRoot/windows-common.ps1"
$root = (Resolve-Path "$PSScriptRoot/..").Path
Set-Location $root
function Pick($value, $default) { if ($value) { $value } else { $default } }
$bin = Pick $env:QUILL_BIN 'target/release/quill.exe'
$tdDir = Pick $env:QUILL_TDJSON_DIR 'native/prefix/bin'
$ntg = Pick $env:QUILL_NTGCALLS_DLL 'vendor/ntgcalls/lib/Release/ntgcalls.dll'
$rlottie = Pick $env:QUILL_RLOTTIE_DLL 'vendor/rlottie/prefix/bin/rlottie.dll'
$ffmpeg = Pick $env:QUILL_FFMPEG_PREFIX 'vendor/ffmpeg/prefix'
$out = Pick $env:OUT 'dist/windows'

$tdFiles = @(Get-ChildItem -File $tdDir -Filter *.dll | ForEach-Object Name)   # tdjson + OpenSSL + zlib
$videoDlls = @(Get-ChildItem -File (Join-Path $ffmpeg 'bin') -Filter *.dll -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^(quillvideo|avcodec-\d+|avformat-\d+|avutil-\d+|swscale-\d+|swresample-\d+)\.dll$' })
if ($videoDlls.Count -ne 6) { throw "expected quillvideo.dll and 5 FFmpeg DLLs in $ffmpeg/bin, found $($videoDlls.Name -join ', ')" }
foreach ($f in @($bin, $ntg, $rlottie, (Join-Path $tdDir 'tdjson.dll'))) {
    if (-not (Test-Path $f)) { throw "missing input $f" }
}

$name = Invoke-QuillExe $bin @('--release-asset-name')
if ($name -notmatch '^quill-windows-(x86_64)$') { throw "unexpected asset name '$name'" }
$pkg = Join-Path $out $name
$zip = Join-Path $out "$name.zip"
Remove-Item -Recurse -Force $pkg, $zip, "$zip.sha256" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $pkg | Out-Null

Copy-Item $bin (Join-Path $pkg 'quill.exe')
foreach ($f in $tdFiles) { Copy-Item (Join-Path $tdDir $f) $pkg }
Copy-Item $ntg (Join-Path $pkg 'ntgcalls.dll')
Copy-Item $rlottie (Join-Path $pkg 'rlottie.dll')
# In-process video: the shim plus FFmpeg's DLLs, dynamically linked so they
# stay replaceable (LGPL), with FFmpeg's license texts.
foreach ($dll in $videoDlls) { Copy-Item $dll.FullName $pkg }
$lic = Join-Path $pkg 'licenses/ffmpeg'
New-Item -ItemType Directory -Force $lic | Out-Null
Copy-Item (Join-Path $ffmpeg 'share/quillvideo/*') $lic

# Bundle the VC++ runtime closure: any imported vcruntime/msvcp/concrt DLL that
# is not in the package yet comes from the MSVC redistributable directory.
$redist = Get-VcRedistDir
Write-Host "VC++ runtime from $redist"
$changed = $true
while ($changed) {
    $changed = $false
    foreach ($file in Get-ChildItem -File $pkg | Where-Object { $_.Extension -in '.exe', '.dll' }) {
        foreach ($dep in Get-PeDependents $file.FullName) {
            if (Test-Path (Join-Path $pkg $dep)) { continue }
            $src = Join-Path $redist $dep
            if (Test-Path $src) { Copy-Item $src $pkg; $changed = $true }
        }
    }
}

Copy-Item 'scripts/windows-package-README.txt' (Join-Path $pkg 'README.txt')
# License and third-party notices (mirrors scripts/stage-licenses.sh).
Copy-Item 'LICENSE', 'THIRD_PARTY.md', 'THIRD_PARTY_LICENSES.md' $pkg
$licDir = Join-Path $pkg 'licenses'
New-Item -ItemType Directory -Force (Join-Path $licDir 'rlottie'), (Join-Path $licDir 'unicode-emoji') | Out-Null
Copy-Item 'licenses/*.txt' $licDir
Copy-Item 'licenses/rlottie/*' (Join-Path $licDir 'rlottie')
Copy-Item 'assets/emoji/LICENSE.txt' (Join-Path $licDir 'unicode-emoji/LICENSE.txt')

& "$PSScriptRoot/check-bundle-pe.ps1" $pkg
if ($LASTEXITCODE -ne 0) { throw 'check-bundle-pe failed' }

Compress-Archive -Path $pkg -DestinationPath $zip -CompressionLevel Optimal
$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
"$hash  $name.zip" | Set-Content -Encoding ascii "$zip.sha256"
"Packaged $zip ({0:N1} MB)" -f ((Get-Item $zip).Length / 1MB) | Write-Host
