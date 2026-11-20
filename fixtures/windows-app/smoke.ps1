[CmdletBinding()]
param(
    [string]$Binary = (Join-Path $PSScriptRoot "target\release\worksets-window-fixture.exe")
)

$ErrorActionPreference = "Stop"
$fixture = (Resolve-Path -LiteralPath $Binary).Path
$fixtureRoot = (Resolve-Path -LiteralPath $PSScriptRoot).Path
if (-not $fixture.StartsWith($fixtureRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Fixture binary must be located below $fixtureRoot"
}

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class WorkSetsFixtureNative {
    public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr parameter);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hwnd);

    [DllImport("user32.dll")]
    public static extern bool IsHungAppWindow(IntPtr hwnd);

    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int width, int height, uint flags);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hwnd, out Rect rectangle);

    [StructLayout(LayoutKind.Sequential)]
    public struct Rect {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    public static IntPtr[] WindowsForProcess(uint processId) {
        var windows = new List<IntPtr>();
        EnumWindows((window, _) => {
            GetWindowThreadProcessId(window, out var candidate);
            if (candidate == processId && IsWindowVisible(window)) windows.Add(window);
            return true;
        }, IntPtr.Zero);
        return windows.ToArray();
    }
}
"@

function Wait-ForWindow {
    param(
        [System.Diagnostics.Process]$Process,
        [int]$MinimumCount = 1,
        [int]$TimeoutMilliseconds = 5000
    )
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $windows = [WorkSetsFixtureNative]::WindowsForProcess([uint32]$Process.Id)
        if ($windows.Count -ge $MinimumCount) { return $windows }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline -and -not $Process.HasExited)
    throw "Expected $MinimumCount window(s) for PID $($Process.Id), found $($windows.Count)."
}

function Stop-FixtureProcess {
    param([System.Diagnostics.Process]$Process)
    if (-not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force
        $Process.WaitForExit(5000) | Out-Null
    }
}

function Start-Fixture {
    param([string]$Mode)
    Start-Process -FilePath $fixture -ArgumentList $Mode -PassThru
}

$results = [System.Collections.Generic.List[object]]::new()

$process = Start-Fixture "normal"
try {
    $windows = Wait-ForWindow $process
    $results.Add([pscustomobject]@{ Mode = "normal"; Result = "passed"; Detail = "$($windows.Count) window" })
} finally { Stop-FixtureProcess $process }

$process = Start-Fixture "delayed"
try {
    Start-Sleep -Milliseconds 350
    $early = [WorkSetsFixtureNative]::WindowsForProcess([uint32]$process.Id)
    if ($early.Count -ne 0) { throw "Delayed mode created a window too early." }
    $windows = Wait-ForWindow $process -TimeoutMilliseconds 4000
    $results.Add([pscustomobject]@{ Mode = "delayed"; Result = "passed"; Detail = "window appeared after delay" })
} finally { Stop-FixtureProcess $process }

$process = Start-Fixture "multiple"
try {
    $windows = Wait-ForWindow $process -MinimumCount 2
    $results.Add([pscustomobject]@{ Mode = "multiple"; Result = "passed"; Detail = "$($windows.Count) windows" })
} finally { Stop-FixtureProcess $process }

$process = Start-Fixture "minimum-size"
try {
    $windows = Wait-ForWindow $process
    [WorkSetsFixtureNative]::SetWindowPos($windows[0], [IntPtr]::Zero, 100, 100, 200, 150, 0x0014) | Out-Null
    Start-Sleep -Milliseconds 150
    $rectangle = New-Object WorkSetsFixtureNative+Rect
    [WorkSetsFixtureNative]::GetWindowRect($windows[0], [ref]$rectangle) | Out-Null
    $width = $rectangle.Right - $rectangle.Left
    $height = $rectangle.Bottom - $rectangle.Top
    if ($width -lt 640 -or $height -lt 420) { throw "Minimum size was not enforced: ${width}x${height}." }
    $results.Add([pscustomobject]@{ Mode = "minimum-size"; Result = "passed"; Detail = "${width}x${height}" })
} finally { Stop-FixtureProcess $process }

$process = Start-Fixture "save-dialog"
try {
    $windows = Wait-ForWindow $process
    [WorkSetsFixtureNative]::PostMessage($windows[0], 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 250
    if ($process.HasExited) { throw "Save-dialog mode closed without asking." }
    $results.Add([pscustomobject]@{ Mode = "save-dialog"; Result = "passed"; Detail = "WM_CLOSE was intercepted" })
} finally { Stop-FixtureProcess $process }

$process = Start-Fixture "hung"
try {
    $windows = Wait-ForWindow $process
    Start-Sleep -Seconds 6
    if (-not [WorkSetsFixtureNative]::IsHungAppWindow($windows[0])) { throw "Hung mode was not detected by Windows." }
    $results.Add([pscustomobject]@{ Mode = "hung"; Result = "passed"; Detail = "IsHungAppWindow=true" })
} finally { Stop-FixtureProcess $process }

$process = Start-Fixture "single-instance"
$second = $null
try {
    Wait-ForWindow $process | Out-Null
    $second = Start-Fixture "single-instance"
    if (-not $second.WaitForExit(3000)) { throw "Second single-instance process did not exit." }
    if ($process.HasExited) { throw "First single-instance process exited unexpectedly." }
    $results.Add([pscustomobject]@{ Mode = "single-instance"; Result = "passed"; Detail = "second launch activated first" })
} finally {
    if ($null -ne $second) { Stop-FixtureProcess $second }
    Stop-FixtureProcess $process
}

$results | Format-Table -AutoSize

