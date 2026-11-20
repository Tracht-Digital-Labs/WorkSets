using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;
using WorkSets.Core;

namespace WorkSets.Windows;

public sealed class WindowsWorkspaceService
{
    private readonly int currentProcessId = Environment.ProcessId;

    public static void EnablePerMonitorDpi()
    {
        _ = NativeInterop.SetProcessDpiAwarenessContext(NativeInterop.DpiAwarenessContextPerMonitorAwareV2);
    }

    public async Task<WorkspaceSnapshot> CaptureAsync(IDesktopService desktops, CancellationToken cancellationToken = default)
    {
        DesktopSnapshot desktopSnapshot;
        try { desktopSnapshot = await desktops.SnapshotAsync(cancellationToken).ConfigureAwait(false); }
        catch (Exception) when (!cancellationToken.IsCancellationRequested)
        {
            desktopSnapshot = new DesktopSnapshot(1, 0);
        }
        var captured = Capture(
            desktopSnapshot.Count,
            desktopSnapshot.CurrentIndex,
            desktops.Status.CanMutate,
            desktops.Status.Experimental);
        var windows = new List<CapturedWindow>(captured.Windows.Count);
        foreach (var window in captured.Windows)
        {
            int? index = null;
            try { index = await desktops.WindowDesktopIndexAsync(window.NativeHandle, cancellationToken).ConfigureAwait(false); }
            catch (Exception) when (!cancellationToken.IsCancellationRequested) { }
            windows.Add(window with { DesktopIndex = index });
        }
        return captured with { Windows = windows };
    }

    public WorkspaceSnapshot Capture(int desktopCount = 1, int currentDesktopIndex = 0, bool desktopMutationAvailable = false, bool desktopMutationExperimental = false)
    {
        var monitorsByHandle = EnumerateMonitors();
        var windows = new List<CapturedWindow>();
        var callback = new NativeInterop.EnumWindowsProc((handle, _) =>
        {
            var captured = CaptureWindow(handle, monitorsByHandle);
            if (captured is not null) windows.Add(captured);
            return true;
        });
        if (!NativeInterop.EnumWindows(callback, 0))
            throw new InvalidOperationException($"EnumWindows failed with Win32 error {Marshal.GetLastWin32Error()}.");
        GC.KeepAlive(callback);
        return new WorkspaceSnapshot(monitorsByHandle.Values.DistinctBy(monitor => monitor.Id).ToArray(), windows, desktopCount, currentDesktopIndex, desktopMutationAvailable, desktopMutationExperimental);
    }

    public void Place(CapturedWindow window, WindowPlacementSpec placement, IReadOnlyList<MonitorRef> monitors)
    {
        if (!NativeInterop.IsWindow(window.NativeHandle)) throw new InvalidOperationException("The target window no longer exists.");
        var mapped = PresetPlanner.MapPlacement(placement, monitors);
        _ = NativeInterop.ShowWindowAsync(window.NativeHandle, 9); // SW_RESTORE before positioning
        if (!NativeInterop.SetWindowPos(
            window.NativeHandle, 0, mapped.Rectangle.X, mapped.Rectangle.Y,
            mapped.Rectangle.Width, mapped.Rectangle.Height,
            NativeInterop.SwpNoActivate | NativeInterop.SwpNoZOrder))
            throw new InvalidOperationException($"SetWindowPos failed with Win32 error {Marshal.GetLastWin32Error()}.");
        _ = NativeInterop.ShowWindowAsync(window.NativeHandle, placement.State switch
        {
            WindowState.Maximized => 3,
            WindowState.Minimized => 6,
            _ => 9,
        });
    }

    public bool RequestClose(CapturedWindow window)
    {
        if (!window.Manageable || !NativeInterop.IsWindow(window.NativeHandle)) return false;
        return NativeInterop.PostMessageW(window.NativeHandle, NativeInterop.WmClose, 0, 0);
    }

    public bool IsHung(nint handle) => NativeInterop.IsWindow(handle) && NativeInterop.IsHungAppWindow(handle);

