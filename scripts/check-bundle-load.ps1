# Load tdjson/ntgcalls/rlottie/quillvideo from the package and assert the OpenSSL/zlib, FFmpeg and
# VC runtime DLLs mapped into this process came from the package directory.
# Run with the package directory's DLLs not on PATH.   pwsh scripts/check-bundle-load.ps1 <dir>
param([Parameter(Mandatory)][string]$Dir)
$ErrorActionPreference = 'Stop'
$Dir = (Resolve-Path $Dir).Path
foreach ($name in 'tdjson.dll', 'ntgcalls.dll', 'rlottie.dll', 'quillvideo.dll') {
    $path = Join-Path $Dir $name
    $handle = [System.Runtime.InteropServices.NativeLibrary]::Load($path)
    Write-Host "loaded $name"
    if ($name -eq 'tdjson.dll') {
        foreach ($sym in 'td_create_client_id', 'td_send', 'td_receive', 'td_execute') {
            $addr = [System.Runtime.InteropServices.NativeLibrary]::GetExport($handle, $sym)
            if ($addr -eq [IntPtr]::Zero) { throw "tdjson.dll does not export $sym" }
        }
    }
    if ($name -eq 'quillvideo.dll') {
        foreach ($sym in 'qv_abi_version', 'qv_open', 'qv_next', 'qv_seek', 'qv_close') {
            $addr = [System.Runtime.InteropServices.NativeLibrary]::GetExport($handle, $sym)
            if ($addr -eq [IntPtr]::Zero) { throw "quillvideo.dll does not export $sym" }
        }
    }
}
$bad = @()
foreach ($m in [System.Diagnostics.Process]::GetCurrentProcess().Modules) {
    if ($m.ModuleName -match '^(libssl-3|libcrypto-3|z\.dll|vcruntime140|msvcp140|av(codec|format|util)-|sw(scale|resample)-)') {
        $inPkg = $m.FileName.StartsWith($Dir, [System.StringComparison]::OrdinalIgnoreCase)
        Write-Host ("{0,-28} {1}" -f $m.ModuleName, $m.FileName)
        # pwsh itself loads vcruntime/msvcp from its own directory, so only the
        # libraries that exist solely for Quill must come from the package.
        if (-not $inPkg -and $m.ModuleName -match '^(libssl-3|libcrypto-3|z\.dll|av(codec|format|util)-|sw(scale|resample)-)') { $bad += $m.FileName }
    }
}
if ($bad.Count) { throw "libraries not loaded from the package: $($bad -join ', ')" }
Write-Host 'check-bundle-load: OK'
