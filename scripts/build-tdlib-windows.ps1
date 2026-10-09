# Build the pinned TDLib (tdjson.dll) with the reviewed Quill export patch on
# Windows (MSVC + vcpkg for OpenSSL/zlib/gperf, as in TDLib's own instructions).
# Output: native/prefix/bin/tdjson.dll, with OpenSSL, zlib and the C runtime
# linked statically (/MT, vcpkg x64-windows-static), so the package ships no
# libssl/libcrypto/z.dll and no vcruntime140*/msvcp140* DLLs
# (docs/decisions/codex-release-pipeline.md).
# Run from an MSVC developer environment. Does not download prebuilt binaries.
. "$PSScriptRoot/windows-common.ps1"
$root = (Resolve-Path "$PSScriptRoot/..").Path
$pin = Get-QuillPin $root 'TDLIB_GIT_COMMIT'
$version = Get-QuillPin $root 'TDLIB_CMAKE_VERSION'
$src = if ($env:TDLIB_SRC) { $env:TDLIB_SRC } else { Join-Path $root 'native/td' }
$build = if ($env:TDLIB_BUILD) { $env:TDLIB_BUILD } else { Join-Path $root 'native/build' }
$prefix = if ($env:TDLIB_PREFIX) { $env:TDLIB_PREFIX } else { Join-Path $root 'native/prefix' }
$vcpkgRoot = if ($env:VCPKG_INSTALLATION_ROOT) { $env:VCPKG_INSTALLATION_ROOT } else { 'C:\vcpkg' }
Write-Host "Pinned TDLib $version @ $pin"

if (-not (Test-Path (Join-Path $src '.git'))) {
    Invoke-Native git @('clone', '--depth', '1', 'https://github.com/tdlib/td.git', $src)
    Invoke-Native git @('-C', $src, 'fetch', '--depth', '1', 'origin', $pin)
    Invoke-Native git @('-C', $src, 'checkout', $pin)
}
$head = (& git -C $src rev-parse HEAD).Trim()
if ($head -ne $pin) { throw "$src is $head, expected $pin" }
$patch = Join-Path $root 'native/patches/tdlib-quill-takeout-contacts.patch'
& git -C $src diff --quiet
if ($LASTEXITCODE -eq 0) {
    Invoke-Native git @('-C', $src, 'apply', '--check', $patch)
    Invoke-Native git @('-C', $src, 'apply', $patch)
} else {
    $applied = ((& git -C $src diff) -join "`n") -replace "`r`n", "`n"
    $expected = (Get-Content -Raw $patch) -replace "`r`n", "`n"
    if ($applied.TrimEnd() -ne $expected.TrimEnd()) { throw "$src has changes other than the pinned Quill export patch" }
}

$vcpkg = Join-Path $vcpkgRoot 'vcpkg.exe'
if (-not (Test-Path $vcpkg)) { throw "vcpkg not found at $vcpkgRoot (set VCPKG_INSTALLATION_ROOT)" }
$triplet = 'x64-windows-static'
Invoke-Native $vcpkg @('install', "openssl:$triplet", "zlib:$triplet", "gperf:$triplet", '--clean-after-build')
$installed = Join-Path $vcpkgRoot "installed\$triplet"
$env:PATH = (Join-Path $installed 'tools\gperf') + ';' + $env:PATH

# No LTO: MSVC /GL + /LTCG on tdjson roughly doubles the (already long) link.
# Static C runtime: TDLib requires only CMake 3.10, so CMP0091 (which makes
# CMAKE_MSVC_RUNTIME_LIBRARY effective) has to be switched on explicitly.
Invoke-Native cmake @('-S', $src, '-B', $build, '-A', 'x64',
    "-DCMAKE_TOOLCHAIN_FILE=$vcpkgRoot\scripts\buildsystems\vcpkg.cmake",
    "-DVCPKG_TARGET_TRIPLET=$triplet", "-DCMAKE_INSTALL_PREFIX=$prefix",
    '-DCMAKE_POLICY_DEFAULT_CMP0091=NEW', '-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded')
Invoke-Native cmake @('--build', $build, '--target', 'tdjson', '--config', 'Release', '--parallel')

New-Item -ItemType Directory -Force (Join-Path $prefix 'bin') | Out-Null
$dll = Get-ChildItem -Recurse -Path $build -Filter tdjson.dll | Select-Object -First 1
if (-not $dll) { throw "tdjson.dll was not built under $build" }
# Everything is static, so tdjson.dll is the only DLL to ship. Fail if it still
# imports OpenSSL, zlib or the dynamic VC++ runtime.
Remove-Item (Join-Path $prefix 'bin\*.dll') -ErrorAction SilentlyContinue
Copy-Item $dll.FullName (Join-Path $prefix 'bin')
$deps = Get-PeDependents (Join-Path $prefix 'bin\tdjson.dll')
Write-Host "tdjson.dll imports: $($deps -join ' ')"
$dynamic = @($deps | Where-Object { $_ -match '^(vcruntime|msvcp|concrt|libssl|libcrypto|zlib|z\.dll)' })
if ($dynamic.Count) { throw "tdjson.dll still imports $($dynamic -join ', ') (expected a static /MT build)" }
$hash = (Get-FileHash -Algorithm SHA256 (Join-Path $prefix 'bin\tdjson.dll')).Hash.ToLower()
"$hash  tdjson.dll" | Tee-Object (Join-Path $prefix 'tdjson.sha256')
Write-Host "Built $(Join-Path $prefix 'bin\tdjson.dll')"
