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
# writes HKLM), so msiexec must run elevated and triggers a UAC prompt. A declined
# prompt surfaces a clean message instead of a raw Start-Process stack trace.
function Invoke-MsiExec {
    param([Parameter(Mandatory = $true)][string]$ArgString)

    try {
        $proc = Start-Process -FilePath "msiexec.exe" -ArgumentList $ArgString -Verb RunAs -Wait -PassThru
    } catch {
        if ($_.Exception.Message -match "canceled by the user") {
            Write-Host "Elevation was declined at the UAC prompt; nothing was changed."
            Write-Host "Re-run and approve the prompt to continue."
            exit 1
        }
        throw
    }
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

    if ((Get-InstalledProductCodes).Count -gt 0) {
        # The local MSI version is pinned (0.1.0), so a plain reinstall is a
        # MajorUpgrade no-op. REINSTALL=ALL REINSTALLMODE=vamus forces every
        # component/file to be rewritten from this package, so the freshly built
        # binaries/dashboard land -- all in a single elevation (one UAC prompt).
        Write-Host "Reinstalling winctl-mcp over the existing install (per-machine; expect a UAC prompt)..."
        Invoke-MsiExec -ArgString "/i `"$local`" REINSTALL=ALL REINSTALLMODE=vamus /passive"
    } else {
        Write-Host "Installing $local (per-machine; expect a UAC prompt)..."
        Invoke-MsiExec -ArgString "/i `"$local`" /passive"
    }
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
