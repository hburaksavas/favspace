[CmdletBinding()]
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$certificateSubject = 'CN=Favspace Personal Code Signing'
$projectRoot = Split-Path -Parent $PSScriptRoot
$artifactsDirectory = Join-Path $projectRoot 'artifacts'
$sourceExecutable = Join-Path $projectRoot 'src-tauri\target\release\favspace.exe'
$portableExecutable = Join-Path $artifactsDirectory 'Favspace-Portable.exe'
$protectedKeyPath = Join-Path $artifactsDirectory 'Favspace-Personal-Code-Signing.pfx.dpapi'
$cargoDirectory = Join-Path $env:USERPROFILE '.cargo\bin'
$entropy = [System.Text.Encoding]::UTF8.GetBytes('Favspace personal code signing key v1')

if (-not (Test-Path -LiteralPath $protectedKeyPath)) {
    throw 'Favspace personal signing certificate is missing. Run scripts\New-PersonalSigningCertificate.ps1 first.'
}
$protectedKey = [System.IO.File]::ReadAllBytes($protectedKeyPath)
$pfxBytes = [System.Security.Cryptography.ProtectedData]::Unprotect(
    $protectedKey,
    $entropy,
    [System.Security.Cryptography.DataProtectionScope]::CurrentUser
)
$certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
    $pfxBytes,
    '',
    [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet
)
if ($certificate.Subject -ne $certificateSubject -or -not $certificate.HasPrivateKey -or $certificate.NotAfter -le (Get-Date)) {
    throw 'Favspace personal signing certificate is invalid or expired.'
}

$env:Path = "$cargoDirectory;$env:Path"
Push-Location $projectRoot
try {
    if (-not $SkipBuild) {
        npm run tauri -- build --no-bundle
        if ($LASTEXITCODE -ne 0) {
            throw "Tauri release build failed with exit code $LASTEXITCODE."
        }
    }

    if (-not (Test-Path -LiteralPath $sourceExecutable)) {
        throw "Release executable is missing: $sourceExecutable"
    }
    New-Item -ItemType Directory -Path $artifactsDirectory -Force | Out-Null
    Copy-Item -LiteralPath $sourceExecutable -Destination $portableExecutable -Force

    $signature = Set-AuthenticodeSignature `
        -FilePath $portableExecutable `
        -Certificate $certificate `
        -HashAlgorithm SHA256

    if (-not $signature.SignerCertificate -or $signature.SignerCertificate.Thumbprint -ne $certificate.Thumbprint) {
        throw 'The portable executable was not signed with the expected Favspace certificate.'
    }

    $hash = Get-FileHash -LiteralPath $portableExecutable -Algorithm SHA256
    $file = Get-Item -LiteralPath $portableExecutable
    [PSCustomObject]@{
        File = $file.FullName
        Size = $file.Length
        SHA256 = $hash.Hash
        SignatureStatus = $signature.Status
        SignatureMessage = $signature.StatusMessage
        Signer = $signature.SignerCertificate.Subject
        CertificateExpires = $signature.SignerCertificate.NotAfter
    }
}
finally {
    Pop-Location
}
