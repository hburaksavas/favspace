[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$publicCertificatePath = Join-Path $projectRoot 'artifacts\Favspace-Personal-Code-Signing.cer'
$portableExecutable = Join-Path $projectRoot 'artifacts\Favspace-Portable.exe'

if (-not (Test-Path -LiteralPath $publicCertificatePath)) {
    throw 'The Favspace public signing certificate is missing. Run scripts\New-PersonalSigningCertificate.ps1 first.'
}

Write-Warning 'Only continue if this certificate was created by you for Favspace. Trusting an unknown certificate is unsafe.'

Import-Certificate `
    -FilePath $publicCertificatePath `
    -CertStoreLocation Cert:\CurrentUser\Root | Out-Null
Import-Certificate `
    -FilePath $publicCertificatePath `
    -CertStoreLocation Cert:\CurrentUser\TrustedPublisher | Out-Null

$certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($publicCertificatePath)
$result = [ordered]@{
    Subject = $certificate.Subject
    Thumbprint = $certificate.Thumbprint
    TrustedFor = 'Current Windows user'
}

if (Test-Path -LiteralPath $portableExecutable) {
    $signature = Get-AuthenticodeSignature -FilePath $portableExecutable
    $result.SignatureStatus = $signature.Status
    $result.SignatureMessage = $signature.StatusMessage
}

[PSCustomObject]$result
