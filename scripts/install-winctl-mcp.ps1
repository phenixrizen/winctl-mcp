param(
    [string]$SourceDir = "",
    [string]$InstallDir = "$env:LOCALAPPDATA\winctl-mcp",
    [string]$Listen = "127.0.0.1:8765",
    [string]$AuthToken = "",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($SourceDir)) {
    $SourceDir = Split-Path -Parent $PSScriptRoot
}

if ([string]::IsNullOrWhiteSpace($AuthToken)) {
    $AuthToken = [Guid]::NewGuid().ToString("N")
}

$binDir = Join-Path $InstallDir "bin"
$assetsDir = Join-Path $InstallDir "assets"
$docsDir = Join-Path $InstallDir "docs"
$examplesDir = Join-Path $InstallDir "examples"
$scriptsDir = Join-Path $InstallDir "scripts"
$logsDir = Join-Path $InstallDir "logs"
$capturesDir = Join-Path $InstallDir "captures"
$artifactsDir = Join-Path $InstallDir "artifacts"
$memoryDir = Join-Path $InstallDir "memory"
$modelsDir = Join-Path $InstallDir "models"
$manifestsDir = Join-Path $InstallDir "manifests"
$replaysDir = Join-Path $InstallDir "replays"
$exportsDir = Join-Path $InstallDir "exports"

@(
    $InstallDir,
    $binDir,
    $assetsDir,
    $docsDir,
    $examplesDir,
    $scriptsDir,
    $logsDir,
    $capturesDir,
    $artifactsDir,
    $memoryDir,
    $modelsDir,
    $manifestsDir,
    $replaysDir,
    $exportsDir
) | ForEach-Object {
    New-Item -ItemType Directory -Force -Path $_ | Out-Null
}

$sourceBin = Join-Path $SourceDir "bin"
if (-not (Test-Path (Join-Path $sourceBin "winctl-mcp-server.exe"))) {
    throw "Package source is missing bin\winctl-mcp-server.exe: $SourceDir"
}

Copy-Item -Path (Join-Path $sourceBin "*.exe") -Destination $binDir -Force
Copy-Item -Path (Join-Path $sourceBin "*.dll") -Destination $binDir -Force -ErrorAction SilentlyContinue

foreach ($folder in @("assets", "docs", "examples", "scripts")) {
    $from = Join-Path $SourceDir $folder
    $to = Join-Path $InstallDir $folder
    if (Test-Path $from) {
        Copy-Item -Path (Join-Path $from "*") -Destination $to -Recurse -Force
    }
}

$configPath = Join-Path $InstallDir "config.toml"
if ($Force -or -not (Test-Path $configPath)) {
    $config = @"
[transport]
mode = "http"
listen = "$Listen"

[auth]
token = "$AuthToken"

[logging]
log_file = "$($logsDir.Replace('\', '/'))/server.log"

[paths]
capture_dir = "$($capturesDir.Replace('\', '/'))"
artifact_dir = "$($artifactsDir.Replace('\', '/'))"
filesystem_roots = [
  "$($capturesDir.Replace('\', '/'))",
  "$($artifactsDir.Replace('\', '/'))",
  "$($exportsDir.Replace('\', '/'))"
]
memory_db = "$(($memoryDir + '\memory.sqlite').Replace('\', '/'))"

[policy]
enable_filesystem_mutation = false
enable_clipboard_write = false
enable_registry_mutation = false
allow_private_network = false
memory_mutation_enabled = true
macro_execution_enabled = true
macro_destructive_tools_allowed = false
max_macro_runtime_ms = 300000
max_macro_steps = 200
screenshot_retention_count = 200
tool_denylist = []

[embedding]
model_path = "$(($modelsDir + '\minilm.onnx').Replace('\', '/'))"
dimension = 384

[macro_execution]
enabled = true
allow_destructive_tools = false
max_runtime_ms = 300000
max_steps = 200
"@
    Set-Content -Path $configPath -Value $config -Encoding UTF8
}

$serverExe = Join-Path $binDir "winctl-mcp-server.exe"
$trayExe = Join-Path $binDir "winctl-tray.exe"

Write-Host "Installed winctl-mcp to $InstallDir"
Write-Host "Config: $configPath"
Write-Host "HTTP server:"
Write-Host "  `"$serverExe`" serve --config `"$configPath`""
Write-Host "Health check:"
Write-Host "  Invoke-RestMethod http://$Listen/healthz"
if (Test-Path $trayExe) {
    Write-Host "Tray/controller:"
    Write-Host "  `"$trayExe`" status"
}
