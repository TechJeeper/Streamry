# Build Streamry macOS universal DMG on a remote Mac via SSH from Windows.
# Universal = Intel (x86_64) + Apple Silicon (aarch64) via tauri --target universal-apple-darwin.
# 1. Create macOS source zip (zip-macos-build.ps1)
# 2. Copy zip to remote host
# 3. unzip, npm install, tauri universal build on remote
# 4. Optionally install built .app to /Applications on the remote Mac
# 5. Copy .dmg back to local dist/ and dist-packages/
#
# Requires: OpenSSH client (scp/ssh).
# Auth: SSH keys (recommended). Same host/key setup as Printventory.
# Windows OpenSSH connection multiplexing is unreliable; it is off by default (set MAC_BUILD_SSH_MULTIPLEX=1 to try).
# Default host: cody@192.168.68.92 (override with -SshHost or MAC_BUILD_SSH_HOST).
#
# One-time key setup (no passwords afterward):
#   ssh-keygen -t ed25519 -f $env:USERPROFILE\.ssh\id_ed25519_printventory
#   Get-Content $env:USERPROFILE\.ssh\id_ed25519_printventory.pub | ssh cody@192.168.68.92 "mkdir -p ~/.ssh && chmod 700 ~/.ssh && cat >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"
#   setx MAC_BUILD_SSH_HOST "cody@192.168.68.92"   # optional
# Add to $env:USERPROFILE\.ssh\config:
#   Host 192.168.68.92
#     IdentityFile ~/.ssh/id_ed25519_printventory

param(
    [string]$SshHost = $(if ($env:MAC_BUILD_SSH_HOST) { $env:MAC_BUILD_SSH_HOST } else { "cody@192.168.68.92" }),
    [string]$RemoteBase = $(if ($env:MAC_BUILD_REMOTE_DIR) { $env:MAC_BUILD_REMOTE_DIR } else { "~/streamry-remote-build" }),
    [switch]$SkipRemoteInstall = [bool]($env:MAC_BUILD_SKIP_INSTALL -eq "1")
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$rootDir = Split-Path -Parent $scriptDir
Set-Location $rootDir

function Require-Command($name) {
    if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
        Write-Error "Required command not found: $name. Install OpenSSH Client on Windows."
    }
}

Require-Command ssh
Require-Command scp

