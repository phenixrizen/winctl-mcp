param(
    [string]$BaseDir = "$env:LOCALAPPDATA\winctl-mcp",
    [string]$ServerExe = "",
    [string]$ConfigPath = "",
    [string]$HealthUrl = "http://127.0.0.1:8765/healthz",
    [string]$TargetProcessName = "",
    [switch]$RunSelfTest
)

$ErrorActionPreference = "Continue"

if ([string]::IsNullOrWhiteSpace($ServerExe)) {
    $ServerExe = Join-Path $BaseDir "bin\winctl-mcp-server.exe"
}
if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
    $ConfigPath = Join-Path $BaseDir "config.toml"
}

function Write-Check {
    param(
        [string]$Name,
        [string]$Status,
        [string]$Detail = ""
    )
    if ([string]::IsNullOrWhiteSpace($Detail)) {
        Write-Host "[$Status] $Name"
    } else {
        Write-Host "[$Status] $Name - $Detail"
    }
}

Write-Host "winctl-mcp diagnostics"
Write-Host "BaseDir: $BaseDir"

if (Test-Path $ServerExe) {
    Write-Check "server executable" "ok" $ServerExe
} else {
    Write-Check "server executable" "missing" $ServerExe
}

if (Test-Path $ConfigPath) {
    Write-Check "config file" "ok" $ConfigPath
} else {
    Write-Check "config file" "missing" $ConfigPath
}

$dirs = @{
    logs = Join-Path $BaseDir "logs"
    captures = Join-Path $BaseDir "captures"
    artifacts = Join-Path $BaseDir "artifacts"
    memory = Join-Path $BaseDir "memory"
    models = Join-Path $BaseDir "models"
    manifests = Join-Path $BaseDir "manifests"
    replays = Join-Path $BaseDir "replays"
    exports = Join-Path $BaseDir "exports"
}

foreach ($entry in $dirs.GetEnumerator() | Sort-Object Name) {
    if (Test-Path $entry.Value) {
        Write-Check "$($entry.Name) directory" "ok" $entry.Value
    } else {
        Write-Check "$($entry.Name) directory" "missing" $entry.Value
    }
}

$captureDir = $dirs.captures
try {
    New-Item -ItemType Directory -Force -Path $captureDir | Out-Null
    $probe = Join-Path $captureDir ".winctl-write-probe"
    Set-Content -Path $probe -Value "probe" -Encoding ASCII
    Remove-Item -Path $probe -Force
    Write-Check "capture directory writable" "ok" $captureDir
} catch {
    Write-Check "capture directory writable" "fail" $_.Exception.Message
}

try {
    $health = Invoke-RestMethod -Uri $HealthUrl -Method Get -TimeoutSec 3
    Write-Check "HTTP health" "ok" ($health | ConvertTo-Json -Compress)
} catch {
    Write-Check "HTTP health" "fail" $_.Exception.Message
}

$serverProcesses = Get-Process -Name "winctl-mcp-server" -ErrorAction SilentlyContinue
if ($serverProcesses) {
    foreach ($process in $serverProcesses) {
        Write-Check "server process" "ok" "pid=$($process.Id) path=$($process.Path)"
    }
} else {
    Write-Check "server process" "not-running"
}

if ($RunSelfTest -and (Test-Path $ServerExe)) {
    Write-Host "--- self-test windows-list ---"
    & $ServerExe self-test windows-list
    Write-Host "--- end self-test ---"
}

if (-not [string]::IsNullOrWhiteSpace($TargetProcessName)) {
    $targetName = [IO.Path]::GetFileNameWithoutExtension($TargetProcessName)
    $targets = Get-Process -Name $targetName -ErrorAction SilentlyContinue
    if ($targets) {
        foreach ($process in $targets) {
            Write-Check "target process" "found" "name=$TargetProcessName pid=$($process.Id) path=$($process.Path)"
        }
    } else {
        Write-Check "target process" "missing" $TargetProcessName
    }
}

$memoryDb = Join-Path $dirs.memory "memory.sqlite"
if (Test-Path $memoryDb) {
    $item = Get-Item $memoryDb
    Write-Check "memory database" "ok" "path=$memoryDb bytes=$($item.Length)"
    foreach ($suffix in @("-wal", "-shm")) {
        $sidecar = "$memoryDb$suffix"
        if (Test-Path $sidecar) {
            $sidecarItem = Get-Item $sidecar
            Write-Check "memory sidecar $suffix" "ok" "bytes=$($sidecarItem.Length)"
        }
    }
} else {
    Write-Check "memory database" "missing" $memoryDb
}

$modelFiles = Get-ChildItem -Path $dirs.models -Filter "*.onnx" -ErrorAction SilentlyContinue
if ($modelFiles) {
    foreach ($model in $modelFiles) {
        Write-Check "embedding model" "found" $model.FullName
    }
} else {
    Write-Check "embedding model" "missing" "expected a MiniLM-compatible .onnx file under $($dirs.models)"
}

$recentJson = @()
foreach ($path in @($dirs.artifacts, $dirs.replays, $dirs.exports, $dirs.manifests)) {
    if (Test-Path $path) {
        $recentJson += Get-ChildItem -Path $path -Recurse -Filter "*.json" -ErrorAction SilentlyContinue
    }
}
$recentJson = $recentJson | Sort-Object LastWriteTime -Descending | Select-Object -First 8
if ($recentJson) {
    Write-Host "--- recent replay/artifact manifests ---"
    foreach ($file in $recentJson) {
        Write-Host "$($file.LastWriteTime.ToString('s')) $($file.FullName)"
    }
} else {
    Write-Check "recent replay/artifact manifests" "none"
}
