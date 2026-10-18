# Install or update Quill on Windows from the latest GitHub release, for the
# current user and without administrator rights. In PowerShell:
#   irm https://raw.githubusercontent.com/shinycake/quill/main/scripts/windows-install.ps1 | iex
#
# Quill isn't signed with a paid code-signing certificate, so a copy downloaded
# in a browser carries the "Mark of the Web" and SmartScreen stops its first
# start ("Windows protected your PC"). Files PowerShell downloads aren't
# marked, and this script clears the mark in case something added it. The
# zip is checked against the release's SHA256SUMS.txt before anything is
# installed. Quill goes to %LOCALAPPDATA%\Programs\Quill with a Start menu
# shortcut and an entry in Settings > Apps for removing it.
#
# Runs on Windows PowerShell 5.1 (built into Windows 10 and 11) and PowerShell 7.
#
# Environment:
#   QUILL_INSTALL_DIR  folder to install into (default %LOCALAPPDATA%\Programs\Quill)
#   QUILL_VERSION      release tag such as v2026.1017.2948 (default: latest)
#   QUILL_ZIP_URL      zip URL; skips the checksum unless QUILL_SHA256 is set
#   QUILL_NO_OPEN=1    don't start Quill when done
#   QUILL_NO_SHORTCUT=1  skip the Start menu shortcut and the Settings > Apps entry

