# Resolve directory junctions and Win32 extended path prefixes before comparing.
# File names alone would also match Codex's unrelated CLI/app-server processes.
if (-not ('CodexProxyFilePath' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class CodexProxyFilePath {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern SafeFileHandle CreateFileW(string path, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern uint GetFinalPathNameByHandleW(SafeFileHandle handle, StringBuilder path, uint size, uint flags);
    public static string Resolve(string path) {
        if (String.IsNullOrEmpty(path)) return null;
        using (var handle = CreateFileW(path, 0, 7, IntPtr.Zero, 3, 0, IntPtr.Zero)) {
            if (handle.IsInvalid) return null;
            var buffer = new StringBuilder(32768);
            uint size = GetFinalPathNameByHandleW(handle, buffer, (uint)buffer.Capacity, 0);
            return size > 0 && size < buffer.Capacity ? buffer.ToString() : null;
        }
    }
}
'@
}
function Get-CodexProcesses([string]$executable = $CodexExecutable) {
    $target = [CodexProxyFilePath]::Resolve($executable)
    if (-not $target) { return }
    $name = [IO.Path]::GetFileNameWithoutExtension($executable)
    Get-Process -Name $name -ErrorAction SilentlyContinue | ForEach-Object {
        try {
            $actual = [CodexProxyFilePath]::Resolve($_.Path)
            if ($actual -and [string]::Equals($actual, $target, [StringComparison]::OrdinalIgnoreCase)) { $_ }
        } catch {}
    }
}
