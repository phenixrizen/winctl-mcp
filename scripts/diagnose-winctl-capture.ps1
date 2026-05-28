param(
    [string]$Exe = "$env:LOCALAPPDATA\winctl-mcp\winctl-mcp-server.exe",
    [string]$BoundId,
    [string]$ProcessName = "Betty.exe",
    [int]$ScreenshotCount = 1,
    [int]$WaitSeconds = 8
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($BoundId)) {
    throw "Pass -BoundId hwnd:0x..."
}

$process = New-Object System.Diagnostics.Process
$process.StartInfo.FileName = $Exe
$process.StartInfo.UseShellExecute = $false
$process.StartInfo.RedirectStandardInput = $true
$process.StartInfo.RedirectStandardOutput = $true
$process.StartInfo.RedirectStandardError = $false
$process.StartInfo.CreateNoWindow = $true

[void]$process.Start()

$requests = @(
    @{
        jsonrpc = "2.0"
        id = 1
        method = "initialize"
        params = @{
            protocolVersion = "2025-03-26"
            capabilities = @{}
            clientInfo = @{
                name = "winctl-diag"
                version = "0.1"
            }
        }
    },
    @{
        jsonrpc = "2.0"
        method = "notifications/initialized"
        params = @{}
    },
    @{
        jsonrpc = "2.0"
        id = 2
        method = "tools/call"
        params = @{
            name = "windows.bind"
            arguments = @{
                id = $BoundId
                must_be_visible = $true
            }
        }
    }
)

for ($i = 0; $i -lt $ScreenshotCount; $i++) {
    $requests += @{
        jsonrpc = "2.0"
        id = 3 + $i
        method = "tools/call"
        params = @{
            name = "capture.screenshot_window"
            arguments = @{
                bound_id = $BoundId
            }
        }
    }
}

$requests += @{
    jsonrpc = "2.0"
    id = 3 + $ScreenshotCount
    method = "tools/call"
    params = @{
        name = "windows.find"
        arguments = @{
            process_name = $ProcessName
        }
    }
}

foreach ($request in $requests) {
    $line = $request | ConvertTo-Json -Compress -Depth 20
    Write-Host ">>> $line"
    $process.StandardInput.WriteLine($line)
    $process.StandardInput.Flush()
    Start-Sleep -Milliseconds 300

    if ($process.HasExited) {
        break
    }
}

Start-Sleep -Seconds $WaitSeconds

if (-not $process.HasExited) {
    $process.StandardInput.Close()
    Start-Sleep -Seconds 1
}

if (-not $process.HasExited) {
    $process.Kill()
}

$stdout = $process.StandardOutput.ReadToEnd()
$process.WaitForExit()

$stderr = "<stderr inherited by console>"

Write-Host "EXIT=$($process.ExitCode)"
Write-Host "---STDOUT---"
Write-Host $stdout
Write-Host "---STDERR---"
Write-Host $stderr
