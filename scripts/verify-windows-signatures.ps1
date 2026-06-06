param(
    [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
    [string[]]$Path
)

$ErrorActionPreference = "Stop"

if ($Path.Count -eq 0) {
    throw "At least one file path or wildcard is required."
}

$files = @()
foreach ($item in $Path) {
    $resolved = @(Resolve-Path -Path $item -ErrorAction Stop)
    foreach ($match in $resolved) {
        $file = Get-Item -LiteralPath $match.Path -ErrorAction Stop
        if ($file.PSIsContainer) {
            $files += @(Get-ChildItem -LiteralPath $file.FullName -File -Recurse)
        } else {
            $files += $file
        }
    }
}

$files = @($files | Sort-Object -Property FullName -Unique)
if ($files.Count -eq 0) {
    throw "No files matched the provided paths."
}

$failures = @()
foreach ($file in $files) {
    $signature = Get-AuthenticodeSignature -FilePath $file.FullName
    if ($signature.Status -ne "Valid") {
        $failures += "{0}: {1} ({2})" -f $file.FullName, $signature.Status, $signature.StatusMessage
        continue
    }

    $subject = if ($signature.SignerCertificate) {
        $signature.SignerCertificate.Subject
    } else {
        "unknown signer"
    }
    Write-Host ("Verified Authenticode signature: {0} [{1}]" -f $file.FullName, $subject)
}

if ($failures.Count -gt 0) {
    throw ("Authenticode signature verification failed:`n" + ($failures -join "`n"))
}
