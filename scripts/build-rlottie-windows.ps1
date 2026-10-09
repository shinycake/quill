# Build the pinned rlottie (rlottie.dll) with MSVC. Output: vendor/rlottie/prefix/bin/rlottie.dll
. "$PSScriptRoot/windows-common.ps1"
$root = (Resolve-Path "$PSScriptRoot/..").Path
$dest = Join-Path $root 'vendor/rlottie'
$pin = 'ea06d2f29ba01b8d06c00a838d107f5e484ae59b'   # keep in sync with scripts/build-rlottie.sh
$srcDir = Join-Path $dest 'source'
if (-not (Test-Path (Join-Path $srcDir '.git'))) {
    Invoke-Native git @('clone', 'https://github.com/Samsung/rlottie.git', $srcDir)
}
Invoke-Native git @('-C', $srcDir, 'checkout', '--detach', $pin)
$prefix = Join-Path $dest 'prefix'
Invoke-Native cmake @('-S', $srcDir, '-B', (Join-Path $dest 'build'), '-A', 'x64',
    '-DCMAKE_POLICY_VERSION_MINIMUM=3.5', '-DBUILD_SHARED_LIBS=ON', '-DLOTTIE_MODULE=OFF', '-DLOTTIE_TEST=OFF',
    '-DCMAKE_WINDOWS_EXPORT_ALL_SYMBOLS=ON', "-DCMAKE_INSTALL_PREFIX=$prefix",
    # Static C runtime (/MT): the package ships no vcruntime140*/msvcp140* DLLs.
    '-DCMAKE_POLICY_DEFAULT_CMP0091=NEW', '-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded')
Invoke-Native cmake @('--build', (Join-Path $dest 'build'), '--config', 'Release', '--parallel')
Invoke-Native cmake @('--install', (Join-Path $dest 'build'), '--config', 'Release')
$bin = Join-Path $prefix 'bin'
New-Item -ItemType Directory -Force $bin | Out-Null
if (-not (Test-Path (Join-Path $bin 'rlottie.dll'))) {
    $dll = Get-ChildItem -Recurse -Path (Join-Path $dest 'build'), $prefix -Filter rlottie.dll | Select-Object -First 1
    if (-not $dll) { throw 'rlottie.dll was not built' }
    Copy-Item $dll.FullName $bin
}
$deps = Get-PeDependents (Join-Path $bin 'rlottie.dll')
Write-Host "rlottie.dll imports: $($deps -join ' ')"
$dynamic = @($deps | Where-Object { $_ -match '^(vcruntime|msvcp|concrt)' })
if ($dynamic.Count) { throw "rlottie.dll still imports $($dynamic -join ', ') (expected a static /MT build)" }
Write-Host "Built $(Join-Path $bin 'rlottie.dll')"