    public static bool ForceTerminate(uint processId)
    {
        var process = NativeInterop.OpenProcess(0x0001, false, processId); // PROCESS_TERMINATE
        if (process == 0) return false;
        try { return NativeInterop.TerminateProcess(process, 1); }
        finally { NativeInterop.CloseHandle(process); }
    }

    public Process? Launch(LaunchItem item)
    {
        var target = item.LaunchTarget;
        return target.Kind switch
        {
            LaunchTargetKind.Executable => LaunchExecutable(target, item.Placement.State),
            LaunchTargetKind.File => Process.Start(new ProcessStartInfo(target.Path!) { UseShellExecute = true }),
            LaunchTargetKind.Url => Process.Start(new ProcessStartInfo(target.Url!) { UseShellExecute = true }),
            LaunchTargetKind.PackagedApp => PackagedAppActivator.Activate(target.Aumid!, target.Arguments),
            _ => throw new ArgumentOutOfRangeException(nameof(item)),
        };
    }

    private static Process LaunchExecutable(LaunchTarget target, WindowState state)
    {
        var start = new ProcessStartInfo(target.Path!)
        {
            UseShellExecute = false,
            WorkingDirectory = target.WorkingDirectory ?? Path.GetDirectoryName(target.Path!) ?? Environment.CurrentDirectory,
            WindowStyle = state == WindowState.Minimized ? ProcessWindowStyle.Minimized : ProcessWindowStyle.Normal,
        };
        foreach (var argument in target.Arguments) start.ArgumentList.Add(argument);
        return Process.Start(start) ?? throw new InvalidOperationException($"Windows did not start {target.Path}.");
    }

    private CapturedWindow? CaptureWindow(nint handle, IReadOnlyDictionary<nint, MonitorRef> monitors)
    {
        if (!NativeInterop.IsWindowVisible(handle)) return null;
        var title = ReadWindowText(handle);
        var className = ReadClassName(handle);
        if (string.IsNullOrWhiteSpace(title) || IsSystemWindow(className)) return null;
        if ((NativeInterop.GetWindowLongW(handle, NativeInterop.GwlExStyle) & NativeInterop.WsExToolWindow) != 0) return null;
        NativeInterop.GetWindowThreadProcessId(handle, out var processId);
        if (processId == 0 || processId == currentProcessId) return null;
        var process = NativeInterop.OpenProcess(NativeInterop.ProcessQueryLimitedInformation, false, processId);
        string? executablePath = null;
        string? aumid = null;
        var elevated = false;
        var manageable = process != 0;
        if (process != 0)
        {
            try
            {
                executablePath = ReadProcessPath(process);
                aumid = ReadAumid(process);
                elevated = ReadElevation(process) ?? false;
                manageable = !elevated;
            }
            finally { NativeInterop.CloseHandle(process); }
        }
        var monitorHandle = NativeInterop.MonitorFromWindow(handle, NativeInterop.MonitorDefaultToNearest);
        if (!monitors.TryGetValue(monitorHandle, out var monitor)) monitor = monitors.Values.FirstOrDefault(candidate => candidate.Primary) ?? monitors.Values.First();
        var placement = ReadPlacement(handle, monitor);
        var name = executablePath is null ? title : FileVersionInfo.GetVersionInfo(executablePath).FileDescription;
        if (string.IsNullOrWhiteSpace(name)) name = executablePath is null ? title : Path.GetFileNameWithoutExtension(executablePath);
        return new CapturedWindow
        {
            NativeHandle = handle,
            ProcessId = processId,
            DisplayName = name,
            ExecutablePath = executablePath,
            Aumid = aumid,
            WindowClass = className,
            Title = title,
            Placement = placement,
            DesktopIndex = 0,
            Manageable = manageable,
            Elevated = elevated,
            IconDataUrl = ReadIconDataUrl(executablePath),
        };
    }

