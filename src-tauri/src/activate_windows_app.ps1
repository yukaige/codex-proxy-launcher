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

# Package activation preserves package identity and accepts Chromium switches.
# It does not accept per-process environment variables; the launcher stages
# app-server variables in Codex's .env only while app-server starts.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

[ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IApplicationActivationManager {
    [PreserveSig]
    int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string appUserModelId,
        [MarshalAs(UnmanagedType.LPWStr)] string arguments, uint options, out uint processId);
    [PreserveSig]
    int ActivateForFile([MarshalAs(UnmanagedType.LPWStr)] string appUserModelId, IntPtr itemArray,
        [MarshalAs(UnmanagedType.LPWStr)] string verb, out uint processId);
    [PreserveSig]
    int ActivateForProtocol([MarshalAs(UnmanagedType.LPWStr)] string appUserModelId,
        IntPtr itemArray, out uint processId);
}

[ComImport, Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")]
public class ApplicationActivationManager : IApplicationActivationManager {
    [PreserveSig]
    [MethodImpl(MethodImplOptions.InternalCall, MethodCodeType = MethodCodeType.Runtime)]
    public extern int ActivateApplication(string appUserModelId, string arguments,
        uint options, out uint processId);
    [PreserveSig]
    [MethodImpl(MethodImplOptions.InternalCall, MethodCodeType = MethodCodeType.Runtime)]
    public extern int ActivateForFile(string appUserModelId, IntPtr itemArray,
        string verb, out uint processId);
    [PreserveSig]
    [MethodImpl(MethodImplOptions.InternalCall, MethodCodeType = MethodCodeType.Runtime)]
    public extern int ActivateForProtocol(string appUserModelId, IntPtr itemArray,
        out uint processId);
}
'@
$manager = [IApplicationActivationManager][ApplicationActivationManager]::new()
$activatedPid = [uint32]0
$arguments = [string]$env:CODEX_APP_ARGUMENTS
$hresult = $manager.ActivateApplication($appId, $arguments, 0, [ref]$activatedPid)
if ($hresult -lt 0) { [Runtime.InteropServices.Marshal]::ThrowExceptionForHR($hresult) }
Write-Output $activatedPid
