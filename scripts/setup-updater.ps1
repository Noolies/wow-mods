<#
.SYNOPSIS
  One-time setup for Hearthforge's signed automatic updates.

.DESCRIPTION
  1. Generates the update-signing key pair outside the repo, in ~/.tauri/wow-mods.key.
  2. Writes the PUBLIC key into src-tauri/tauri.conf.json (safe to commit).
  3. Stores the PRIVATE key and its password as GitHub Actions secrets with the gh CLI.

  The private key is never written into the repo, printed, or copied to the clipboard.
  Keep ~/.tauri/wow-mods.key backed up somewhere safe: without it you can't ship
  updates to people who already have the app installed.

.EXAMPLE
  ./scripts/setup-updater.ps1
  ./scripts/setup-updater.ps1 -Repo Noolies/wow-mods
#>
param(
  [string]$Repo = "Noolies/wow-mods"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$keyDir = Join-Path $HOME ".tauri"
$keyPath = Join-Path $keyDir "wow-mods.key"
New-Item -ItemType Directory -Force -Path $keyDir | Out-Null

function Read-Password([string]$prompt) {
  $secure = Read-Host -AsSecureString $prompt
  $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
  try { return [Runtime.InteropServices.Marshal]::PtrToStringAuto($bstr) }
  finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
}

if (Test-Path $keyPath) {
  Write-Host "Using the existing signing key at $keyPath"
  $password = Read-Password "Password for the existing signing key"
} else {
  $password = Read-Password "Choose a password for the new signing key"
  Push-Location $root
  try { npx tauri signer generate --ci --write-keys $keyPath --password $password | Out-Null }
  finally { Pop-Location }
  if (-not (Test-Path "$keyPath.pub")) { throw "Key generation failed." }
  Write-Host "Created a signing key at $keyPath"
}

# Put the public key into the app config.
$publicKey = (Get-Content "$keyPath.pub" -Raw).Trim()
$configPath = Join-Path $root "src-tauri\tauri.conf.json"
$config = Get-Content $configPath -Raw
$updated = [regex]::Replace($config, '("pubkey":\s*")[^"]*(")', "`${1}$publicKey`${2}")
[IO.File]::WriteAllText($configPath, $updated, (New-Object Text.UTF8Encoding $false))
Write-Host "Wrote the public key to src-tauri/tauri.conf.json"

# Hand the private key and password to GitHub Actions through stdin.
Get-Content $keyPath -Raw | gh secret set TAURI_SIGNING_PRIVATE_KEY --repo $Repo
$password | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo $Repo
Write-Host "Saved TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD as secrets on $Repo"

Write-Host ""
Write-Host "Next: commit src-tauri/tauri.conf.json, then publish a release:"
Write-Host "  git tag v0.1.0"
Write-Host "  git push origin main --tags"
