param([string]$MingwBin)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $projectRoot

foreach ($name in @('CARGO_HOME', 'RUSTUP_HOME')) {
    if (-not [Environment]::GetEnvironmentVariable($name, 'Process')) {
        $value = [Environment]::GetEnvironmentVariable($name, 'User')
        if ($value) { [Environment]::SetEnvironmentVariable($name, $value, 'Process') }
    }
}
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$env:PATH = (Join-Path $cargoHome 'bin') + ';' + $env:PATH
$rustInfo = & rustc -vV
if ($LASTEXITCODE -ne 0) { throw 'Rust is not available' }
$gnu = [bool]($rustInfo -match 'host: .*windows-gnu')
if ($gnu) {
    if (-not $MingwBin) {
        $MingwBin = @('D:\msys64\mingw64\bin', 'C:\msys64\mingw64\bin') |
            Where-Object { Test-Path -LiteralPath (Join-Path $_ 'gcc.exe') } | Select-Object -First 1
    }
    if (-not $MingwBin) { throw 'Provide -MingwBin with the MSYS2 MinGW64 bin directory' }
    $env:PATH = $MingwBin + ';' + $env:PATH
}

& npm.cmd ci
if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
& npm.cmd run typecheck
if ($LASTEXITCODE -ne 0) { throw 'Type checking failed' }
& cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed' }

& npm.cmd test
if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }

& npm.cmd run dist:windows
if ($LASTEXITCODE -ne 0) { throw 'Windows release build failed' }
$packageJson = [IO.File]::ReadAllText((Join-Path $projectRoot 'package.json'), [Text.Encoding]::UTF8) | ConvertFrom-Json
$version = $packageJson.version
$releaseDir = Join-Path $projectRoot "release/v$version"
$packageDir = Join-Path $releaseDir 'windows-x64'
New-Item -ItemType Directory -Force $packageDir | Out-Null
$executable = Join-Path $projectRoot 'src-tauri/target/release/codex-proxy-launcher.exe'
$pe = [IO.File]::ReadAllBytes($executable)
$offset = [BitConverter]::ToInt32($pe, 0x3c)
if ([BitConverter]::ToUInt16($pe, $offset + 4) -ne 0x8664 -or
    [BitConverter]::ToUInt16($pe, $offset + 24 + 68) -ne 2) { throw 'Expected an x64 Windows GUI executable' }
Copy-Item -LiteralPath $executable -Destination $packageDir -Force
$files = @(Join-Path $packageDir 'codex-proxy-launcher.exe')
if ($gnu) {
    Copy-Item -LiteralPath (Join-Path $projectRoot 'src-tauri/target/release/WebView2Loader.dll') -Destination $packageDir -Force
    $files += Join-Path $packageDir 'WebView2Loader.dll'
}
$asset = Join-Path $releaseDir "Codex-Proxy-Launcher-v$version-windows-x64.zip"
Compress-Archive -LiteralPath $files -DestinationPath $asset -Force
Get-FileHash -LiteralPath $asset -Algorithm SHA256
Write-Output "Windows release asset: $asset"
