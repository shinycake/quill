# Build the pinned TDLib (tdjson.dll) with the reviewed Quill export patch on
# Windows (MSVC + vcpkg for OpenSSL/zlib/gperf, as in TDLib's own instructions).
# Output: native/prefix/bin/{tdjson.dll, libssl-3-x64.dll, libcrypto-3-x64.dll, z.dll}
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
Invoke-Native $vcpkg @('install', 'openssl:x64-windows', 'zlib:x64-windows', 'gperf:x64-windows', '--clean-after-build')
$installed = Join-Path $vcpkgRoot 'installed\x64-windows'
$env:PATH = (Join-Path $installed 'tools\gperf') + ';' + $env:PATH

# No LTO: MSVC /GL + /LTCG on tdjson roughly doubles the (already long) link.
Invoke-Native cmake @('-S', $src, '-B', $build, '-A', 'x64',
    "-DCMAKE_TOOLCHAIN_FILE=$vcpkgRoot\scripts\buildsystems\vcpkg.cmake",
    '-DVCPKG_TARGET_TRIPLET=x64-windows', "-DCMAKE_INSTALL_PREFIX=$prefix")
Invoke-Native cmake @('--build', $build, '--target', 'tdjson', '--config', 'Release', '--parallel')

New-Item -ItemType Directory -Force (Join-Path $prefix 'bin') | Out-Null
$dll = Get-ChildItem -Recurse -Path $build -Filter tdjson.dll | Select-Object -First 1
if (-not $dll) { throw "tdjson.dll was not built under $build" }
# The vcpkg toolchain's app-local step puts the runtime DLLs (OpenSSL and zlib,
# which vcpkg names z.dll) next to tdjson.dll; ship exactly that set.
foreach ($f in Get-ChildItem -File $dll.DirectoryName -Filter *.dll) { Copy-Item $f.FullName (Join-Path $prefix 'bin') }
foreach ($name in 'tdjson.dll', 'libssl-3-x64.dll', 'libcrypto-3-x64.dll') {
    if (-not (Test-Path (Join-Path $prefix "bin\$name"))) { throw "$name missing from $prefix\bin" }
}
$hash = (Get-FileHash -Algorithm SHA256 (Join-Path $prefix 'bin\tdjson.dll')).Hash.ToLower()
"$hash  tdjson.dll" | Tee-Object (Join-Path $prefix 'tdjson.sha256')
Write-Host "Built $(Join-Path $prefix 'bin\tdjson.dll')"
