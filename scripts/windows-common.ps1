# Shared helpers for the Windows build/package scripts (dot-source this file).
$ErrorActionPreference = 'Stop'

function Get-QuillPin([string]$Root, [string]$Name) {
    $text = Get-Content -Raw (Join-Path $Root 'src/pins.rs')
    if ($text -notmatch ('(?m)' + $Name + ': &str = "([^"]+)"')) { throw "pin $Name not found in src/pins.rs" }
    return $Matches[1]
}

function Invoke-Native {
    # Run a native command and fail on a non-zero exit (PowerShell does not).
    param([Parameter(Mandatory)][string]$Exe, [string[]]$Arguments)
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe exited with $LASTEXITCODE" }
}

function Invoke-QuillExe {
    # quill.exe is a GUI-subsystem binary: capture its stdout via a redirected
    # file rather than relying on the pipeline to wait for it.
    param([Parameter(Mandatory)][string]$Exe, [Parameter(Mandatory)][string[]]$Arguments)
    $out = [System.IO.Path]::GetTempFileName()
    try {
        $p = Start-Process -FilePath $Exe -ArgumentList $Arguments -Wait -PassThru -NoNewWindow -RedirectStandardOutput $out
        if ($p.ExitCode -ne 0) { throw "$Exe $Arguments exited with $($p.ExitCode)" }
        return (Get-Content -Raw $out).Trim()
    } finally { Remove-Item -Force $out -ErrorAction SilentlyContinue }
}

function Get-Dumpbin {
    $d = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    if (-not $d) { throw 'dumpbin.exe not found: run from an MSVC developer environment (ilammy/msvc-dev-cmd or vcvars64.bat)' }
    return $d.Source
}

function Get-PeDependents {
    # DLL names a PE file imports (static and delay-load), lower-cased.
    param([Parameter(Mandatory)][string]$Path)
    $lines = & (Get-Dumpbin) /nologo /dependents $Path
    if ($LASTEXITCODE -ne 0) { throw "dumpbin failed on $Path" }
    $deps = foreach ($line in $lines) {
        if ($line -match '^\s{2,}(\S+\.(?:dll|DLL))\s*$') { $Matches[1].ToLowerInvariant() }
    }
    return @($deps | Sort-Object -Unique)
}

# DLLs that exist in System32 on a dev machine but are not guaranteed on a clean
# Windows install (the VC runtime is bundled app-locally; the rest ships with us).
$script:NotSystem = '^(vcruntime|msvcp|concrt|vcomp|vccorlib|libcrypto|libssl|zlib|vulkan-1)'

function Test-SystemDll([string]$Name) {
    if ($Name -match '^(api-ms-win-|ext-ms-win-)') { return $true }
    if ($Name -match $script:NotSystem) { return $false }
    return (Test-Path (Join-Path $env:SystemRoot "System32\$Name"))
}

function Get-VcRedistDir {
    # Microsoft.VC14x.CRT directory holding the app-local VC++ runtime DLLs.
    if ($env:VCToolsRedistDir) {
        $d = Get-ChildItem -Directory (Join-Path $env:VCToolsRedistDir 'x64') -Filter 'Microsoft.VC*.CRT' -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($d) { return $d.FullName }
    }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path $vswhere) {
        $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($vs) {
            $d = Get-ChildItem -Directory (Join-Path $vs 'VC\Redist\MSVC\*\x64') -Filter 'Microsoft.VC*.CRT' -ErrorAction SilentlyContinue | Sort-Object FullName | Select-Object -Last 1
            if ($d) { return $d.FullName }
        }
    }
    throw 'VC++ redistributable directory not found'
}
