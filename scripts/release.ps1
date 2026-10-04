# Publishes a release that installed copies of MulchLauncher update to.
#
#   scripts\release.ps1            # releases the version in Cargo.toml
#
# Bump `version` in Cargo.toml first (installed copies only update to a
# higher version). Builds the release exe, writes its SHA-256 checksum, and
# creates the GitHub release vX.Y.Z with both files.
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot)

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$tag = "v$version"
if (gh release view $tag 2>$null) { throw "$tag is already released: bump the version in Cargo.toml first." }

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "build failed" }

$out = Join-Path $env:TEMP "mulch-release-$version"
New-Item -ItemType Directory -Force $out | Out-Null
$exe = Join-Path $out "MulchLauncher.exe"
Copy-Item target\release\MulchLauncher.exe $exe -Force
$hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
[IO.File]::WriteAllText("$exe.sha256", "$hash  MulchLauncher.exe`n")

gh release create $tag $exe "$exe.sha256" --title "MulchLauncher $version" --generate-notes
Write-Host "Released $tag ($hash)"