# One script block: piped into iex, nothing runs until all of it has arrived,
# and `return` ends the install without closing the window.
& {
Set-StrictMode -Version 3
$ErrorActionPreference = 'Stop'
# Invoke-WebRequest's progress bar slows downloads tenfold in PowerShell 5.1.
$ProgressPreference = 'SilentlyContinue'

$repo = 'shinycake/quill'
$name = 'quill-windows-x86_64'
$asset = "$name.zip"
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Quill'

function Say([string] $text) { Write-Host $text }
function Fail([string] $text) { throw "quill install: $text" }

function Get-Url([string] $url, [string] $outFile) {
    if ($outFile) {
        Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $outFile
    } else {
        # Text, decoded here: a byte[] returned from a function is unrolled.
        $content = (Invoke-WebRequest -UseBasicParsing -Uri $url).Content
        if ($content -is [byte[]]) { $content = [Text.Encoding]::UTF8.GetString($content) }
        $content
    }
}

# The copies of quill.exe running from this install. Matched by path, so a
# Quill run from somewhere else is left alone.
function Get-Running([string] $exe) {
    @(Get-Process -Name quill -ErrorAction SilentlyContinue | Where-Object {
        try { $_.Path -and ([IO.Path]::GetFullPath($_.Path) -eq $exe) } catch { $false }
    })
}

try {
    if ([Environment]::OSVersion.Platform -ne 'Win32NT') { Fail 'this installer is for Windows' }
    if ([Environment]::OSVersion.Version.Major -lt 10) { Fail 'Quill needs Windows 10 or newer' }
    # ARM64 Windows 11 runs the x64 build through its built-in emulation.
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) { $arch = $env:PROCESSOR_ARCHITEW6432 }
    if ($arch -ne 'AMD64' -and $arch -ne 'ARM64') { Fail "Quill needs 64-bit Windows (this is $arch)" }
    # Windows PowerShell 5.1 may still default to TLS 1.0, which GitHub refuses.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $dest = $env:QUILL_INSTALL_DIR
    if (-not $dest) { $dest = Join-Path $env:LOCALAPPDATA 'Programs\Quill' }
    $dest = [IO.Path]::GetFullPath($dest).TrimEnd('\', '/')
    $exe = Join-Path $dest 'quill.exe'

    if ($env:QUILL_ZIP_URL) {
        $zipUrl = $env:QUILL_ZIP_URL
        $sumsUrl = $null
    } else {
        if ($env:QUILL_VERSION) {
            $base = "https://github.com/$repo/releases/download/$($env:QUILL_VERSION)"
        } else {
            $base = "https://github.com/$repo/releases/latest/download"
        }
        $zipUrl = "$base/$asset"
        $sumsUrl = "$base/SHA256SUMS.txt"
    }

    $want = $env:QUILL_SHA256
    if (-not $want -and $sumsUrl) {
        try { $sums = Get-Url $sumsUrl } catch { Fail "couldn't fetch the release checksums ($($_.Exception.Message))" }
        foreach ($line in ($sums -split "`n")) {
            $fields = $line.Trim() -split '\s+'
            if ($fields.Count -eq 2 -and $fields[1] -eq $asset) { $want = $fields[0] }
        }
        if (-not $want) { Fail "the release checksums don't list $asset" }
    }

    $work = Join-Path ([IO.Path]::GetTempPath()) ("quill-install-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory $work | Out-Null
    try {
        $zip = Join-Path $work $asset
        Say "Downloading $zipUrl"
        try { Get-Url $zipUrl $zip } catch { Fail "download failed ($($_.Exception.Message))" }
        if ($want) {
            $got = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
            if ($got -ne $want.ToLowerInvariant()) { Fail "checksum mismatch (expected $want, got $got)" }
            Say 'Checksum OK'
        }

        $unpacked = Join-Path $work 'unpacked'
        Expand-Archive -LiteralPath $zip -DestinationPath $unpacked
        $pkg = Join-Path $unpacked $name
        if (-not (Test-Path -LiteralPath (Join-Path $pkg 'quill.exe'))) { Fail 'the archive has no quill.exe' }

        # Replace a running copy only after it has quit: Windows keeps a
        # running program's files locked.
        if (@(Get-Running $exe).Count) {
            Say 'Quitting Quill to update it'
            try { Start-Process -FilePath $exe -ArgumentList '--quit' -WindowStyle Hidden } catch { }
            for ($i = 0; $i -lt 20 -and @(Get-Running $exe).Count; $i++) { Start-Sleep -Milliseconds 500 }
            if (@(Get-Running $exe).Count) { Fail 'Quill is still running; quit it and run this again' }
        }

        # Stage beside the destination, then swap with renames on one volume,
        # so an interrupted install never leaves a half-deleted Quill.
        $parent = Split-Path -Parent $dest
        New-Item -ItemType Directory -Force $parent | Out-Null
        $staged = "$dest.installing"
        $old = "$dest.old"
        # An earlier run stopped between its two renames: put Quill back first.
        if (-not (Test-Path -LiteralPath $dest) -and (Test-Path -LiteralPath $old)) {
            Move-Item -LiteralPath $old -Destination $dest
        }
        foreach ($p in $staged, $old) {
            if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath $p -Recurse -Force }
        }
        Move-Item -LiteralPath $pkg -Destination $staged
        Get-ChildItem -LiteralPath $staged -Recurse -File | Unblock-File
        if (Test-Path -LiteralPath $dest) { Move-Item -LiteralPath $dest -Destination $old }
        # Antivirus scanning the new files can hold them for a moment.
        $moved = $false
        for ($i = 0; $i -lt 10 -and -not $moved; $i++) {
            try { Move-Item -LiteralPath $staged -Destination $dest; $moved = $true } catch { Start-Sleep -Milliseconds 500 }
        }
        if (-not $moved) {
            if (Test-Path -LiteralPath $old) { Move-Item -LiteralPath $old -Destination $dest }
            Fail "couldn't replace $dest; close any window showing that folder and run this again"
        }
        if (Test-Path -LiteralPath $old) {
            Remove-Item -LiteralPath $old -Recurse -Force -ErrorAction SilentlyContinue
        }
    } finally {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }

    $version = (Get-Item -LiteralPath $exe).VersionInfo.ProductVersion

    if ($env:QUILL_NO_SHORTCUT -ne '1') {
        $programs = [Environment]::GetFolderPath('Programs')
        $shell = New-Object -ComObject WScript.Shell
        $link = $shell.CreateShortcut((Join-Path $programs 'Quill.lnk'))
        $link.TargetPath = $exe
        $link.WorkingDirectory = $dest
        $link.IconLocation = "$exe,0"
        $link.Description = 'Quill, a Telegram client'
        $link.Save()

        # Settings > Apps entry. The command lives in the registry, not in the
        # install folder, because in-app updates replace that folder. Account
        # data (in %APPDATA%) is kept.
        $uninstall = @"
`$ErrorActionPreference = 'SilentlyContinue'
`$dest = '$([Management.Automation.Language.CodeGeneration]::EscapeSingleQuotedStringContent($dest))'
`$exe = Join-Path `$dest 'quill.exe'
Get-Process -Name quill | Where-Object { `$_.Path -eq `$exe } | ForEach-Object { Start-Process `$exe '--quit' -WindowStyle Hidden; `$_.WaitForExit(10000) | Out-Null }
Remove-Item -LiteralPath `$dest -Recurse -Force
# Keep the Settings > Apps entry while anything is left to remove.
if (Test-Path -LiteralPath `$dest) { exit 1 }
Remove-Item -LiteralPath (Join-Path ([Environment]::GetFolderPath('Programs')) 'Quill.lnk') -Force
`$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if ("`$((Get-ItemProperty `$run).Quill)".Contains(`$exe)) { Remove-ItemProperty `$run -Name Quill }
`$tg = 'HKCU:\Software\Classes\tg\shell\open\command'
if ("`$((Get-ItemProperty `$tg).'(default)')".Contains(`$exe)) { Remove-Item 'HKCU:\Software\Classes\tg' -Recurse -Force }
Remove-Item 'HKCU:\Software\Classes\AppUserModelId\org.shinycake.quill' -Recurse -Force
Remove-Item '$uninstallKey' -Recurse -Force
exit 0
"@
        $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($uninstall))
        New-Item -Path $uninstallKey -Force | Out-Null
        $entry = @{
            DisplayName     = 'Quill'
            Publisher       = 'Quill (open source)'
            DisplayIcon     = "$exe,0"
            InstallLocation = $dest
            URLInfoAbout    = "https://github.com/$repo"
            UninstallString = "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -EncodedCommand $encoded"
        }
        foreach ($k in $entry.Keys) { New-ItemProperty -Path $uninstallKey -Name $k -Value $entry[$k] -Force | Out-Null }
        foreach ($k in 'NoModify', 'NoRepair') {
            New-ItemProperty -Path $uninstallKey -Name $k -Value 1 -PropertyType DWord -Force | Out-Null
        }
    }

    if ($version) { Say "Installed Quill $version to $dest" } else { Say "Installed Quill to $dest" }
    if ($env:QUILL_NO_OPEN -ne '1') { Start-Process -FilePath $exe -WorkingDirectory $dest }
    $global:LASTEXITCODE = 0
} catch {
    # No `exit`: run through iex, it would close the PowerShell window.
    Write-Host $_.Exception.Message -ForegroundColor Red
    $global:LASTEXITCODE = 1
}
}
