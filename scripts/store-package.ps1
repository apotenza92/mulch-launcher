# Builds the Microsoft Store package (MSIX) to upload in Partner Center.
#
#   scripts\store-package.ps1
#
# Needs packaging\store\identity.json with the app's identity from Partner
# Center (Product management > Product identity): copy identity.example.json
# and fill in Package/Identity/Name, Package/Identity/Publisher and
# Package/Properties/PublisherDisplayName. The Store signs the package, so it
# isn't signed here. Output: target\store\MulchLauncher-<version>.msix
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot)

$identityFile = "packaging\store\identity.json"
if (-not (Test-Path $identityFile)) {
    throw "Missing $identityFile. Copy identity.example.json and fill in the app's identity from Partner Center."
}
$identity = Get-Content $identityFile -Raw | ConvertFrom-Json
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value

$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64" -Directory |
    Where-Object { Test-Path "$($_.FullName)\makeappx.exe" } | Sort-Object Name -Descending | Select-Object -First 1
if (-not $sdk) { throw "The Windows SDK isn't installed (winget install Microsoft.WindowsSDK.10.0.26100)." }

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "build failed" }

# A fresh staging folder for this version.
$stage = "target\store\$version"
if (Test-Path $stage) { throw "$stage already exists: delete it or bump the version." }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item target\release\MulchLauncher.exe $stage
Copy-Item packaging\store\Assets $stage -Recurse
$manifest = (Get-Content packaging\store\AppxManifest.xml -Raw).
    Replace("{{NAME}}", $identity.name).
    Replace("{{PUBLISHER}}", $identity.publisher).
    Replace("{{PUBLISHER_DISPLAY_NAME}}", $identity.publisherDisplayName).
    Replace("{{VERSION}}", "$version.0")
[IO.File]::WriteAllText((Join-Path (Resolve-Path $stage) "AppxManifest.xml"), $manifest)

# Index the scaled images so Windows picks the right size for each use.
Push-Location $stage
& "$($sdk.FullName)\makepri.exe" createconfig /cf ..\priconfig.xml /dq en-US /o | Out-Null
& "$($sdk.FullName)\makepri.exe" new /pr . /cf ..\priconfig.xml /of resources.pri /o | Out-Null
Pop-Location

$msix = "target\store\MulchLauncher-$version.msix"
& "$($sdk.FullName)\makeappx.exe" pack /d $stage /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "packaging failed" }
Write-Host "Built $msix"
