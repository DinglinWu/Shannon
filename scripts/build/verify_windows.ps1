param(
    [Parameter(Mandatory = $true)][string]$Artifact,
    [string]$ExpectedThumbprint = ''
)
$ErrorActionPreference = 'Stop'
$signature = Get-AuthenticodeSignature -LiteralPath $Artifact
if ($signature.Status -ne 'Valid') {
    throw "Windows signature invalid: $($signature.Status)"
}
if ($ExpectedThumbprint -and $signature.SignerCertificate.Thumbprint -ne $ExpectedThumbprint) {
    throw 'Windows signing certificate does not match the configured publisher.'
}
