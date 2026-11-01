# Starts a native app, waits, optionally drives it, saves a screenshot of its window, and closes it:
#   window-shot.ps1 -Exe <path> [-Out <png>] [-Wait 5] [-AppArgs <args>] [-AppHome <folder>] [-Steps <step>, ...]
# AppHome keeps a standalone app's projects out of Documents (CANOPY_APP_HOME).
# Steps run in order once the window is up. Coordinates are pixels from the window's top-left corner:
#   key:<SendKeys keys>   e.g. "key: " (Space), "key:^z" (Ctrl+Z), "key:^{TAB}", "key:{DEL}"
#   click:<x>,<y>    rclick:<x>,<y>    dclick:<x>,<y>    drag:<x1>,<y1>,<x2>,<y2>
#   wait:<ms>        shot:<png>
# Keys and clicks only go out while the app's window is in front, so they never land in another window.
param([string]$Exe, [string]$Out = "", [int]$Wait = 5, [string[]]$AppArgs = @(), [string]$AppHome = "", [string[]]$Steps = @())
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Win {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
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

function Get-Rect {
  $rect = New-Object Win+RECT
  [Win]::GetWindowRect($handle, [ref]$rect) | Out-Null
  $rect
}
function Assert-InFront {
  if ([Win]::GetForegroundWindow() -ne $handle) {
    [Win]::SetForegroundWindow($handle) | Out-Null
    Start-Sleep -Milliseconds 300
    if ([Win]::GetForegroundWindow() -ne $handle) { throw "The app's window isn't in front; not sending input" }
  }
}
function Move-To([int]$x, [int]$y) {
  $rect = Get-Rect
  [Win]::SetCursorPos($rect.Left + $x, $rect.Top + $y) | Out-Null
  Start-Sleep -Milliseconds 60
}
function Press([uint32]$down, [uint32]$up) {
  [Win]::mouse_event($down, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 40
  [Win]::mouse_event($up, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
}
function Save-Shot([string]$path) {
  $rect = Get-Rect
  $width = $rect.Right - $rect.Left; $height = $rect.Bottom - $rect.Top
  $bitmap = New-Object System.Drawing.Bitmap $width, $height
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
  $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $graphics.Dispose(); $bitmap.Dispose()
  Write-Host "Saved $path ($width x $height)"
}

try {
  foreach ($step in $Steps) {
    $colon = $step.IndexOf(":")
    $kind = $step.Substring(0, $colon); $value = $step.Substring($colon + 1)
    $n = @($value -split "," | ForEach-Object { if ($_ -match '^-?\d+$') { [int]$_ } })
    switch ($kind) {
      "key" { Assert-InFront; [System.Windows.Forms.SendKeys]::SendWait($value); Start-Sleep -Milliseconds 120 }
      "click" { Assert-InFront; Move-To $n[0] $n[1]; Press 0x0002 0x0004 }
      "rclick" { Assert-InFront; Move-To $n[0] $n[1]; Press 0x0008 0x0010 }
      "dclick" { Assert-InFront; Move-To $n[0] $n[1]; Press 0x0002 0x0004; Press 0x0002 0x0004 }
      "drag" {
        Assert-InFront; Move-To $n[0] $n[1]
        [Win]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        for ($i = 1; $i -le 12; $i++) { Move-To ($n[0] + ($n[2] - $n[0]) * $i / 12) ($n[1] + ($n[3] - $n[1]) * $i / 12) }
        Start-Sleep -Milliseconds 150
        [Win]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 150
      }
      "wait" { Start-Sleep -Milliseconds $n[0] }
      "shot" { Save-Shot $value }
      default { throw "Unknown step: $step" }
    }
  }
  if ($Out) { Save-Shot $Out }
} finally {
  $process.CloseMainWindow() | Out-Null
  if (-not $process.WaitForExit(10000)) { Write-Host "The app didn't close in 10 s" }
}