$sshControlDir = Join-Path $env:USERPROFILE ".ssh\streamry-cm"
$controlId = ($SshHost -replace '[^\w@.-]', '_') -replace '@', '_'
$sshControlFile = Join-Path $sshControlDir "cm-$controlId"
$sshControlPath = $sshControlFile.Replace('\', '/')
$script:MultiplexEnabled = $false
$script:MacBuildSshOpts = @(
    "-o", "ControlMaster=auto",
    "-o", "ControlPath=$sshControlPath",
    "-o", "ControlPersist=600"
)
if ($env:OS -eq "Windows_NT" -and $env:MAC_BUILD_SSH_MULTIPLEX -ne "1") {
    $script:MacBuildSshOpts = @()
} else {
    $script:MultiplexEnabled = $true
}

function Invoke-SshQuiet {
    param(
        [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
        [string[]]$SshArguments
    )
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "SilentlyContinue"
    try {
        & ssh.exe @SshArguments 2>&1 | Out-Null
        return $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $prev
    }
}

$script:SshKeepAlive = @("-o", "ServerAliveInterval=30", "-o", "ServerAliveCountMax=120", "-o", "BatchMode=yes")

function Invoke-SshCommand {
    param([string]$RemoteCmd)
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "SilentlyContinue"
    try {
        & ssh.exe @($script:MacBuildSshOpts + $script:SshKeepAlive + @($SshHost, $RemoteCmd))
        if ($LASTEXITCODE -ne 0) {
            Write-Error "SSH command failed on $SshHost (exit $LASTEXITCODE)."
        }
    } finally {
        $ErrorActionPreference = $prev
    }
}

function Invoke-ScpCommand {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "SilentlyContinue"
    try {
        & scp.exe @($script:MacBuildSshOpts + $script:SshKeepAlive + @($Source, $Destination))
        return $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $prev
    }
}

function Remove-StaleControlSocket {
    if (-not $script:MultiplexEnabled) { return }
    if (Test-Path -LiteralPath $sshControlFile) {
        Remove-Item -LiteralPath $sshControlFile -Force -ErrorAction SilentlyContinue
    }
}

function Close-SshMultiplex {
    if (-not $script:MultiplexEnabled) { return }

    $checkCode = Invoke-SshQuiet @($script:MacBuildSshOpts + @("-O", "check", $SshHost))
    if ($checkCode -eq 0) {
        Invoke-SshQuiet @($script:MacBuildSshOpts + @("-O", "exit", $SshHost)) | Out-Null
    }
    Remove-StaleControlSocket
}

function Get-RemoteShellPath([string]$Path) {
    if ($Path -match '^~(/.*)?$') {
        return ('$HOME' + $Matches[1])
    }
    return $Path
}

function Invoke-RemoteZsh([string]$TargetHost, [string]$Script, [switch]$Capture) {
    # Windows here-strings and PowerShell piping both inject CRLF; zsh treats trailing CR as
    # part of the command. Write LF bytes to ssh stdin.
    $Script = (($Script -replace "`r`n", "`n") -replace "`r", "`n").TrimEnd() + "`n"
    $scriptBytes = [System.Text.Encoding]::UTF8.GetBytes($Script)
    $echoLive = -not $Capture

    $argList = @($script:MacBuildSshOpts) + @($TargetHost, "zsh -ils")
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "ssh.exe"
    $psi.Arguments = ($argList | ForEach-Object {
        if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
    }) -join ' '
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true

    $proc = New-Object System.Diagnostics.Process
    $proc.StartInfo = $psi
    $stdoutBuilder = New-Object System.Text.StringBuilder
    $stderrBuilder = New-Object System.Text.StringBuilder

    $outEvent = Register-ObjectEvent -InputObject $proc -EventName OutputDataReceived -MessageData @{
        Builder = $stdoutBuilder
        Echo = $echoLive
    } -Action {
        if (-not [string]::IsNullOrEmpty($EventArgs.Data)) {
            [void]$Event.MessageData.Builder.AppendLine($EventArgs.Data)
            if ($Event.MessageData.Echo) { Write-Host $EventArgs.Data }
        }
    }
    $errEvent = Register-ObjectEvent -InputObject $proc -EventName ErrorDataReceived -MessageData @{
        Builder = $stderrBuilder
        Echo = $echoLive
    } -Action {
        if (-not [string]::IsNullOrEmpty($EventArgs.Data)) {
            [void]$Event.MessageData.Builder.AppendLine($EventArgs.Data)
            if ($Event.MessageData.Echo) { Write-Host $EventArgs.Data }
        }
    }

    [void]$proc.Start()
    $proc.BeginOutputReadLine()
    $proc.BeginErrorReadLine()
    $proc.StandardInput.BaseStream.Write($scriptBytes, 0, $scriptBytes.Length)
    $proc.StandardInput.Close()
    $proc.WaitForExit()

    Unregister-Event -SourceIdentifier $outEvent.Name -ErrorAction SilentlyContinue
    Unregister-Event -SourceIdentifier $errEvent.Name -ErrorAction SilentlyContinue
    Remove-Job $outEvent.Id -Force -ErrorAction SilentlyContinue
    Remove-Job $errEvent.Id -Force -ErrorAction SilentlyContinue

    $combined = ($stdoutBuilder.ToString() + $stderrBuilder.ToString()).TrimEnd()
    if ($proc.ExitCode -ne 0) {
        if ($Capture -and $combined) { Write-Host $combined }
        throw "Remote zsh command failed (exit code $($proc.ExitCode))"
    }
    if ($Capture) {
        return $combined
    }
}

$packageJson = Get-Content -Raw -Path "package.json" | ConvertFrom-Json
$version = $packageJson.version
$stagingName = "streamry-macos-build-$version"
$zipName = "$stagingName.zip"
$zipDistDir = "dist-packages"
$distDir = "dist"
$zipPath = Join-Path $zipDistDir $zipName
$remoteZipPath = "$RemoteBase/$zipName"
$remoteShellBase = Get-RemoteShellPath $RemoteBase
$remoteShellProject = "$remoteShellBase/$stagingName"
$remoteUniversalBundle = "$remoteShellProject/src-tauri/target/universal-apple-darwin/release/bundle"

Write-Host "=== Streamry remote macOS universal build ===" -ForegroundColor Cyan
Write-Host "Version:   $version"
Write-Host "SSH host:  $SshHost"
Write-Host "Remote:    $RemoteBase"
Write-Host "Target:    universal-apple-darwin (x86_64 + aarch64)"
Write-Host ""

# Step 1: Create macOS build zip locally
Write-Host "[1/5] Creating macOS build zip..." -ForegroundColor Yellow
& (Join-Path $scriptDir "zip-macos-build.ps1")
if (-not (Test-Path $zipPath)) {
    Write-Error "Zip not found after packaging: $zipPath"
}
$zipPath = (Resolve-Path $zipPath).Path

# Steps 2-4: remote upload, build, download
try {
if ($script:MultiplexEnabled) {
    if (-not (Test-Path $sshControlDir)) {
        New-Item -ItemType Directory -Path $sshControlDir -Force | Out-Null
    }
    Remove-StaleControlSocket
} else {
    Write-Host "Tip: set up SSH keys to skip repeated password prompts (see scripts/build-mac-remote.ps1 header)." -ForegroundColor DarkGray
}

# Step 2: Copy zip to remote Mac
Write-Host ""
Write-Host "[2/5] Uploading zip to $SshHost ..." -ForegroundColor Yellow
Invoke-SshCommand "mkdir -p $RemoteBase"
if ((Invoke-ScpCommand -Source $zipPath -Destination "${SshHost}:${remoteZipPath}") -ne 0) {
    Write-Error "Failed to upload zip to $SshHost"
}
Write-Host "Uploaded: $remoteZipPath" -ForegroundColor Green

# Step 3: Upload and run a remote build script (avoids interactive zsh stdin parsing issues)
Write-Host ""
Write-Host "[3/5] Running npm install and tauri build on remote (this may take several minutes)..." -ForegroundColor Yellow
$remoteBuildShName = "streamry-remote-build.sh"
$localBuildSh = Join-Path $env:TEMP $remoteBuildShName
$remoteBuildShPath = "$RemoteBase/$remoteBuildShName"
$remoteBuildScript = @"
#!/bin/zsh
set -e
set -u
setopt pipefail
export PATH="/opt/homebrew/bin:/usr/local/bin:`$HOME/.cargo/bin:`$PATH"
[ -s "`$HOME/.cargo/env" ] && . "`$HOME/.cargo/env"
[ -s "`$HOME/.nvm/nvm.sh" ] && . "`$HOME/.nvm/nvm.sh"
command -v fnm >/dev/null && eval "`$(fnm env -s zsh)"
command -v npm >/dev/null || { echo "npm not found over SSH; install Node or add it to ~/.zprofile"; exit 127; }
command -v cargo >/dev/null || { echo "cargo not found over SSH; install Rust via rustup"; exit 127; }
rustup target add aarch64-apple-darwin x86_64-apple-darwin
mkdir -p $remoteShellBase
cd $remoteShellBase
chmod -R u+w '$stagingName' 2>/dev/null || true
rm -rf '$stagingName' || true
test ! -d '$stagingName' || mv '$stagingName' "$stagingName.old.`$(date +%s)"
unzip -o -q '$zipName'
cd '$stagingName'
npm install
npm run tauri build -- --target universal-apple-darwin --bundles dmg,app || true
BUNDLE="src-tauri/target/universal-apple-darwin/release/bundle"
DMG_DIR="`$BUNDLE/dmg"
APP_PATH="`$BUNDLE/macos/Streamry.app"
DMG_PATH="`$DMG_DIR/Streamry_${version}_universal.dmg"
# Prefer any Tauri-produced DMG; rename to universal if needed
setopt NULL_GLOB
tauri_dmg=(`$DMG_DIR/*.dmg)
if [ ! -f "`$DMG_PATH" ] && [ -n "`$tauri_dmg" ] && [ -f "`$tauri_dmg[1]" ]; then
  mv "`$tauri_dmg[1]" "`$DMG_PATH"
fi
# Tauri create-dmg can fail over headless SSH; fall back to hdiutil UDZO
if [ ! -f "`$DMG_PATH" ] && [ -d "`$APP_PATH" ]; then
  echo "Tauri DMG missing; creating universal DMG with hdiutil..."
  mkdir -p "`$DMG_DIR"
  tmpdir=`$(mktemp -d)
  cp -R "`$APP_PATH" "`$tmpdir/"
  ln -s /Applications "`$tmpdir/Applications"
  hdiutil create -volname Streamry -srcfolder "`$tmpdir" -ov -format UDZO "`$DMG_PATH"
  rm -rf "`$tmpdir"
fi
test -d "`$APP_PATH"
BIN="`$APP_PATH/Contents/MacOS/streamry"
ARCHS=`$(lipo -archs "`$BIN" 2>/dev/null || true)
echo "Binary arches: `$ARCHS"
echo "`$ARCHS" | grep -qw 'x86_64' || { echo "ERROR: missing x86_64"; lipo -info "`$BIN" || true; exit 1; }
echo "`$ARCHS" | grep -qw 'arm64' || { echo "ERROR: missing arm64"; lipo -info "`$BIN" || true; exit 1; }
test -f "`$DMG_PATH"
ls -1 "`$DMG_PATH"
"@
$remoteBuildScript = (($remoteBuildScript -replace "`r`n", "`n") -replace "`r", "`n").TrimEnd() + "`n"
[System.IO.File]::WriteAllText($localBuildSh, $remoteBuildScript, [System.Text.UTF8Encoding]::new($false))

if ((Invoke-ScpCommand -Source $localBuildSh -Destination "${SshHost}:${remoteBuildShPath}") -ne 0) {
    Write-Error "Failed to upload remote build script to $SshHost"
}

try {
    Invoke-SshCommand "zsh $remoteShellBase/$remoteBuildShName"
} catch {
    Write-Error "Remote build failed. Check npm/tauri output above. $($_.Exception.Message)"
}
Write-Host "Remote build finished." -ForegroundColor Green

$remoteDmgListing = & ssh.exe @($script:MacBuildSshOpts + @(
    $SshHost,
    "ls -1 $remoteUniversalBundle/dmg/*.dmg 2>/dev/null | head -1"
)) 2>&1 | Out-String
$remoteDmgPath = ($remoteDmgListing -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -match '\.dmg$' } | Select-Object -Last 1)
if (-not $remoteDmgPath) {
    Write-Error "No .dmg on remote after build. Expected: $remoteUniversalBundle/dmg/*.dmg"
}
Write-Host "Remote DMG: $remoteDmgPath" -ForegroundColor Green

if (-not $SkipRemoteInstall) {
    Write-Host ""
    Write-Host "[4/5] Installing app on remote Mac (/Applications) ..." -ForegroundColor Yellow
    $remoteInstallScript = @"
set -e
set -u
setopt pipefail
DMG='$remoteDmgPath'
if [ ! -f "`$DMG" ]; then
  echo "DMG not found: `$DMG"
  exit 1
fi

osascript -e 'quit app "Streamry"' 2>/dev/null || true
sleep 1

MOUNT_DIR=`$(hdiutil attach "`$DMG" -nobrowse -noverify -noautofsck | grep -o '/Volumes/.*' | head -1 | tr -d '\r')
if [ -z "`$MOUNT_DIR" ] || [ ! -d "`$MOUNT_DIR" ]; then
  echo "Failed to mount DMG: `$DMG"
  exit 1
fi

APP_SRC=`$(find "`$MOUNT_DIR" -maxdepth 1 -name '*.app' -print -quit)
if [ -z "`$APP_SRC" ]; then
  hdiutil detach "`$MOUNT_DIR" -quiet || true
  echo "No .app bundle found in DMG"
  exit 1
fi

APP_NAME=`$(basename "`$APP_SRC")
DEST="/Applications/`$APP_NAME"
echo "Installing `$APP_NAME to `$DEST ..."
rm -rf "`$DEST"
ditto "`$APP_SRC" "`$DEST"
hdiutil detach "`$MOUNT_DIR" -quiet
echo "Installed: `$DEST"
test -d "`$DEST"
"@

    try {
        Invoke-RemoteZsh $SshHost $remoteInstallScript
    } catch {
        Write-Error "Remote install failed. $($_.Exception.Message)"
    }
    Write-Host "Remote install finished. Open Streamry from /Applications or Spotlight on the Mac." -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "[4/5] Skipping remote install (SkipRemoteInstall / MAC_BUILD_SKIP_INSTALL=1)." -ForegroundColor DarkGray
}

# Step 5: Copy DMG back to local dist/ (and dist-packages/)
Write-Host ""
Write-Host "[5/5] Downloading universal DMG to $distDir ..." -ForegroundColor Yellow
foreach ($dir in @($distDir, $zipDistDir)) {
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir | Out-Null
    }
}

$beforeDmgs = @(Get-ChildItem -Path $distDir -Filter "*.dmg" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })

$localDistDir = (Resolve-Path $distDir).Path
if ((Invoke-ScpCommand -Source "${SshHost}:${remoteDmgPath}" -Destination $localDistDir) -ne 0) {
    Write-Error "Failed to download DMG from $SshHost"
}

$localDmgs = Get-ChildItem -Path $distDir -Filter "*universal*.dmg" -ErrorAction SilentlyContinue |
    Where-Object { $beforeDmgs -notcontains $_.FullName } |
    Sort-Object LastWriteTime -Descending
if (-not $localDmgs) {
    $localDmgs = Get-ChildItem -Path $distDir -Filter "*.dmg" -ErrorAction SilentlyContinue |
        Where-Object { $beforeDmgs -notcontains $_.FullName } |
        Sort-Object LastWriteTime -Descending
}
if (-not $localDmgs) {
    $localDmgs = Get-ChildItem -Path $distDir -Filter "*.dmg" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
}
if (-not $localDmgs) {
    Write-Error "No .dmg found in local dist after download. Check remote path: $remoteDmgPath"
}

foreach ($dmg in $localDmgs) {
    Copy-Item -Path $dmg.FullName -Destination (Join-Path $zipDistDir $dmg.Name) -Force
}

Write-Host ""
Write-Host "=== Remote macOS universal build complete ===" -ForegroundColor Green
foreach ($dmg in $localDmgs) {
    $sizeMb = [math]::Round($dmg.Length / 1MB, 2)
    Write-Host "  $($dmg.FullName) ($sizeMb MB)"
}
} finally {
    Close-SshMultiplex
}
