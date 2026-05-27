param(
    [string]$Target = "",
    [string]$Cargo = "cargo",
    [switch]$Release
)

$ErrorActionPreference = "Stop"

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw "This harness must be run on a Windows desktop session."
}

$env:WINCTL_RUN_WINDOWS_INTEGRATION = "1"

$cargoArgs = @("test", "-p", "winctl-test-target", "--test", "windows_integration")
if ($Target -ne "") {
    $cargoArgs += @("--target", $Target)
}
if ($Release) {
    $cargoArgs += "--release"
}
$cargoArgs += @("--", "--nocapture")

Write-Host "Running Windows integration harness:"
Write-Host "  $Cargo $($cargoArgs -join ' ')"

& $Cargo @cargoArgs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
