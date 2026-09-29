$ErrorActionPreference = 'Stop'
$executable = [IO.Path]::GetFullPath($env:CODEX_SELECTED_APP_PATH)
$root = Split-Path -Parent $executable
while ($root -and -not (Test-Path -LiteralPath (Join-Path $root 'AppxManifest.xml') -PathType Leaf)) {
    $parent = Split-Path -Parent $root
    if ($parent -eq $root) { throw '找不到应用程序包清单。' }
    $root = $parent
}
if (-not $root) { throw '找不到应用程序包清单。' }

$settings = [System.Xml.XmlReaderSettings]::new()
$settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
$settings.XmlResolver = $null
$reader = [System.Xml.XmlReader]::Create((Join-Path $root 'AppxManifest.xml'), $settings)
try {
    $manifest = [System.Xml.XmlDocument]::new()
    $manifest.XmlResolver = $null
    $manifest.Load($reader)
} finally { $reader.Dispose() }

$name = [string]$manifest.Package.Identity.Name
if ($name -notin @('OpenAI.Codex', 'OpenAI.ChatGPT')) { throw '所选程序包不是 Codex 或 ChatGPT。' }
$application = $manifest.Package.Applications.Application | Where-Object {
    $relative = [string]$_.Executable
    $relative -and -not [IO.Path]::IsPathRooted($relative) -and
    [string]::Equals([IO.Path]::GetFullPath((Join-Path $root $relative)), $executable,
        [StringComparison]::OrdinalIgnoreCase)
} | Select-Object -First 1
if (-not $application) { throw '所选文件不是程序包声明的应用入口。' }
$package = Get-AppxPackage -Name $name | Select-Object -First 1
if (-not $package) { throw '当前用户没有注册该应用程序包。' }
$appId = '{0}!{1}' -f $package.PackageFamilyName, $application.Id
Start-Process -FilePath 'explorer.exe' -ArgumentList ('shell:AppsFolder\' + $appId)
