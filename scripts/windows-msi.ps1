param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("install", "uninstall")]
    [string]$Action,

    # Required for -Action install. Path to the locally built MSI (may be a
    # \\wsl.localhost\ UNC path when invoked from WSL via `make install-msi`).
    [string]$MsiPath = "",

    # Must match the UpgradeCode authored in scripts/build-windows-msi.ps1.
    [string]$UpgradeCode = "{8E08F85A-B6A8-47A7-90DB-F5B7B2D8BA09}"
)

$ErrorActionPreference = "Stop"

# The packaged MSI is authored Scope="perMachine" (installs under Program Files,
# writes HKLM), so msiexec must run elevated. Each call triggers a UAC prompt.
function Invoke-MsiExec {
    param([Parameter(Mandatory = $true)][string]$ArgString)

    $proc = Start-Process -FilePath "msiexec.exe" -ArgumentList $ArgString -Verb RunAs -Wait -PassThru
    if ($proc.ExitCode -eq 3010) {
        Write-Host "msiexec completed; a reboot is required to finish (exit 3010)."
        return
    }
    if ($proc.ExitCode -ne 0) {
        throw "msiexec '$ArgString' failed with exit code $($proc.ExitCode)."
    }
}

# ProductCodes currently installed under our UpgradeCode (empty if not installed).
function Get-InstalledProductCodes {
    $installer = New-Object -ComObject WindowsInstaller.Installer
    try {
        return @($installer.RelatedProducts($UpgradeCode))
    } catch {
        return @()
    }
}

if ($Action -eq "install") {
    if ([string]::IsNullOrWhiteSpace($MsiPath)) {
        throw "install requires -MsiPath (run ``make msi`` first to build it)."
    }
    if (-not (Test-Path -LiteralPath $MsiPath)) {
        throw "MSI not found: $MsiPath (run ``make msi`` first)."
    }
    $resolved = (Resolve-Path -LiteralPath $MsiPath).ProviderPath

    # Elevated msiexec cannot reach a \\wsl.localhost\ share (the admin token has
    # no WSL drive mapping), so stage the MSI to a local Windows temp file first.
    $local = $resolved
    if ($resolved.StartsWith("\\")) {
        $local = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetFileName($resolved))
        Copy-Item -LiteralPath $resolved -Destination $local -Force
    }

    # The local MSI version is pinned (0.1.0), so installing it over an existing
    # install is a MajorUpgrade no-op and would NOT refresh the files. Remove any
    # prior install first so the freshly built binaries/dashboard actually land.
    foreach ($productCode in Get-InstalledProductCodes) {
        Write-Host "Removing existing $productCode before reinstall (UAC)..."
        Invoke-MsiExec -ArgString "/x $productCode /passive"
    }

    Write-Host "Installing $local (per-machine; expect a UAC prompt)..."
    Invoke-MsiExec -ArgString "/i `"$local`" /passive"
    Write-Host "Installed winctl-mcp."
}
else {
    # Uninstall every product registered under our UpgradeCode, so it works even
    # if the local MSI was rebuilt to a different version/ProductCode (or is gone).
    $related = Get-InstalledProductCodes
    if ($related.Count -eq 0) {
        Write-Host "winctl-mcp is not installed (no product for UpgradeCode $UpgradeCode)."
        return
    }
    foreach ($productCode in $related) {
        Write-Host "Uninstalling $productCode (expect a UAC prompt)..."
        Invoke-MsiExec -ArgString "/x $productCode /passive"
    }
    Write-Host "Uninstalled winctl-mcp."
}
