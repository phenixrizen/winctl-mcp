param(
    [Parameter(Mandatory = $true)]
    [string]$DistDir,

    [Parameter(Mandatory = $true)]
    [string]$Version,

    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"

function New-WixId {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Prefix,
        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $sha = $sha256.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Value))
    } finally {
        $sha256.Dispose()
    }
    $hex = -join ($sha[0..7] | ForEach-Object { $_.ToString("x2") })
    return "$Prefix$hex"
}

function New-StableGuid {
    param([Parameter(Mandatory = $true)][string]$Value)

    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha256.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Value))
    } finally {
        $sha256.Dispose()
    }
    $bytes = $hash[0..15]
    return ([Guid]::new([byte[]]$bytes)).ToString("B").ToUpperInvariant()
}

function ConvertTo-WixSource {
    param([Parameter(Mandatory = $true)][string]$Path)
    return $Path.Replace("&", "&amp;").Replace("<", "&lt;").Replace(">", "&gt;").Replace('"', "&quot;")
}

function Resolve-NativePath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $resolved = Resolve-Path $Path
    if (-not [string]::IsNullOrWhiteSpace($resolved.ProviderPath)) {
        return $resolved.ProviderPath
    }
    return $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($resolved.Path)
}

function ConvertTo-NativeUnresolvedPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $unresolved = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
    $providerPrefix = "Microsoft.PowerShell.Core\FileSystem::"
    if ($unresolved.StartsWith($providerPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $unresolved.Substring($providerPrefix.Length)
    }
    return $unresolved
}

