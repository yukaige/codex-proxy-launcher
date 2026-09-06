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

# Compile once, discover the exact test executables from Cargo, then run them.
# GNU test harnesses need the same Common Controls v6 manifest as the GUI EXE.
$messages = & cargo test --manifest-path src-tauri/Cargo.toml --no-run --message-format=json
if ($LASTEXITCODE -ne 0) { throw 'Test compilation failed' }
$testBinaries = @($messages | ForEach-Object {
    $item = $_ | ConvertFrom-Json
    if ($item.reason -eq 'compiler-artifact' -and $item.profile.test -and $item.executable) {
        $item.executable
    }
})
if (-not $testBinaries.Count) { throw 'Cargo returned no test executables' }
if ($gnu) {
    if (-not ('CodexTestManifest' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class CodexTestManifest {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    public static extern IntPtr BeginUpdateResourceW(string path, bool delete);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool UpdateResourceW(IntPtr handle, IntPtr type, IntPtr name, ushort language, byte[] data, uint size);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool EndUpdateResourceW(IntPtr handle, bool discard);
}
'@
    }
    $manifest = '<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0"><dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" /></dependentAssembly></dependency></assembly>'
    $bytes = [Text.Encoding]::UTF8.GetBytes($manifest)
    foreach ($testBinary in $testBinaries) {
        $handle = [CodexTestManifest]::BeginUpdateResourceW($testBinary, $false)
        if ($handle -eq [IntPtr]::Zero) { throw "Cannot open test resource: $testBinary" }
        if (-not [CodexTestManifest]::UpdateResourceW($handle, [IntPtr]24, [IntPtr]1, 0, $bytes, $bytes.Length)) {
            [CodexTestManifest]::EndUpdateResourceW($handle, $true) | Out-Null
            throw "Cannot write test manifest: $testBinary"
        }
        if (-not [CodexTestManifest]::EndUpdateResourceW($handle, $false)) { throw 'Cannot save test manifest' }
    }
}
foreach ($testBinary in $testBinaries) {
    # Tauri places the GNU WebView2 loader next to the debug application.
    $env:PATH = (Split-Path -Parent (Split-Path -Parent $testBinary)) + ';' + $env:PATH
    & $testBinary
    if ($LASTEXITCODE -ne 0) { throw "Tests failed: $testBinary" }
}

& npm.cmd run dist:windows
if ($LASTEXITCODE -ne 0) { throw 'Windows release build failed' }
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
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
