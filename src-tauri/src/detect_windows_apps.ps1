$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

function Get-PackageExecutables([string]$root) {
    $manifestPath = Join-Path $root 'AppxManifest.xml'
    if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
        # The package name and entry-point filename need not match. In particular,
        # Codex packages can contain a helper Codex.exe but launch ChatGPT.exe.
        $settings = [System.Xml.XmlReaderSettings]::new()
        $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
        $settings.XmlResolver = $null
        $reader = [System.Xml.XmlReader]::Create($manifestPath, $settings)
        try {
            $manifest = [System.Xml.XmlDocument]::new()
            $manifest.XmlResolver = $null
            $manifest.Load($reader)
        } finally { $reader.Dispose() }
        if ($manifest.Package.Identity.Name -notin @('OpenAI.Codex', 'OpenAI.ChatGPT')) { return }
        $prefix = [IO.Path]::GetFullPath($root).TrimEnd('\') + '\'
        foreach ($application in $manifest.Package.Applications.Application) {
            $relative = [string]$application.Executable
            if ([string]::IsNullOrWhiteSpace($relative) -or [IO.Path]::IsPathRooted($relative)) { continue }
            $path = [IO.Path]::GetFullPath((Join-Path $root $relative))
            if ($path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -and
                [IO.Path]::GetFileName($path) -in @('Codex.exe', 'ChatGPT.exe') -and
                (Test-Path -LiteralPath $path -PathType Leaf)) { $path }
        }
        return
    }
    foreach ($relative in @('app\Codex.exe', 'Codex.exe', 'app\ChatGPT.exe', 'ChatGPT.exe')) {
        $path = Join-Path $root $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) { $path }
    }
}

$candidates = @(if ($env:CODEX_SELECTED_APP_PATH) {
    $directory = Split-Path -Parent $env:CODEX_SELECTED_APP_PATH
    foreach ($root in @($directory, (Split-Path -Parent $directory))) {
        if ($root -and (Test-Path -LiteralPath (Join-Path $root 'AppxManifest.xml') -PathType Leaf)) {
            Get-PackageExecutables $root
            break
        }
    }
} else {
    foreach ($name in @('OpenAI.Codex', 'OpenAI.ChatGPT')) {
        foreach ($package in (Get-AppxPackage -Name $name)) {
            if ([string]::IsNullOrWhiteSpace($package.InstallLocation)) { continue }
            Get-PackageExecutables $package.InstallLocation
        }
    }
})
ConvertTo-Json -InputObject $candidates -Compress
