param(
    [string]$Binary = (Join-Path $PSScriptRoot '..\..\target\release\rustdm-ui.exe'),
    [string]$StateDirectory = (Join-Path $PSScriptRoot ("..\..\target\tray-smoke\" + [guid]::NewGuid()))
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'This smoke test requires an interactive Windows desktop.' }
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class RustDmTraySmoke {
    public delegate bool EnumProc(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    public static IntPtr Find(uint process, string className) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            var name = new StringBuilder(256); GetClassName(window, name, name.Capacity);
            var title = new StringBuilder(256); GetWindowText(window, title, title.Capacity);
            bool matches = className == "RustDM main window" ? title.ToString() == "RustDM" : name.ToString() == className;
            if (owner == process && matches) { found = window; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@

function Wait-Until([scriptblock]$Condition, [string]$Failure) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (!( & $Condition )) {
        if ([DateTime]::UtcNow -ge $deadline) { throw $Failure }
        Start-Sleep -Milliseconds 50
    }
}
function Open-TrayMenu {
    [RustDmTraySmoke]::SetCursorPos(700, 500) | Out-Null
    # The notification callback used by tray-icon 0.26. Send the same right-button
    # release that Windows delivers, including when the icon is in the overflow.
    [RustDmTraySmoke]::PostMessage($script:trayWindow, 6002, [IntPtr]::Zero, [IntPtr]0x205) | Out-Null
    Wait-Until { [RustDmTraySmoke]::Find($script:preview.Id, '#32768') -ne [IntPtr]::Zero } 'Tray menu did not open'
    Start-Sleep -Milliseconds 200
}
function Select-TrayMenuItem([bool]$Last) {
    $menu = [RustDmTraySmoke]::Find($script:preview.Id, '#32768')
    $bounds = New-Object RustDmTraySmoke+Rect
    [RustDmTraySmoke]::GetWindowRect($menu, [ref]$bounds) | Out-Null
    $row = if ($Last) { $bounds.Bottom - 12 } else { $bounds.Top + 12 }
    [RustDmTraySmoke]::SetCursorPos(($bounds.Left + 70), $row) | Out-Null
    [RustDmTraySmoke]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    [RustDmTraySmoke]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
}

$Binary = (Resolve-Path -LiteralPath $Binary).Path
New-Item -ItemType Directory -Path $StateDirectory -Force | Out-Null
$StateDirectory = (Resolve-Path -LiteralPath $StateDirectory).Path
if (Test-Path -LiteralPath (Join-Path $StateDirectory 'preferences.json')) {
    throw 'Choose a new state directory; smoke tests never replace existing preferences.'
}
[IO.File]::WriteAllText((Join-Path $StateDirectory 'preferences.json'), '{"version":1,"close_to_tray":true}', (New-Object Text.UTF8Encoding($false)))
$script:preview = Start-Process -FilePath $Binary -ArgumentList @('--state-dir', ('"' + $StateDirectory + '"')) -WindowStyle Normal -RedirectStandardError (Join-Path $StateDirectory 'desktop-stderr.txt') -PassThru
$null = $script:preview.Handle
try {
    # Process.MainWindowHandle can select winit's internal event target during startup.
    Wait-Until { [RustDmTraySmoke]::Find($script:preview.Id, 'RustDM main window') -ne [IntPtr]::Zero } 'Desktop did not open'
    $window = [RustDmTraySmoke]::Find($script:preview.Id, 'RustDM main window')
    Wait-Until { [RustDmTraySmoke]::IsWindowVisible($window) } 'Desktop root was never visible'
    Start-Sleep -Milliseconds 1000
    Wait-Until { [RustDmTraySmoke]::Find($script:preview.Id, 'tray_icon_app') -ne [IntPtr]::Zero } 'Native tray adapter was not created'
    $script:trayWindow = [RustDmTraySmoke]::Find($script:preview.Id, 'tray_icon_app')
    [RustDmTraySmoke]::PostMessage($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Wait-Until { ![RustDmTraySmoke]::IsWindowVisible($window) } 'Close-to-tray did not hide the window'
    if ($script:preview.HasExited) { throw 'Close-to-tray exited the application' }
    [RustDmTraySmoke]::PostMessage($script:trayWindow, 6002, [IntPtr]::Zero, [IntPtr]0x202) | Out-Null
    Wait-Until { [RustDmTraySmoke]::IsWindowVisible($window) } 'Tray click did not restore the hidden window'

    [RustDmTraySmoke]::PostMessage($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Wait-Until { ![RustDmTraySmoke]::IsWindowVisible($window) } 'Second close did not hide the window'
    Open-TrayMenu
    $menu = [RustDmTraySmoke]::Find($script:preview.Id, '#32768')
    $bounds = New-Object RustDmTraySmoke+Rect
    [RustDmTraySmoke]::GetWindowRect($menu, [ref]$bounds) | Out-Null
    $bitmap = New-Object Drawing.Bitmap ($bounds.Right-$bounds.Left), ($bounds.Bottom-$bounds.Top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $StateDirectory 'tray-menu.png'), [Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    Select-TrayMenuItem $false
    Wait-Until { [RustDmTraySmoke]::IsWindowVisible($window) } 'Show RustDM menu action did not restore the window'

    [RustDmTraySmoke]::PostMessage($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Wait-Until { ![RustDmTraySmoke]::IsWindowVisible($window) } 'Third close did not hide the window'
    Open-TrayMenu
    Select-TrayMenuItem $true
    if (!$script:preview.WaitForExit(10000)) { throw 'Quit from the hidden tray menu did not exit' }
    $script:preview.Refresh()
    if ($script:preview.ExitCode -ne 0) { throw "Desktop exited with $($script:preview.ExitCode)" }
    if ([RustDmTraySmoke]::Find($script:preview.Id, 'tray_icon_app') -ne [IntPtr]::Zero) { throw 'Tray window survived application exit' }
    Write-Output "PASS: close-to-tray, hidden click/menu restore, hidden quit, native cleanup. Evidence: $StateDirectory"
} finally {
    if (!$script:preview.HasExited) {
        # Clean up only the process this smoke test started after a failed assertion.
        Stop-Process -Id $script:preview.Id
    }
}
