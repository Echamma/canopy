# Starts a native app, waits, saves a screenshot of its window, and closes it:
#   window-shot.ps1 -Exe <path> -Out <png> [-Wait 5] [-AppArgs <args>] [-AppHome <folder>]
# AppHome keeps a standalone app's projects out of Documents (CANOPY_APP_HOME).
param([string]$Exe, [string]$Out, [int]$Wait = 5, [string[]]$AppArgs = @(), [string]$AppHome = "")
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Win {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[Win]::SetProcessDPIAware() | Out-Null
if ($AppHome) { $env:CANOPY_APP_HOME = $AppHome }
$process = if ($AppArgs.Count) { Start-Process -FilePath $Exe -ArgumentList ($AppArgs | ForEach-Object { '"' + $_ + '"' }) -PassThru } else { Start-Process -FilePath $Exe -PassThru }
Start-Sleep -Seconds $Wait
$process.Refresh()
$handle = $process.MainWindowHandle
if ($handle -eq [IntPtr]::Zero) { throw "No window appeared" }
[Win]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 600
$rect = New-Object Win+RECT
[Win]::GetWindowRect($handle, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left; $height = $rect.Bottom - $rect.Top
$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
$bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$process.CloseMainWindow() | Out-Null
$process.WaitForExit(10000) | Out-Null
Write-Host "Saved $Out ($width x $height)"
