[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$certificateSubject = 'CN=Favspace Personal Code Signing'
$projectRoot = Split-Path -Parent $PSScriptRoot
$artifactsDirectory = Join-Path $projectRoot 'artifacts'
$protectedKeyPath = Join-Path $artifactsDirectory 'Favspace-Personal-Code-Signing.pfx.dpapi'
$publicCertificatePath = Join-Path $artifactsDirectory 'Favspace-Personal-Code-Signing.cer'
$entropy = [System.Text.Encoding]::UTF8.GetBytes('Favspace personal code signing key v1')

New-Item -ItemType Directory -Path $artifactsDirectory -Force | Out-Null

if (Test-Path -LiteralPath $protectedKeyPath) {
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

    if ($certificate.Subject -ne $certificateSubject -or -not $certificate.HasPrivateKey) {
        throw 'The protected Favspace signing key is invalid.'
    }
    if ($certificate.NotAfter -le (Get-Date).AddDays(30)) {
        throw 'The Favspace signing certificate expires in less than 30 days. Remove the protected key and create a new certificate.'
    }
}
else {
    $rsa = [System.Security.Cryptography.RSA]::Create(3072)
    try {
        $distinguishedName = [System.Security.Cryptography.X509Certificates.X500DistinguishedName]::new($certificateSubject)
        $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
            $distinguishedName,
            $rsa,
            [System.Security.Cryptography.HashAlgorithmName]::SHA256,
            [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
        )
        $request.CertificateExtensions.Add(
            [System.Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new(
                [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature,
                $true
            )
        )
        $enhancedKeyUsages = [System.Security.Cryptography.OidCollection]::new()
        [void]$enhancedKeyUsages.Add([System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.3', 'Code Signing'))
        $request.CertificateExtensions.Add(
            [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new(
                $enhancedKeyUsages,
                $false
            )
        )

        $certificate = $request.CreateSelfSigned((Get-Date).AddMinutes(-5), (Get-Date).AddYears(5))
        $pfxBytes = $certificate.Export(
            [System.Security.Cryptography.X509Certificates.X509ContentType]::Pfx,
            ''
        )
        $protectedKey = [System.Security.Cryptography.ProtectedData]::Protect(
            $pfxBytes,
            $entropy,
            [System.Security.Cryptography.DataProtectionScope]::CurrentUser
        )
        [System.IO.File]::WriteAllBytes($protectedKeyPath, $protectedKey)
    }
    finally {
        $rsa.Dispose()
    }
}

$publicBytes = $certificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert)
[System.IO.File]::WriteAllBytes($publicCertificatePath, $publicBytes)

[PSCustomObject]@{
    Subject = $certificate.Subject
    Thumbprint = $certificate.Thumbprint
    Expires = $certificate.NotAfter
    PrivateKeyProtection = 'Windows DPAPI / CurrentUser'
    ProtectedKey = $protectedKeyPath
    PublicCertificate = $publicCertificatePath
}
