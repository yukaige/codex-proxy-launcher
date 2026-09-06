param([string]$Gh = 'gh')
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
if ($Gh -eq 'gh' -and -not (Get-Command gh -ErrorAction SilentlyContinue)) {
    $portableGh = Join-Path (Get-Location) 'src-tauri/target/build-tools/gh/bin/gh.exe'
    if (Test-Path -LiteralPath $portableGh) { $Gh = $portableGh }
}
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
$tag = "v$version"
$repository = 'yukaige/codex-proxy-launcher'
$releaseDir = Join-Path (Get-Location) "release/$tag"
$assets = @(
    (Join-Path $releaseDir "Codex-Proxy-Launcher-$tag-windows-x64.zip"),
    (Join-Path $releaseDir "Codex-Proxy-Launcher-$tag-macos-arm64.dmg")
)
foreach ($asset in $assets) {
    if (-not (Test-Path -LiteralPath $asset -PathType Leaf)) { throw "Missing local build: $asset" }
}
$head = & git rev-parse HEAD
$tagCommit = & git rev-parse "$tag^{commit}"
if ($LASTEXITCODE -ne 0 -or $head -ne $tagCommit) { throw 'Release tag must identify the current source commit' }
if (& git status --porcelain) { throw 'Commit source changes before publishing' }
& $Gh auth status --hostname github.com
if ($LASTEXITCODE -ne 0) { throw 'Sign in with gh auth login before publishing' }
$checksums = Join-Path $releaseDir 'SHA256SUMS.txt'
$lines = foreach ($asset in $assets) {
    $hash = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($asset))"
}
[IO.File]::WriteAllLines($checksums, $lines, [Text.UTF8Encoding]::new($false))
$assets += $checksums
& $Gh release create $tag @assets --repo $repository --verify-tag --draft --title "Codex Proxy Launcher $tag" --notes-file RELEASE_NOTES.md
if ($LASTEXITCODE -ne 0) { throw 'Draft release upload failed' }
$json = & $Gh release view $tag --repo $repository --json assets
if ($LASTEXITCODE -ne 0) { throw 'Cannot verify uploaded assets' }
$uploaded = ($json | ConvertFrom-Json).assets
foreach ($asset in $assets) {
    $file = Get-Item -LiteralPath $asset
    if (-not ($uploaded | Where-Object { $_.name -eq $file.Name -and $_.size -eq $file.Length })) {
        throw "Uploaded asset verification failed: $($file.Name)"
    }
}
& $Gh release edit $tag --repo $repository --draft=false --latest
if ($LASTEXITCODE -ne 0) { throw 'Release publication failed; the draft remains available' }
& $Gh release view $tag --repo $repository --json url --jq .url
