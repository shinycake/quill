# Fail if any PE file in the Windows package imports a DLL that is neither in the
# package nor a Windows system DLL. Needs dumpbin.exe (MSVC developer environment).
#   pwsh scripts/check-bundle-pe.ps1 <package dir>
param([Parameter(Mandatory)][string]$Dir)
. "$PSScriptRoot/windows-common.ps1"
$Dir = (Resolve-Path $Dir).Path

foreach ($required in 'quill.exe', 'tdjson.dll', 'ntgcalls.dll', 'rlottie.dll', 'libssl-3-x64.dll', 'libcrypto-3-x64.dll') {
    if (-not (Test-Path (Join-Path $Dir $required))) { throw "package is missing $required" }
}

$bundled = @{}
Get-ChildItem -File $Dir | ForEach-Object { $bundled[$_.Name.ToLowerInvariant()] = $true }

$failures = @()
$checked = 0
foreach ($file in Get-ChildItem -File $Dir | Where-Object { $_.Extension -in '.exe', '.dll' }) {
    $checked++
    $deps = Get-PeDependents $file.FullName
    foreach ($dep in $deps) {
        if ($bundled.ContainsKey($dep)) { continue }
        if (Test-SystemDll $dep) { continue }
        $failures += "$($file.Name) imports $dep, which is neither bundled nor a Windows system DLL"
    }
    Write-Host ("{0,-24} {1}" -f $file.Name, ($deps -join ' '))
}
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Error $_ -ErrorAction Continue }
    exit 1
}
Write-Host "check-bundle-pe: OK ($checked PE files in $Dir)"