    private static Dictionary<nint, MonitorRef> EnumerateMonitors()
    {
        var identities = DisplayIdentityMap();
        var monitors = new Dictionary<nint, MonitorRef>();
        var callback = new NativeInterop.MonitorEnumProc((nint handle, nint hdc, ref NativeInterop.NativeRect rectangle, nint data) =>
        {
            _ = hdc;
            _ = rectangle;
            _ = data;
            var info = new NativeInterop.MonitorInfoEx { Size = (uint)Marshal.SizeOf<NativeInterop.MonitorInfoEx>(), DeviceName = string.Empty };
            if (!NativeInterop.GetMonitorInfoW(handle, ref info)) return true;
            _ = NativeInterop.GetDpiForMonitor(handle, 0, out var dpiX, out var dpiY);
            if (dpiX == 0) dpiX = 96;
            if (dpiY == 0) dpiY = 96;
            var device = info.DeviceName.TrimEnd('\0');
            identities.TryGetValue(device, out var identity);
            var id = identity?.Id ?? $"monitor-{StableHash(device):x16}";
            var friendly = string.IsNullOrWhiteSpace(identity?.FriendlyName) ? device : identity.FriendlyName;
            monitors[handle] = new MonitorRef(
                id, device, friendly,
                ToRect(info.Monitor), ToRect(info.WorkArea), dpiX, dpiY,
                (info.Flags & 1) != 0);
            return true;
        });
        if (!NativeInterop.EnumDisplayMonitors(0, 0, callback, 0))
            throw new InvalidOperationException($"EnumDisplayMonitors failed with Win32 error {Marshal.GetLastWin32Error()}.");
        GC.KeepAlive(callback);
        if (monitors.Count == 0) throw new InvalidOperationException("Windows did not report an active monitor.");
        return monitors;
    }

    private static Dictionary<string, MonitorIdentity> DisplayIdentityMap()
    {
        var result = new Dictionary<string, MonitorIdentity>(StringComparer.OrdinalIgnoreCase);
        if (NativeInterop.GetDisplayConfigBufferSizes(NativeInterop.QdcOnlyActivePaths, out var pathCount, out var modeCount) != 0) return result;
        var paths = new NativeInterop.DisplayConfigPathInfo[pathCount];
        var modes = new NativeInterop.DisplayConfigModeInfo[modeCount];
        if (NativeInterop.QueryDisplayConfig(NativeInterop.QdcOnlyActivePaths, ref pathCount, paths, ref modeCount, modes, 0) != 0) return result;
        foreach (var path in paths.Take((int)pathCount))
        {
            var source = new NativeInterop.DisplayConfigSourceDeviceName
            {
                Header = new NativeInterop.DisplayConfigDeviceInfoHeader
                {
                    Type = NativeInterop.DisplayConfigDeviceInfoGetSourceName,
                    Size = (uint)Marshal.SizeOf<NativeInterop.DisplayConfigSourceDeviceName>(),
                    AdapterId = path.SourceInfo.AdapterId,
                    Id = path.SourceInfo.Id,
                },
                ViewGdiDeviceName = string.Empty,
            };
            var target = new NativeInterop.DisplayConfigTargetDeviceName
            {
                Header = new NativeInterop.DisplayConfigDeviceInfoHeader
                {
                    Type = NativeInterop.DisplayConfigDeviceInfoGetTargetName,
                    Size = (uint)Marshal.SizeOf<NativeInterop.DisplayConfigTargetDeviceName>(),
                    AdapterId = path.TargetInfo.AdapterId,
                    Id = path.TargetInfo.Id,
                },
                MonitorFriendlyDeviceName = string.Empty,
                MonitorDevicePath = string.Empty,
            };
            if (NativeInterop.DisplayConfigGetSourceDeviceInfo(ref source) != 0 || NativeInterop.DisplayConfigGetTargetDeviceInfo(ref target) != 0) continue;
            var device = source.ViewGdiDeviceName.TrimEnd('\0');
            var pathId = target.MonitorDevicePath.TrimEnd('\0');
            if (device.Length == 0 || pathId.Length == 0) continue;
            result[device] = new MonitorIdentity($"monitor-{StableHash(pathId):x16}", target.MonitorFriendlyDeviceName.TrimEnd('\0'));
        }
        return result;
    }

