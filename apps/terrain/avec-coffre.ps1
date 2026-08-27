# Ouvre le coffre et lance une commande avec le clair dans SON environnement.
#
# Le secret ne touche jamais un fichier, et n'apparait jamais dans `argv` : il
# vit dans l'environnement du processus enfant, lisible par le seul compte
# courant — la meme frontiere que le coffre DPAPI lui-meme.
param(
  [Parameter(Mandatory = $true)][string]$script,
  [string]$coffre = "$env:USERPROFILE\.noe\coffre\salesforce-de.dpapi",
  [Parameter(ValueFromRemainingArguments = $true)][string[]]$reste
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Security
$brut = [System.IO.File]::ReadAllBytes($coffre)
$clair = [System.Security.Cryptography.ProtectedData]::Unprotect(
  $brut, $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser)
$env:NOE_COFFRE = [System.Text.Encoding]::UTF8.GetString($clair)
try {
  & node $script @reste
} finally {
  # On ne laisse pas le clair dans l'environnement du shell appelant.
  Remove-Item Env:\NOE_COFFRE -ErrorAction SilentlyContinue
}