function Get-RelativePathCompat {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BasePath,
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $baseFull = (Resolve-NativePath $BasePath).Replace('\', '/').TrimEnd('/') + '/'
    $pathFull = (Resolve-NativePath $Path).Replace('\', '/')
    if (-not $pathFull.StartsWith($baseFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Path is not under base path: $Path"
    }
    return $pathFull.Substring($baseFull.Length).Replace("/", "\")
}

function Add-DirectoryComponentGroup {
    param(
        [Parameter(Mandatory = $true)]
        [System.Text.StringBuilder]$Builder,
        [Parameter(Mandatory = $true)]
        [string]$GroupId,
        [Parameter(Mandatory = $true)]
        [string]$DirectoryId,
        [Parameter(Mandatory = $true)]
        [System.IO.FileInfo[]]$Files,
        [Parameter(Mandatory = $true)]
        [string]$DistRoot
    )

    [void]$Builder.AppendLine("    <ComponentGroup Id=`"$GroupId`" Directory=`"$DirectoryId`">")
    foreach ($file in $Files) {
        $relative = (Get-RelativePathCompat -BasePath $DistRoot -Path $file.FullName).Replace("\", "/")
        $componentId = New-WixId -Prefix "Cmp" -Value $relative
        $fileId = New-WixId -Prefix "Fil" -Value $relative
        $guid = New-StableGuid "component:$relative"
        $source = ConvertTo-WixSource $file.FullName
        [void]$Builder.AppendLine("      <Component Id=`"$componentId`" Guid=`"$guid`">")
        [void]$Builder.AppendLine("        <File Id=`"$fileId`" Source=`"$source`" KeyPath=`"yes`" />")
        [void]$Builder.AppendLine("      </Component>")
    }
    [void]$Builder.AppendLine("    </ComponentGroup>")
}

$productVersion = $Version -replace "-.*$", ""
if ($productVersion -notmatch "^\d+\.\d+\.\d+$") {
    throw "MSI product version must be major.minor.patch; got $Version"
}

$wix = Get-Command wix -ErrorAction Stop
$temp = Join-Path ([System.IO.Path]::GetTempPath()) "winctl-mcp-msi-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $temp | Out-Null

try {
    $sourceDist = Resolve-NativePath $DistDir
    $dist = $sourceDist
    if ($sourceDist.StartsWith("\\")) {
        $dist = Join-Path $temp "dist"
        Copy-Item -Path $sourceDist -Destination $dist -Recurse -Force
    }

    if (-not (Test-Path (Join-Path $dist "bin\winctl-mcp-server.exe"))) {
        throw "MSI source is missing bin\winctl-mcp-server.exe: $dist"
    }
    if (-not (Test-Path (Join-Path $dist "bin\winctl-tray.exe"))) {
        throw "MSI source is missing bin\winctl-tray.exe: $dist"
    }
    if (-not (Test-Path (Join-Path $dist "bin\winctl-launcher.exe"))) {
        throw "MSI source is missing bin\winctl-launcher.exe: $dist"
    }
    $iconPath = Join-Path $dist "assets\winctl.ico"
    if (-not (Test-Path $iconPath)) {
        throw "MSI source is missing assets\winctl.ico: $dist"
    }

    if ([string]::IsNullOrWhiteSpace($OutputPath)) {
        $OutputPath = Join-Path (Split-Path -Parent $sourceDist) "winctl-mcp-$Version-windows-x64.msi"
    }
    $requestedOutputPath = ConvertTo-NativeUnresolvedPath $OutputPath
    $buildOutputPath = $requestedOutputPath
    if ($requestedOutputPath.StartsWith("\\")) {
        $buildOutputPath = Join-Path $temp ([System.IO.Path]::GetFileName($requestedOutputPath))
    }

    $wxs = Join-Path $temp "Product.wxs"
    $upgradeCode = "{8E08F85A-B6A8-47A7-90DB-F5B7B2D8BA09}"
    $productName = "winctl MCP"
    $manufacturer = "RockSolid Labs, Inc"
    $iconSource = ConvertTo-WixSource $iconPath
    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.AppendLine('<?xml version="1.0" encoding="UTF-8"?>')
    [void]$builder.AppendLine('<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">')
    [void]$builder.AppendLine("  <Package Name=`"$productName`" Manufacturer=`"$manufacturer`" Version=`"$productVersion`" UpgradeCode=`"$upgradeCode`" Scope=`"perMachine`">")
    [void]$builder.AppendLine('    <MajorUpgrade DowngradeErrorMessage="A newer version of winctl-mcp is already installed." />')
    [void]$builder.AppendLine('    <MediaTemplate EmbedCab="yes" />')
    [void]$builder.AppendLine("    <Icon Id=`"WinctlIcon.ico`" SourceFile=`"$iconSource`" />")
    [void]$builder.AppendLine('    <Property Id="ARPPRODUCTICON" Value="WinctlIcon.ico" />')
    [void]$builder.AppendLine("    <Feature Id=`"Main`" Title=`"$productName`" Level=`"1`">")
    [void]$builder.AppendLine('      <ComponentGroupRef Id="RootFiles" />')
    [void]$builder.AppendLine('      <ComponentGroupRef Id="AssetsFiles" />')
    [void]$builder.AppendLine('      <ComponentGroupRef Id="BrandAssetsFiles" />')
    [void]$builder.AppendLine('      <ComponentGroupRef Id="BinFiles" />')
    [void]$builder.AppendLine('      <ComponentGroupRef Id="DocsFiles" />')
    [void]$builder.AppendLine('      <ComponentGroupRef Id="ExamplesFiles" />')
    [void]$builder.AppendLine('      <ComponentGroupRef Id="ScriptsFiles" />')
    [void]$builder.AppendLine('      <ComponentRef Id="StartMenuShortcut" />')
    [void]$builder.AppendLine('    </Feature>')
    [void]$builder.AppendLine('    <StandardDirectory Id="ProgramFiles64Folder">')
    [void]$builder.AppendLine('      <Directory Id="INSTALLFOLDER" Name="winctl-mcp">')
    [void]$builder.AppendLine('        <Directory Id="ASSETSDIR" Name="assets">')
    [void]$builder.AppendLine('          <Directory Id="BRANDASSETSDIR" Name="brand" />')
    [void]$builder.AppendLine('        </Directory>')
    [void]$builder.AppendLine('        <Directory Id="BINDIR" Name="bin" />')
    [void]$builder.AppendLine('        <Directory Id="DOCSDIR" Name="docs" />')
    [void]$builder.AppendLine('        <Directory Id="EXAMPLESDIR" Name="examples" />')
    [void]$builder.AppendLine('        <Directory Id="SCRIPTSDIR" Name="scripts" />')
    [void]$builder.AppendLine('      </Directory>')
    [void]$builder.AppendLine('    </StandardDirectory>')
    [void]$builder.AppendLine('    <StandardDirectory Id="ProgramMenuFolder">')
    [void]$builder.AppendLine('      <Directory Id="ApplicationProgramsFolder" Name="winctl" />')
    [void]$builder.AppendLine('    </StandardDirectory>')

    $rootFiles = @(Get-ChildItem -Path $dist -File)
    $assetsFiles = @(Get-ChildItem -Path (Join-Path $dist "assets") -File -ErrorAction SilentlyContinue)
    $brandAssetsFiles = @(Get-ChildItem -Path (Join-Path $dist "assets\brand") -File -ErrorAction SilentlyContinue)
    $binFiles = @(Get-ChildItem -Path (Join-Path $dist "bin") -File)
    $docsFiles = @(Get-ChildItem -Path (Join-Path $dist "docs") -File -ErrorAction SilentlyContinue)
    $examplesFiles = @(Get-ChildItem -Path (Join-Path $dist "examples") -File -ErrorAction SilentlyContinue)
    $scriptsFiles = @(Get-ChildItem -Path (Join-Path $dist "scripts") -File -ErrorAction SilentlyContinue)

    Add-DirectoryComponentGroup -Builder $builder -GroupId "RootFiles" -DirectoryId "INSTALLFOLDER" -Files $rootFiles -DistRoot $dist
    Add-DirectoryComponentGroup -Builder $builder -GroupId "AssetsFiles" -DirectoryId "ASSETSDIR" -Files $assetsFiles -DistRoot $dist
    Add-DirectoryComponentGroup -Builder $builder -GroupId "BrandAssetsFiles" -DirectoryId "BRANDASSETSDIR" -Files $brandAssetsFiles -DistRoot $dist
    Add-DirectoryComponentGroup -Builder $builder -GroupId "BinFiles" -DirectoryId "BINDIR" -Files $binFiles -DistRoot $dist
    Add-DirectoryComponentGroup -Builder $builder -GroupId "DocsFiles" -DirectoryId "DOCSDIR" -Files $docsFiles -DistRoot $dist
    Add-DirectoryComponentGroup -Builder $builder -GroupId "ExamplesFiles" -DirectoryId "EXAMPLESDIR" -Files $examplesFiles -DistRoot $dist
    Add-DirectoryComponentGroup -Builder $builder -GroupId "ScriptsFiles" -DirectoryId "SCRIPTSDIR" -Files $scriptsFiles -DistRoot $dist

    [void]$builder.AppendLine('    <Component Id="StartMenuShortcut" Directory="ApplicationProgramsFolder" Guid="{4E7DFA08-5C72-4FB5-8E05-7961F0464AA2}">')
    [void]$builder.AppendLine('      <Shortcut Id="WinctlMcpControlShortcut" Name="winctl Control" Description="Start the winctl local control app and MCP server." Target="[BINDIR]winctl-launcher.exe" Arguments="run" WorkingDirectory="BINDIR" Icon="WinctlIcon.ico" IconIndex="0" />')
    [void]$builder.AppendLine('      <Shortcut Id="WinctlMcpDashboardShortcut" Name="winctl Dashboard" Description="Start the MCP server if needed and open the dashboard." Target="[BINDIR]winctl-launcher.exe" Arguments="open-dashboard" WorkingDirectory="BINDIR" Icon="WinctlIcon.ico" IconIndex="0" />')
    [void]$builder.AppendLine('      <Shortcut Id="WinctlMcpRecorderShortcut" Name="winctl Recorder" Description="Start the MCP server if needed and open the dashboard Recorder tab." Target="[BINDIR]winctl-launcher.exe" Arguments="open-recorder" WorkingDirectory="BINDIR" Icon="WinctlIcon.ico" IconIndex="0" />')
    [void]$builder.AppendLine('      <RemoveFolder Id="ApplicationProgramsFolder" On="uninstall" />')
    [void]$builder.AppendLine('      <RegistryValue Root="HKLM" Key="Software\winctl-mcp" Name="StartMenuShortcut" Type="integer" Value="1" KeyPath="yes" />')
    [void]$builder.AppendLine('    </Component>')
    [void]$builder.AppendLine('  </Package>')
    [void]$builder.AppendLine('</Wix>')

    Set-Content -Path $wxs -Value $builder.ToString() -Encoding UTF8
    & $wix.Source build $wxs -arch x64 -out $buildOutputPath
    if ($LASTEXITCODE -ne 0) {
        throw "wix build failed with exit code $LASTEXITCODE"
    }
    if ($buildOutputPath -ne $requestedOutputPath) {
        Copy-Item -Path $buildOutputPath -Destination $requestedOutputPath -Force
    }

    Write-Host "Built MSI: $requestedOutputPath"
} finally {
    Remove-Item -Path $temp -Recurse -Force -ErrorAction SilentlyContinue
}
