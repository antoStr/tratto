# Signs one Windows file (the app, its installer, its uninstaller): cargo-packager calls it as
# its sign-command. A signature from a trusted certificate is what stops SmartScreen and the
# antivirus from treating Tratto as an unknown program. Without a certificate set up it does
# nothing, so builds keep working (unsigned).
#
# Two ways, chosen by what the build has (on GitHub: Settings > Secrets and variables > Actions):
# - a code signing certificate as a .pfx file: WINDOWS_CERTIFICATE (the file, in base64) and
#   WINDOWS_CERTIFICATE_PASSWORD;
# - Azure Trusted Signing: AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET (an app with the
#   "Trusted Signing Certificate Profile Signer" role), TRUSTED_SIGNING_ENDPOINT (for example
#   https://weu.codesigning.azure.net), TRUSTED_SIGNING_ACCOUNT, TRUSTED_SIGNING_PROFILE.
#
# Runs in Windows PowerShell 5.1, which every Windows has.
param([Parameter(Mandatory = $true)][string]$File)
$ErrorActionPreference = 'Stop'

function Find-SignTool {
    $found = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($found) { return $found.Source }
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $tool = Get-ChildItem $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like '*\x64\*' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if (-not $tool) { throw 'signtool.exe not found: install the Windows SDK.' }
    return $tool.FullName
}

if ($env:WINDOWS_CERTIFICATE) {
    $dir = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
    $pfx = Join-Path $dir ('tratto-sign-' + [guid]::NewGuid().ToString('N') + '.pfx')
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
    try {
        $tool = Find-SignTool
        # A timestamp keeps the signature valid after the certificate expires.
        & $tool sign /fd sha256 /f $pfx /p $env:WINDOWS_CERTIFICATE_PASSWORD /tr 'http://timestamp.digicert.com' /td sha256 /d 'Tratto' $File
        if ($LASTEXITCODE -ne 0) { throw "signtool failed with code $LASTEXITCODE on $File" }
    } finally {
        Remove-Item $pfx -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Signed $File"
    exit 0
}

if ($env:TRUSTED_SIGNING_ENDPOINT) {
    if (-not (Get-Module -ListAvailable -Name TrustedSigning)) {
        Install-PackageProvider -Name NuGet -MinimumVersion 2.8.5.201 -Force -Scope CurrentUser | Out-Null
        Install-Module -Name TrustedSigning -Force -Scope CurrentUser -AllowClobber
    }
    Invoke-TrustedSigning -Endpoint $env:TRUSTED_SIGNING_ENDPOINT -CodeSigningAccountName $env:TRUSTED_SIGNING_ACCOUNT -CertificateProfileName $env:TRUSTED_SIGNING_PROFILE -Files $File -FileDigest SHA256 -TimestampRfc3161 'http://timestamp.acs.microsoft.com' -TimestampDigest SHA256
    Write-Host "Signed $File"
    exit 0
}

Write-Host "No code signing certificate set up: $File stays unsigned."
exit 0