    private static WindowPlacementSpec ReadPlacement(nint handle, MonitorRef monitor)
    {
        var placement = new NativeInterop.WindowPlacement { Length = (uint)Marshal.SizeOf<NativeInterop.WindowPlacement>() };
        NativeInterop.NativeRect native;
        uint show = 1;
        if (NativeInterop.GetWindowPlacement(handle, ref placement))
        {
            native = placement.NormalPosition;
            show = placement.ShowCommand;
        }
        else if (!NativeInterop.GetWindowRect(handle, out native))
            throw new InvalidOperationException($"Unable to capture window geometry (Win32 {Marshal.GetLastWin32Error()}).");
        var workspace = ToRect(native);
        var screen = WindowGeometry.WorkspaceToScreen(workspace, monitor);
        var state = show switch
        {
            2 or 6 or 7 => WindowState.Minimized,
            3 => WindowState.Maximized,
            _ => WindowState.Normal,
        };
        return WindowGeometry.ToPlacement(screen, monitor, state);
    }

    private static string ReadWindowText(nint handle)
    {
        var length = NativeInterop.GetWindowTextLengthW(handle);
        if (length <= 0) return string.Empty;
        var buffer = new StringBuilder(length + 1);
        _ = NativeInterop.GetWindowTextW(handle, buffer, buffer.Capacity);
        return buffer.ToString();
    }

    private static string ReadClassName(nint handle)
    {
        var buffer = new StringBuilder(256);
        _ = NativeInterop.GetClassNameW(handle, buffer, buffer.Capacity);
        return buffer.ToString();
    }

    private static string? ReadProcessPath(nint process)
    {
        var capacity = 32_768u;
        var buffer = new StringBuilder((int)capacity);
        return NativeInterop.QueryFullProcessImageNameW(process, 0, buffer, ref capacity) ? buffer.ToString() : null;
    }

    private static string? ReadAumid(nint process)
    {
        uint length = 0;
        var first = NativeInterop.GetApplicationUserModelId(process, ref length, null);
        if (first != (int)NativeInterop.ErrorInsufficientBuffer || length is 0 or > 4096) return null;
        var buffer = new StringBuilder((int)length);
        return NativeInterop.GetApplicationUserModelId(process, ref length, buffer) == 0 ? buffer.ToString() : null;
    }

    private static bool? ReadElevation(nint process)
    {
        if (!NativeInterop.OpenProcessToken(process, NativeInterop.TokenQuery, out var token)) return null;
        try
        {
            return NativeInterop.GetTokenInformation(
                token, NativeInterop.TokenElevationClass, out var elevation,
                (uint)Marshal.SizeOf<NativeInterop.TokenElevation>(), out _)
                ? elevation.TokenIsElevated != 0 : null;
        }
        finally { NativeInterop.CloseHandle(token); }
    }

    private static string? ReadIconDataUrl(string? executablePath)
    {
        if (string.IsNullOrWhiteSpace(executablePath) || !File.Exists(executablePath)) return null;
        try
        {
            using var icon = Icon.ExtractAssociatedIcon(executablePath);
            if (icon is null) return null;
            using var bitmap = icon.ToBitmap();
            using var stream = new MemoryStream();
            bitmap.Save(stream, ImageFormat.Png);
            return stream.Length <= 128 * 1024 ? $"data:image/png;base64,{Convert.ToBase64String(stream.ToArray())}" : null;
        }
        catch (Exception) when (executablePath.Length < 32_768) { return null; }
    }

    private static bool IsSystemWindow(string className) => className is
        "Shell_TrayWnd" or "Shell_SecondaryTrayWnd" or "Progman" or "WorkerW" or "DV2ControlHost" or "Windows.UI.Core.CoreWindow";

    private static RectI32 ToRect(NativeInterop.NativeRect rectangle) =>
        new(rectangle.Left, rectangle.Top, rectangle.Right - rectangle.Left, rectangle.Bottom - rectangle.Top);

    private static ulong StableHash(string value)
    {
        var hash = 14695981039346656037UL;
        foreach (var valueByte in Encoding.UTF8.GetBytes(value.ToLowerInvariant()))
            hash = (hash ^ valueByte) * 1099511628211UL;
        return hash;
    }

    private sealed record MonitorIdentity(string Id, string FriendlyName);
}
